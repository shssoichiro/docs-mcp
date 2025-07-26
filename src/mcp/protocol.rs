//! MCP Protocol Types and Messages
//!
//! This module defines the core Model Context Protocol message types,
//! following the JSON-RPC 2.0 specification and MCP protocol version 2025-06-18.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MCP Protocol Version
pub const MCP_VERSION: &str = "2025-06-18";

/// JSON-RPC 2.0 version identifier
pub const JSONRPC_VERSION: &str = "2.0";

/// Unique identifier for JSON-RPC messages
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
}

/// JSON-RPC 2.0 Request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: RequestId,
}

/// JSON-RPC 2.0 Response message (success)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: serde_json::Value,
    pub id: RequestId,
}

/// JSON-RPC 2.0 Error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub error: JsonRpcError,
    pub id: Option<RequestId>,
}

/// JSON-RPC 2.0 Notification message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// Any JSON-RPC message type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    ErrorResponse(JsonRpcErrorResponse),
    Notification(JsonRpcNotification),
}

/// MCP Initialize Request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: Implementation,
}

/// MCP Initialize Response result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: Implementation,
    pub instructions: Option<String>,
}

/// Client capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub experimental: Option<HashMap<String, serde_json::Value>>,
    pub sampling: Option<SamplingCapability>,
}

/// Server capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub experimental: Option<HashMap<String, serde_json::Value>>,
    pub logging: Option<LoggingCapability>,
    pub prompts: Option<PromptsCapability>,
    pub resources: Option<ResourcesCapability>,
    pub tools: Option<ToolsCapability>,
}

/// Implementation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

/// Server health status for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHealthStatus {
    pub connection_state: crate::mcp::server::ConnectionState,
    pub tools_registered: usize,
    pub resources_registered: usize,
    pub uptime: std::time::Duration,
}

/// Detailed server statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatistics {
    pub server_info: Implementation,
    pub capabilities: ServerCapabilities,
    pub connection_state: crate::mcp::server::ConnectionState,
    pub registered_tools: Vec<String>,
    pub registered_resources: Vec<String>,
}

/// Sampling capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingCapability {}

/// Logging capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingCapability {}

/// Prompts capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Resources capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Tools capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Tool call request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    pub arguments: Option<HashMap<String, serde_json::Value>>,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

/// Tool content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource { resource: ResourceReference },
}

/// Resource reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReference {
    pub uri: String,
    pub text: Option<String>,
}

/// Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// List tools response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<Tool>,
}

/// List resources response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesResult {
    pub resources: Vec<Resource>,
}

/// Progress notification parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressParams {
    #[serde(rename = "progressToken")]
    pub progress_token: serde_json::Value,
    pub progress: f64,
    pub total: Option<f64>,
}

/// Log level enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

/// Logging message parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingMessageParams {
    pub level: LogLevel,
    pub data: serde_json::Value,
    pub logger: Option<String>,
}

/// Standard JSON-RPC error codes
#[allow(dead_code)]
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}

/// MCP-specific error codes
#[allow(dead_code)]
pub mod mcp_error_codes {
    pub const INVALID_PROTOCOL_VERSION: i32 = -32000;
    pub const TOOL_NOT_FOUND: i32 = -32001;
    pub const RESOURCE_NOT_FOUND: i32 = -32002;
    pub const PROMPT_NOT_FOUND: i32 = -32003;
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request
    #[inline]
    pub fn new(method: String, params: Option<serde_json::Value>, id: RequestId) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method,
            params,
            id,
        }
    }
}

impl JsonRpcResponse {
    /// Create a new JSON-RPC response
    #[inline]
    pub fn new(result: serde_json::Value, id: RequestId) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            result,
            id,
        }
    }
}

impl JsonRpcErrorResponse {
    /// Create a new JSON-RPC error response
    #[inline]
    pub fn new(error: JsonRpcError, id: Option<RequestId>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            error,
            id,
        }
    }
}

impl JsonRpcNotification {
    /// Create a new JSON-RPC notification
    #[inline]
    pub fn new(method: String, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method,
            params,
        }
    }
}

impl JsonRpcError {
    /// Create a new JSON-RPC error
    #[inline]
    pub fn new(code: i32, message: String, data: Option<serde_json::Value>) -> Self {
        Self {
            code,
            message,
            data,
        }
    }

    /// Create a parse error
    #[inline]
    pub fn parse_error() -> Self {
        Self::new(error_codes::PARSE_ERROR, "Parse error".to_string(), None)
    }

    /// Create an invalid request error
    #[inline]
    pub fn invalid_request() -> Self {
        Self::new(
            error_codes::INVALID_REQUEST,
            "Invalid Request".to_string(),
            None,
        )
    }

    /// Create a method not found error
    #[inline]
    pub fn method_not_found() -> Self {
        Self::new(
            error_codes::METHOD_NOT_FOUND,
            "Method not found".to_string(),
            None,
        )
    }

    /// Create an invalid params error
    #[inline]
    pub fn invalid_params(message: Option<String>) -> Self {
        let msg = message.unwrap_or_else(|| "Invalid params".to_string());
        Self::new(error_codes::INVALID_PARAMS, msg, None)
    }

    /// Create an internal error
    #[inline]
    pub fn internal_error(message: Option<String>) -> Self {
        let msg = message.unwrap_or_else(|| "Internal error".to_string());
        Self::new(error_codes::INTERNAL_ERROR, msg, None)
    }
}
