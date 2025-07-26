//! MCP Error Handling
//!
//! This module provides comprehensive error handling for the MCP server,
//! including error classification, formatting, and response generation.

use crate::mcp::protocol::*;
use thiserror::Error;
use tracing::error;

/// MCP-specific errors that can occur during server operation
#[derive(Error, Debug)]
pub enum McpError {
    #[error("Protocol version not supported: {version}. Supported versions: {supported:?}")]
    UnsupportedProtocolVersion {
        version: String,
        supported: Vec<String>,
    },

    #[error("Tool not found: {name}")]
    ToolNotFound { name: String },

    #[error("Resource not found: {uri}")]
    ResourceNotFound { uri: String },

    #[error("Prompt not found: {name}")]
    PromptNotFound { name: String },

    #[error("Invalid tool parameters for {tool}: {message}")]
    InvalidToolParameters { tool: String, message: String },

    #[error("Tool execution failed for {tool}: {message}")]
    ToolExecutionFailed { tool: String, message: String },

    #[error("Resource access failed for {uri}: {message}")]
    ResourceAccessFailed { uri: String, message: String },

    #[error("Server not initialized")]
    ServerNotInitialized,

    #[error("Server already initialized")]
    ServerAlreadyInitialized,

    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("Internal server error: {message}")]
    InternalError { message: String },

    #[error("JSON-RPC parse error: {message}")]
    ParseError { message: String },

    #[error("Method not found: {method}")]
    MethodNotFound { method: String },

    #[error("Invalid parameters: {message}")]
    InvalidParameters { message: String },

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout occurred: {operation}")]
    Timeout { operation: String },

    #[error("Validation error: {message}")]
    ValidationError { message: String },
}

impl McpError {
    /// Convert MCP error to JSON-RPC error
    #[inline]
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        match self {
            Self::UnsupportedProtocolVersion { version, supported } => JsonRpcError::new(
                mcp_error_codes::INVALID_PROTOCOL_VERSION,
                format!(
                    "Unsupported protocol version: {}. Supported: {}",
                    version,
                    supported.join(", ")
                ),
                None,
            ),
            Self::ToolNotFound { name } => JsonRpcError::new(
                mcp_error_codes::TOOL_NOT_FOUND,
                format!("Tool not found: {}", name),
                None,
            ),
            Self::ResourceNotFound { uri } => JsonRpcError::new(
                mcp_error_codes::RESOURCE_NOT_FOUND,
                format!("Resource not found: {}", uri),
                None,
            ),
            Self::PromptNotFound { name } => JsonRpcError::new(
                mcp_error_codes::PROMPT_NOT_FOUND,
                format!("Prompt not found: {}", name),
                None,
            ),
            Self::InvalidToolParameters { tool, message } => JsonRpcError::new(
                error_codes::INVALID_PARAMS,
                format!("Invalid parameters for tool '{}': {}", tool, message),
                None,
            ),
            Self::ToolExecutionFailed { tool, message } => JsonRpcError::new(
                error_codes::INTERNAL_ERROR,
                format!("Tool '{}' execution failed: {}", tool, message),
                None,
            ),
            Self::ResourceAccessFailed { uri, message } => JsonRpcError::new(
                error_codes::INTERNAL_ERROR,
                format!("Resource '{}' access failed: {}", uri, message),
                None,
            ),
            Self::ServerNotInitialized => JsonRpcError::new(
                error_codes::INVALID_REQUEST,
                "Server not initialized. Send initialize request first.".to_string(),
                None,
            ),
            Self::ServerAlreadyInitialized => JsonRpcError::new(
                error_codes::INVALID_REQUEST,
                "Server already initialized.".to_string(),
                None,
            ),
            Self::InvalidRequest { message } => {
                JsonRpcError::new(error_codes::INVALID_REQUEST, message.clone(), None)
            }
            Self::InternalError { message } => {
                JsonRpcError::new(error_codes::INTERNAL_ERROR, message.clone(), None)
            }
            Self::ParseError { message } => {
                JsonRpcError::new(error_codes::PARSE_ERROR, message.clone(), None)
            }
            Self::MethodNotFound { method } => JsonRpcError::new(
                error_codes::METHOD_NOT_FOUND,
                format!("Method not found: {}", method),
                None,
            ),
            Self::InvalidParameters { message } => {
                JsonRpcError::new(error_codes::INVALID_PARAMS, message.clone(), None)
            }
            Self::ConnectionClosed => JsonRpcError::new(
                error_codes::INTERNAL_ERROR,
                "Connection closed".to_string(),
                None,
            ),
            Self::Timeout { operation } => JsonRpcError::new(
                error_codes::INTERNAL_ERROR,
                format!("Operation timed out: {}", operation),
                None,
            ),
            Self::ValidationError { message } => JsonRpcError::new(
                error_codes::INVALID_PARAMS,
                format!("Validation error: {}", message),
                None,
            ),
        }
    }

    /// Create error response message
    #[inline]
    pub fn to_error_response(&self, id: Option<RequestId>) -> JsonRpcMessage {
        let error = self.to_jsonrpc_error();
        let error_response = JsonRpcErrorResponse::new(error, id);
        JsonRpcMessage::ErrorResponse(error_response)
    }

    /// Log the error with appropriate level
    #[inline]
    pub fn log(&self) {
        match self {
            Self::ParseError { .. }
            | Self::InvalidRequest { .. }
            | Self::InvalidParameters { .. } => {
                error!("Client error: {}", self);
            }
            Self::ToolNotFound { .. }
            | Self::ResourceNotFound { .. }
            | Self::PromptNotFound { .. } => {
                error!("Not found error: {}", self);
            }
            Self::ToolExecutionFailed { .. }
            | Self::ResourceAccessFailed { .. }
            | Self::InternalError { .. } => {
                error!("Server error: {}", self);
            }
            _ => {
                error!("MCP error: {}", self);
            }
        }
    }
}

/// Error handler utility for consistent error processing
pub struct ErrorHandler;

impl ErrorHandler {
    /// Handle any error and convert to appropriate JSON-RPC response
    #[inline]
    pub fn handle_error(error: &anyhow::Error, id: Option<RequestId>) -> JsonRpcMessage {
        // Try to downcast to MCP error first
        if let Some(mcp_error) = error.downcast_ref::<McpError>() {
            mcp_error.log();
            return mcp_error.to_error_response(id);
        }

        // Handle other error types
        error!("Unexpected error: {}", error);
        let internal_error = McpError::InternalError {
            message: error.to_string(),
        };
        internal_error.to_error_response(id)
    }

    /// Handle validation errors
    #[inline]
    pub fn handle_validation_error(message: &str) -> JsonRpcError {
        JsonRpcError::new(
            error_codes::INVALID_PARAMS,
            format!("Validation failed: {}", message),
            None,
        )
    }

    /// Handle protocol version errors
    #[inline]
    pub fn handle_protocol_version_error(requested: &str, supported: &[String]) -> JsonRpcError {
        JsonRpcError::new(
            mcp_error_codes::INVALID_PROTOCOL_VERSION,
            format!(
                "Unsupported protocol version: {}. Supported: {}",
                requested,
                supported.join(", ")
            ),
            None,
        )
    }

    /// Create a generic internal error
    #[inline]
    pub fn internal_error(message: &str) -> JsonRpcError {
        JsonRpcError::new(error_codes::INTERNAL_ERROR, message.to_string(), None)
    }

    /// Create a parse error
    #[inline]
    pub fn parse_error(message: Option<&str>) -> JsonRpcError {
        let msg = message.unwrap_or("Parse error");
        JsonRpcError::new(error_codes::PARSE_ERROR, msg.to_string(), None)
    }

    /// Create an invalid request error
    #[inline]
    pub fn invalid_request(message: Option<&str>) -> JsonRpcError {
        let msg = message.unwrap_or("Invalid request");
        JsonRpcError::new(error_codes::INVALID_REQUEST, msg.to_string(), None)
    }

    /// Create a method not found error
    #[inline]
    pub fn method_not_found(method: &str) -> JsonRpcError {
        JsonRpcError::new(
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {}", method),
            None,
        )
    }
}

/// Result type for MCP operations
pub type McpResult<T> = Result<T, McpError>;

/// Convert from anyhow::Error to McpError
impl From<anyhow::Error> for McpError {
    #[inline]
    fn from(error: anyhow::Error) -> Self {
        Self::InternalError {
            message: error.to_string(),
        }
    }
}

/// Convert from serde_json::Error to McpError
impl From<serde_json::Error> for McpError {
    #[inline]
    fn from(error: serde_json::Error) -> Self {
        Self::ParseError {
            message: error.to_string(),
        }
    }
}

/// Convert from validation errors to McpError
impl From<jsonschema::ValidationError<'_>> for McpError {
    #[inline]
    fn from(error: jsonschema::ValidationError) -> Self {
        Self::ValidationError {
            message: format!("{}:{}", error.instance_path, error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_not_found_error() {
        let error = McpError::ToolNotFound {
            name: "test_tool".to_string(),
        };

        let jsonrpc_error = error.to_jsonrpc_error();
        assert_eq!(jsonrpc_error.code, mcp_error_codes::TOOL_NOT_FOUND);
        assert!(jsonrpc_error.message.contains("test_tool"));
    }

    #[test]
    fn invalid_protocol_version_error() {
        let error = McpError::UnsupportedProtocolVersion {
            version: "invalid".to_string(),
            supported: vec!["2025-06-18".to_string()],
        };

        let jsonrpc_error = error.to_jsonrpc_error();
        assert_eq!(
            jsonrpc_error.code,
            mcp_error_codes::INVALID_PROTOCOL_VERSION
        );
        assert!(jsonrpc_error.message.contains("invalid"));
        assert!(jsonrpc_error.message.contains("2025-06-18"));
    }

    #[test]
    fn error_response_creation() {
        let error = McpError::InternalError {
            message: "test error".to_string(),
        };

        let response = error.to_error_response(Some(RequestId::String("test".to_string())));

        if let JsonRpcMessage::ErrorResponse(err_resp) = response {
            assert_eq!(err_resp.error.code, error_codes::INTERNAL_ERROR);
            assert!(err_resp.error.message.contains("test error"));
        } else {
            panic!("Expected error response");
        }
    }

    #[test]
    fn error_handler_methods() {
        let error = ErrorHandler::method_not_found("test_method");
        assert_eq!(error.code, error_codes::METHOD_NOT_FOUND);
        assert!(error.message.contains("test_method"));

        let error = ErrorHandler::internal_error("test message");
        assert_eq!(error.code, error_codes::INTERNAL_ERROR);
        assert!(error.message.contains("test message"));
    }
}
