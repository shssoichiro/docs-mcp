//! MCP Protocol Implementation Tests
//!
//! Comprehensive unit tests for the MCP server implementation,
//! including protocol compliance, message handling, and error cases.

use super::*;
use crate::mcp::server::MessageHandler;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

/// Mock tool handler for testing
struct MockToolHandler {
    response: CallToolResult,
}

impl MockToolHandler {
    fn new(response: CallToolResult) -> Self {
        Self { response }
    }
}

#[async_trait]
impl ToolHandler for MockToolHandler {
    async fn handle(&self, _params: CallToolParams) -> anyhow::Result<CallToolResult> {
        Ok(self.response.clone())
    }
}

/// Mock resource handler for testing
struct MockResourceHandler {
    response: serde_json::Value,
}

impl MockResourceHandler {
    fn new(response: serde_json::Value) -> Self {
        Self { response }
    }
}

#[async_trait]
impl ResourceHandler for MockResourceHandler {
    async fn handle(&self, _uri: &str) -> anyhow::Result<serde_json::Value> {
        Ok(self.response.clone())
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn request_id_serialization() {
        let string_id = RequestId::String("test-123".to_string());
        let number_id = RequestId::Number(42);

        let string_json = serde_json::to_string(&string_id).expect("can serialize");
        let number_json = serde_json::to_string(&number_id).expect("can serialize");

        assert_eq!(string_json, "\"test-123\"");
        assert_eq!(number_json, "42");
    }

    #[test]
    fn jsonrpc_request_creation() {
        let request = JsonRpcRequest::new(
            "test_method".to_string(),
            Some(json!({"param": "value"})),
            RequestId::String("test-id".to_string()),
        );

        assert_eq!(request.jsonrpc, JSONRPC_VERSION);
        assert_eq!(request.method, "test_method");
        assert!(request.params.is_some());
        assert_eq!(request.id, RequestId::String("test-id".to_string()));
    }

    #[test]
    fn jsonrpc_response_creation() {
        let response = JsonRpcResponse::new(json!({"result": "success"}), RequestId::Number(1));

        assert_eq!(response.jsonrpc, JSONRPC_VERSION);
        assert_eq!(response.result, json!({"result": "success"}));
        assert_eq!(response.id, RequestId::Number(1));
    }

    #[test]
    fn jsonrpc_error_creation() {
        let error = JsonRpcError::method_not_found();

        assert_eq!(error.code, error_codes::METHOD_NOT_FOUND);
        assert_eq!(error.message, "Method not found");
        assert!(error.data.is_none());
    }

    #[test]
    fn mcp_error_codes() {
        assert_eq!(error_codes::PARSE_ERROR, -32700);
        assert_eq!(error_codes::INVALID_REQUEST, -32600);
        assert_eq!(error_codes::METHOD_NOT_FOUND, -32601);
        assert_eq!(error_codes::INVALID_PARAMS, -32602);
        assert_eq!(error_codes::INTERNAL_ERROR, -32603);
    }

    #[test]
    fn tool_definition_serialization() {
        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "param": {"type": "string"}
                }
            }),
        };

        let serialized = serde_json::to_string(&tool).expect("can serialize");
        let deserialized: Tool = serde_json::from_str(&serialized).expect("can deserialize");

        assert_eq!(deserialized.name, "test_tool");
        assert_eq!(deserialized.description, Some("A test tool".to_string()));
    }

    #[test]
    fn initialize_params_serialization() {
        let params = InitializeParams {
            protocol_version: MCP_VERSION.to_string(),
            capabilities: ClientCapabilities {
                experimental: None,
                sampling: Some(SamplingCapability {}),
            },
            client_info: Implementation {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let serialized = serde_json::to_string(&params).expect("can serialize");
        let deserialized: InitializeParams =
            serde_json::from_str(&serialized).expect("can deserialize");

        assert_eq!(deserialized.protocol_version, MCP_VERSION);
        assert_eq!(deserialized.client_info.name, "test-client");
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[tokio::test]
    async fn validator_creation() {
        let validator = McpValidator::new();
        assert!(validator.is_ok());
    }

    #[tokio::test]
    async fn protocol_version_validation() {
        let validator = McpValidator::new().expect("validator created");

        assert!(validator.is_protocol_version_supported(MCP_VERSION));
        assert!(!validator.is_protocol_version_supported("invalid-version"));
    }

    #[tokio::test]
    async fn request_validation() {
        let validator = McpValidator::new().expect("validator created");

        let valid_request = JsonRpcRequest::new(
            "ping".to_string(),
            None,
            RequestId::String("test-1".to_string()),
        );

        assert!(validator.validate_request(&valid_request).is_ok());
    }

    #[tokio::test]
    async fn initialize_params_validation() {
        let validator = McpValidator::new().expect("validator created");

        let valid_params = json!({
            "protocolVersion": MCP_VERSION,
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
                .validate_with_schema("initialize_params", &valid_params)
                .is_ok()
        );

        let invalid_params = json!({
            "protocolVersion": MCP_VERSION
            // Missing required fields
        });

        assert!(
            validator
                .validate_with_schema("initialize_params", &invalid_params)
                .is_err()
        );
    }

    #[tokio::test]
    async fn raw_message_validation() {
        let validator = McpValidator::new().expect("validator created");

        let valid_request_json = json!({
            "jsonrpc": "2.0",
            "method": "ping",
            "id": "test-1"
        });

        let result = validator.validate_raw_message(&valid_request_json);
        assert!(result.is_ok());

        if let Ok(JsonRpcMessage::Request(req)) = result {
            assert_eq!(req.method, "ping");
            assert_eq!(req.id, RequestId::String("test-1".to_string()));
        } else {
            panic!("Expected request message");
        }
    }
}

#[cfg(test)]
mod server_tests {
    use super::*;

    #[tokio::test]
    async fn server_creation() {
        let server = McpServer::new("test-server".to_string(), "1.0.0".to_string());
        assert!(server.is_ok());

        let server = server.expect("server created");
        assert_eq!(server.server_info.name, "test-server");
        assert_eq!(server.server_info.version, "1.0.0");
        assert_eq!(
            server.connection_state().await,
            ConnectionState::Uninitialized
        );
    }

    #[tokio::test]
    async fn tool_registration() {
        let server =
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created");

        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({"type": "object"}),
        };

        let mock_response = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Test response".to_string(),
            }],
            is_error: Some(false),
        };
        let handler = MockToolHandler::new(mock_response);

        let result = server.register_tool(tool.clone(), handler).await;
        assert!(result.is_ok());

        // Verify tool was registered
        let tools = server.tools.read().await;
        assert_eq!(
            tools.get("test_tool").expect("has key test_tool").name,
            "test_tool"
        );
    }

    #[tokio::test]
    async fn resource_registration() {
        let server =
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created");

        let resource = Resource {
            uri: "test://resource".to_string(),
            name: "Test Resource".to_string(),
            description: Some("A test resource".to_string()),
            mime_type: Some("text/plain".to_string()),
        };

        let handler = MockResourceHandler::new(json!({"data": "test"}));

        let result = server.register_resource(resource.clone(), handler).await;
        assert!(result.is_ok());

        // Verify resource was registered
        let resources = server.resources.read().await;
        assert_eq!(
            resources
                .get("test://resource")
                .expect("has key test://resource")
                .name,
            "Test Resource"
        );
    }

    #[tokio::test]
    async fn connection_state_transitions() {
        let server =
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created");

        // Initial state
        assert_eq!(
            server.connection_state().await,
            ConnectionState::Uninitialized
        );

        // Simulate state changes
        {
            let mut state = server.connection_state.write().await;
            *state = ConnectionState::Initializing;
        }
        assert_eq!(
            server.connection_state().await,
            ConnectionState::Initializing
        );

        {
            let mut state = server.connection_state.write().await;
            *state = ConnectionState::Ready;
        }
        assert_eq!(server.connection_state().await, ConnectionState::Ready);
    }
}

#[cfg(test)]
mod message_handler_tests {
    use super::*;

    #[tokio::test]
    async fn initialize_request_handling() {
        let server = Arc::new(
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created"),
        );
        let handler = MessageHandler::new(Arc::clone(&server));

        let init_params = json!({
            "protocolVersion": MCP_VERSION,
            "capabilities": {
                "experimental": {}
            },
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        });

        let result = handler.handle_initialize(Some(init_params)).await;
        let response_value = result.expect("result is ok");
        let response: InitializeResult =
            serde_json::from_value(response_value).expect("can convert struct");

        assert_eq!(response.protocol_version, MCP_VERSION);
        assert_eq!(response.server_info.name, "test-server");
    }

    #[tokio::test]
    async fn initialize_with_invalid_version() {
        let server = Arc::new(
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created"),
        );
        let handler = MessageHandler::new(Arc::clone(&server));

        let init_params = json!({
            "protocolVersion": "invalid-version",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        });

        let result = handler.handle_initialize(Some(init_params)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ping_request_handling() {
        let server = Arc::new(
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created"),
        );
        let handler = MessageHandler::new(Arc::clone(&server));

        let result = handler.handle_ping().await;
        let response = result.expect("result is ok");
        assert_eq!(response, serde_json::json!({}));
    }

    #[tokio::test]
    async fn list_tools_request_handling() {
        let server = Arc::new(
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created"),
        );

        // Register a test tool
        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({"type": "object"}),
        };

        let mock_response = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Test response".to_string(),
            }],
            is_error: Some(false),
        };
        let handler_tool = MockToolHandler::new(mock_response);
        server
            .register_tool(tool.clone(), handler_tool)
            .await
            .expect("register tool succeeded");

        let handler = MessageHandler::new(Arc::clone(&server));
        let result = handler.handle_list_tools().await;
        let response_value = result.expect("result is ok");
        let response: ListToolsResult =
            serde_json::from_value(response_value).expect("can convert struct");

        assert_eq!(response.tools.len(), 1);
        assert_eq!(response.tools[0].name, "test_tool");
    }

    #[tokio::test]
    async fn call_tool_request_handling() {
        let server = Arc::new(
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created"),
        );

        // Register a test tool
        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({"type": "object"}),
        };

        let expected_response = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Test response".to_string(),
            }],
            is_error: Some(false),
        };
        let handler_tool = MockToolHandler::new(expected_response.clone());
        server
            .register_tool(tool.clone(), handler_tool)
            .await
            .expect("register tool succeeded");

        let handler = MessageHandler::new(Arc::clone(&server));

        let call_params = json!({
            "name": "test_tool",
            "arguments": {}
        });

        let result = handler.handle_call_tool(Some(call_params)).await;
        let response_value = result.expect("result is ok");
        let response: CallToolResult =
            serde_json::from_value(response_value).expect("can convert struct");

        assert_eq!(response.content.len(), 1);
        if let ToolContent::Text { text } = &response.content[0] {
            assert_eq!(text, "Test response");
        } else {
            panic!("Expected text content");
        }
    }

    #[tokio::test]
    async fn call_nonexistent_tool() {
        let server = Arc::new(
            McpServer::new("test-server".to_string(), "1.0.0".to_string()).expect("server created"),
        );
        let handler = MessageHandler::new(Arc::clone(&server));

        let call_params = json!({
            "name": "nonexistent_tool",
            "arguments": {}
        });

        let result = handler.handle_call_tool(Some(call_params)).await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn mcp_error_to_jsonrpc_conversion() {
        let error = McpError::ToolNotFound {
            name: "test_tool".to_string(),
        };

        let jsonrpc_error = error.to_jsonrpc_error();
        assert_eq!(jsonrpc_error.code, mcp_error_codes::TOOL_NOT_FOUND);
        assert!(jsonrpc_error.message.contains("test_tool"));
    }

    #[test]
    fn error_response_creation() {
        let error = McpError::InvalidParameters {
            message: "Invalid input".to_string(),
        };

        let response = error.to_error_response(Some(RequestId::String("test".to_string())));

        if let JsonRpcMessage::ErrorResponse(err_resp) = response {
            assert_eq!(err_resp.error.code, error_codes::INVALID_PARAMS);
            assert!(err_resp.error.message.contains("Invalid input"));
            assert_eq!(err_resp.id, Some(RequestId::String("test".to_string())));
        } else {
            panic!("Expected error response");
        }
    }

    #[test]
    fn error_handler_utilities() {
        let error = ErrorHandler::method_not_found("unknown_method");
        assert_eq!(error.code, error_codes::METHOD_NOT_FOUND);
        assert!(error.message.contains("unknown_method"));

        let error = ErrorHandler::internal_error("Something went wrong");
        assert_eq!(error.code, error_codes::INTERNAL_ERROR);
        assert!(error.message.contains("Something went wrong"));
    }
}

#[cfg(test)]
mod tool_registry_tests {
    use super::*;

    #[test]
    fn tool_registry_creation() {
        let registry = ToolRegistry::new();
        assert!(registry.list_tools().is_empty());
    }

    #[test]
    fn tool_registration_in_registry() {
        let mut registry = ToolRegistry::new();

        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: json!({"type": "object"}),
        };

        registry.register(tool);

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test_tool");

        let retrieved_tool = registry.get_tool("test_tool");
        assert_eq!(retrieved_tool.expect("tool is some").name, "test_tool");
    }

    #[test]
    fn default_tool_registry() {
        let registry = ToolRegistry::create_default();
        let tools = registry.list_tools();

        // Should have search_docs and list_sites tools
        assert_eq!(tools.len(), 2);

        let tool_names: Vec<&String> = tools.iter().map(|t| &t.name).collect();
        assert!(tool_names.contains(&&"search_docs".to_string()));
        assert!(tool_names.contains(&&"list_sites".to_string()));
    }
}
