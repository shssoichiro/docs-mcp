//! MCP Message Validation
//!
//! This module provides JSON schema validation for MCP protocol messages
//! to ensure protocol compliance and proper error handling.

use crate::mcp::protocol::*;
use anyhow::{Result, anyhow};
use jsonschema::{Draft, JSONSchema};
use serde_json::{Value, json};
use std::collections::HashMap;
use tracing::debug;

/// JSON Schema validator for MCP messages
#[derive(Debug)]
pub struct McpValidator {
    schemas: HashMap<String, JSONSchema>,
}

impl McpValidator {
    /// Create a new MCP validator with built-in schemas
    #[inline]
    pub fn new() -> Result<Self> {
        let mut validator = Self {
            schemas: HashMap::new(),
        };

        // Load built-in schemas
        validator.load_builtin_schemas()?;

        Ok(validator)
    }

    /// Load built-in JSON schemas for MCP message types
    fn load_builtin_schemas(&mut self) -> Result<()> {
        // JSON-RPC Request schema
        let request_schema = json!({
            "type": "object",
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "const": "2.0"
                },
                "method": {"type": "string"},
                "params": {},
                "id": {
                    "oneOf": [
                        {"type": "string"},
                        {"type": "integer"}
                    ]
                }
            },
            "required": ["jsonrpc", "method", "id"]
        });
        self.add_schema("jsonrpc_request", &request_schema)?;

        // JSON-RPC Response schema
        let response_schema = json!({
            "type": "object",
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "const": "2.0"
                },
                "result": {},
                "id": {
                    "oneOf": [
                        {"type": "string"},
                        {"type": "integer"}
                    ]
                }
            },
            "required": ["jsonrpc", "result", "id"]
        });
        self.add_schema("jsonrpc_response", &response_schema)?;

        // JSON-RPC Error Response schema
        let error_response_schema = json!({
            "type": "object",
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "const": "2.0"
                },
                "error": {
                    "type": "object",
                    "properties": {
                        "code": {"type": "integer"},
                        "message": {"type": "string"},
                        "data": {}
                    },
                    "required": ["code", "message"]
                },
                "id": {
                    "oneOf": [
                        {"type": "string"},
                        {"type": "integer"},
                        {"type": "null"}
                    ]
                }
            },
            "required": ["jsonrpc", "error", "id"]
        });
        self.add_schema("jsonrpc_error_response", &error_response_schema)?;

        // JSON-RPC Notification schema
        let notification_schema = json!({
            "type": "object",
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "const": "2.0"
                },
                "method": {"type": "string"},
                "params": {}
            },
            "required": ["jsonrpc", "method"]
        });
        self.add_schema("jsonrpc_notification", &notification_schema)?;

        // MCP Initialize Request schema
        let initialize_schema = json!({
            "type": "object",
            "properties": {
                "protocolVersion": {"type": "string"},
                "capabilities": {
                    "type": "object",
                    "properties": {
                        "experimental": {"type": "object"},
                        "sampling": {"type": "object"}
                    }
                },
                "clientInfo": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "version": {"type": "string"}
                    },
                    "required": ["name", "version"]
                }
            },
            "required": ["protocolVersion", "capabilities", "clientInfo"]
        });
        self.add_schema("initialize_params", &initialize_schema)?;

        // Tool Call Parameters schema
        let tool_call_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "arguments": {"type": "object"}
            },
            "required": ["name"]
        });
        self.add_schema("call_tool_params", &tool_call_schema)?;

        debug!("Loaded {} built-in JSON schemas", self.schemas.len());
        Ok(())
    }

    /// Add a JSON schema to the validator
    #[inline]
    pub fn add_schema(&mut self, name: &str, schema: &Value) -> Result<()> {
        let compiled = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(schema)
            .map_err(|e| anyhow!("Failed to compile schema '{}': {}", name, e))?;

        self.schemas.insert(name.to_string(), compiled);
        Ok(())
    }

    /// Validate a JSON-RPC message
    #[inline]
    pub fn validate_message(&self, message: &JsonRpcMessage) -> Result<()> {
        match message {
            JsonRpcMessage::Request(req) => self.validate_request(req),
            JsonRpcMessage::Response(resp) => self.validate_response(resp),
            JsonRpcMessage::ErrorResponse(err_resp) => self.validate_error_response(err_resp),
            JsonRpcMessage::Notification(notif) => self.validate_notification(notif),
        }
    }

    /// Validate a JSON-RPC request
    #[inline]
    pub fn validate_request(&self, request: &JsonRpcRequest) -> Result<()> {
        let request_value = serde_json::to_value(request)?;
        self.validate_with_schema("jsonrpc_request", &request_value)?;

        // Validate method-specific parameters
        if let Some(params) = &request.params {
            self.validate_method_params(&request.method, params)?;
        }

        Ok(())
    }

    /// Validate a JSON-RPC response
    #[inline]
    pub fn validate_response(&self, response: &JsonRpcResponse) -> Result<()> {
        let response_value = serde_json::to_value(response)?;
        self.validate_with_schema("jsonrpc_response", &response_value)
    }

    /// Validate a JSON-RPC error response
    #[inline]
    pub fn validate_error_response(&self, error_response: &JsonRpcErrorResponse) -> Result<()> {
        let error_value = serde_json::to_value(error_response)?;
        self.validate_with_schema("jsonrpc_error_response", &error_value)
    }

    /// Validate a JSON-RPC notification
    #[inline]
    pub fn validate_notification(&self, notification: &JsonRpcNotification) -> Result<()> {
        let notification_value = serde_json::to_value(notification)?;
        self.validate_with_schema("jsonrpc_notification", &notification_value)
    }

    /// Validate method-specific parameters
    fn validate_method_params(&self, method: &str, params: &Value) -> Result<()> {
        let schema_name = match method {
            "initialize" => "initialize_params",
            "tools/call" => "call_tool_params",
            _ => {
                // For unknown methods, we skip parameter validation
                debug!("No parameter validation schema for method: {}", method);
                return Ok(());
            }
        };

        self.validate_with_schema(schema_name, params)
    }

    /// Validate a value against a named schema
    #[inline]
    pub fn validate_with_schema(&self, schema_name: &str, value: &Value) -> Result<()> {
        let schema = self
            .schemas
            .get(schema_name)
            .ok_or_else(|| anyhow!("Schema '{}' not found", schema_name))?;

        let validation_result = schema.validate(value);
        if let Err(errors) = validation_result {
            let error_messages: Vec<String> = errors
                .into_iter()
                .map(|e| format!("{}:{}", e.instance_path, e))
                .collect();

            return Err(anyhow!(
                "Schema validation failed for '{}': {}",
                schema_name,
                error_messages.join(", ")
            ));
        }

        Ok(())
    }

    /// Validate a raw JSON value as a JSON-RPC message
    #[inline]
    pub fn validate_raw_message(&self, value: &Value) -> Result<JsonRpcMessage> {
        // Try to parse as different message types
        if let Ok(request) = serde_json::from_value::<JsonRpcRequest>(value.clone()) {
            self.validate_request(&request)?;
            return Ok(JsonRpcMessage::Request(request));
        }

        if let Ok(response) = serde_json::from_value::<JsonRpcResponse>(value.clone()) {
            self.validate_response(&response)?;
            return Ok(JsonRpcMessage::Response(response));
        }

        if let Ok(error_response) = serde_json::from_value::<JsonRpcErrorResponse>(value.clone()) {
            self.validate_error_response(&error_response)?;
            return Ok(JsonRpcMessage::ErrorResponse(error_response));
        }

        if let Ok(notification) = serde_json::from_value::<JsonRpcNotification>(value.clone()) {
            self.validate_notification(&notification)?;
            return Ok(JsonRpcMessage::Notification(notification));
        }

        Err(anyhow!(
            "Value does not match any known JSON-RPC message type"
        ))
    }

    /// Check if a protocol version is supported
    #[inline]
    pub fn is_protocol_version_supported(&self, version: &str) -> bool {
        version == MCP_VERSION
    }

    /// Get supported protocol versions
    #[inline]
    pub fn supported_protocol_versions(&self) -> Vec<&'static str> {
        vec![MCP_VERSION]
    }
}

impl Default for McpValidator {
    #[inline]
    fn default() -> Self {
        Self::new().expect("Failed to create default MCP validator")
    }
}

/// Validation error helper functions
#[inline]
pub fn validation_error(message: &str) -> JsonRpcError {
    JsonRpcError::invalid_params(Some(message.to_string()))
}

/// Protocol version error helper
#[inline]
pub fn protocol_version_error(requested: &str, supported: &[&str]) -> JsonRpcError {
    JsonRpcError::new(
        mcp_error_codes::INVALID_PROTOCOL_VERSION,
        format!(
            "Unsupported protocol version: {}. Supported versions: {}",
            requested,
            supported.join(", ")
        ),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_creation() {
        let validator = McpValidator::new();
        let validator = validator.expect("validator is ok");
        assert!(!validator.schemas.is_empty());
    }

    #[test]
    fn protocol_version_validation() {
        let validator = McpValidator::new().expect("validator is ok");

        assert!(validator.is_protocol_version_supported(MCP_VERSION));
        assert!(!validator.is_protocol_version_supported("invalid-version"));
    }

    #[test]
    fn request_validation() {
        let validator = McpValidator::new().expect("validator is ok");

        let valid_request = JsonRpcRequest::new(
            "test_method".to_string(),
            Some(json!({"key": "value"})),
            RequestId::String("test-id".to_string()),
        );

        assert!(validator.validate_request(&valid_request).is_ok());
    }

    #[test]
    fn initialize_params_validation() {
        let validator = McpValidator::new().expect("validator is ok");

        let params = json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "experimental": {}
            },
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        });

        assert!(
            validator
                .validate_with_schema("initialize_params", &params)
                .is_ok()
        );
    }

    #[test]
    fn invalid_params_validation() {
        let validator = McpValidator::new().expect("validator is ok");

        let invalid_params = json!({
            "protocolVersion": "2025-06-18"
            // Missing required fields
        });

        assert!(
            validator
                .validate_with_schema("initialize_params", &invalid_params)
                .is_err()
        );
    }
}
