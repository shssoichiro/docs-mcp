#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

//! MCP Server Integration Tests
//!
//! Comprehensive integration tests for the complete MCP server functionality,
//! including server startup, tool registration, message handling, and cleanup.

use docs_mcp::config::Config;
use docs_mcp::database::sqlite::Database;
use docs_mcp::mcp::tools::ListSitesHandler;
use docs_mcp::mcp::{CallToolParams, McpServer, ToolContent, ToolHandler};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// Test helper to create a temporary config and database setup
async fn setup_test_environment() -> (TempDir, Config, Arc<Database>) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a test config (using default values for this test)
    let config = Config::default();

    // Initialize test database
    let db_path = temp_dir.path().join("test_metadata.db");
    let database = Arc::new(
        Database::new(db_path.to_string_lossy().as_ref())
            .await
            .expect("Failed to create test database"),
    );

    (temp_dir, config, database)
}

/// Test MCP server creation and basic initialization
#[tokio::test]
async fn mcp_server_initialization() {
    let server = McpServer::new("test-server".to_string(), "1.0.0".to_string())
        .expect("Failed to create MCP server");

    assert_eq!(server.server_info.name, "test-server");
    assert_eq!(server.server_info.version, "1.0.0");

    let connection_state = server.connection_state().await;
    assert_eq!(
        connection_state,
        docs_mcp::mcp::server::ConnectionState::Uninitialized
    );

    let health_status = server.health_status().await;
    assert_eq!(health_status.tools_registered, 0);
    assert_eq!(health_status.resources_registered, 0);
}

/// Test tool registration and functionality
#[tokio::test]
async fn tool_registration() {
    let (_temp_dir, _config, database) = setup_test_environment().await;

    let server = Arc::new(
        McpServer::new("test-server".to_string(), "1.0.0".to_string())
            .expect("Failed to create MCP server"),
    );

    // Mock vector store and ollama client for testing
    // Note: These would need to be mocked or use test implementations
    // For now, we'll test the registration process

    // Test list_sites tool registration
    let list_handler = ListSitesHandler::new(Arc::clone(&database));
    let list_tool = ListSitesHandler::tool_definition();

    server
        .register_tool(list_tool.clone(), list_handler)
        .await
        .expect("Failed to register list_sites tool");

    let health_status = server.health_status().await;
    assert_eq!(health_status.tools_registered, 1);

    let statistics = server.server_statistics().await;
    assert_eq!(statistics.registered_tools.len(), 1);
    assert!(
        statistics
            .registered_tools
            .contains(&"list_sites".to_string())
    );
}

/// Test list_sites tool execution with empty database
#[tokio::test]
async fn list_sites_tool_empty() {
    let (_temp_dir, _config, database) = setup_test_environment().await;

    let handler = ListSitesHandler::new(Arc::clone(&database));
    let params = CallToolParams {
        name: "list_sites".to_string(),
        arguments: Some(HashMap::new()),
    };

    let result = handler.handle(params).await.expect("Tool execution failed");

    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.content.len(), 1);

    if let ToolContent::Text { text } = &result.content[0] {
        let response: serde_json::Value =
            serde_json::from_str(text).expect("Failed to parse JSON response");

        assert!(response["sites"].is_array());
        assert_eq!(response["sites"].as_array().expect("is array").len(), 0);
    } else {
        panic!("Expected text content");
    }
}

/// Test MCP server message handling for list_tools request
#[tokio::test]
async fn message_handler_list_tools() {
    use docs_mcp::mcp::server::MessageHandler;

    let (_temp_dir, _config, database) = setup_test_environment().await;

    let server = Arc::new(
        McpServer::new("test-server".to_string(), "1.0.0".to_string())
            .expect("Failed to create MCP server"),
    );

    // Register list_sites tool
    let list_handler = ListSitesHandler::new(Arc::clone(&database));
    let list_tool = ListSitesHandler::tool_definition();

    server
        .register_tool(list_tool, list_handler)
        .await
        .expect("Failed to register tool");

    let handler = MessageHandler::new(Arc::clone(&server));
    let result = handler
        .handle_list_tools()
        .await
        .expect("Failed to list tools");

    let tools_result: docs_mcp::mcp::ListToolsResult =
        serde_json::from_value(result).expect("Failed to deserialize tools result");

    assert_eq!(tools_result.tools.len(), 1);
    assert_eq!(tools_result.tools[0].name, "list_sites");
}

/// Test server health monitoring and statistics
#[tokio::test]
async fn server_health_monitoring() {
    let server = McpServer::new("test-server".to_string(), "1.0.0".to_string())
        .expect("Failed to create MCP server");

    let health_status = server.health_status().await;
    assert_eq!(
        health_status.connection_state,
        docs_mcp::mcp::server::ConnectionState::Uninitialized
    );
    assert_eq!(health_status.tools_registered, 0);
    assert_eq!(health_status.resources_registered, 0);
    assert!(health_status.uptime.as_secs() > 0);

    let statistics = server.server_statistics().await;
    assert_eq!(statistics.server_info.name, "test-server");
    assert_eq!(statistics.server_info.version, "1.0.0");
    assert_eq!(statistics.registered_tools.len(), 0);
    assert_eq!(statistics.registered_resources.len(), 0);
}

/// Test error handling for invalid tool calls
#[tokio::test]
async fn error_handling_invalid_tool() {
    use docs_mcp::mcp::server::MessageHandler;

    let server = Arc::new(
        McpServer::new("test-server".to_string(), "1.0.0".to_string())
            .expect("Failed to create MCP server"),
    );

    let handler = MessageHandler::new(Arc::clone(&server));
    let params = Some(json!({
        "name": "nonexistent_tool",
        "arguments": {}
    }));

    let result = handler.handle_call_tool(params).await;
    assert!(result.is_err());

    let error_message = result.expect_err("is error").to_string();
    assert!(error_message.contains("Tool not found"));
}

/// Test JSON schema validation for tool parameters
#[tokio::test]
async fn tool_parameter_validation() {
    let list_tool = ListSitesHandler::tool_definition();

    // Verify tool definition structure
    assert_eq!(list_tool.name, "list_sites");
    assert!(list_tool.description.is_some());

    let schema = list_tool.input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());

    // list_sites should have no required parameters
    let properties = schema["properties"].as_object().expect("is map");
    assert!(properties.is_empty());
}

/// Test concurrent tool operations (simulated)
#[tokio::test]
async fn concurrent_tool_operations() {
    let (_temp_dir, _config, database) = setup_test_environment().await;

    // Execute multiple tool calls concurrently
    let mut handles = Vec::new();

    for i in 0..5 {
        let handler_clone = ListSitesHandler::new(Arc::clone(&database));
        let handle = tokio::spawn(async move {
            let params = CallToolParams {
                name: format!("list_sites_{}", i),
                arguments: Some(HashMap::new()),
            };

            handler_clone.handle(params).await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.expect("Task failed");
        assert!(result.is_ok());

        let tool_result = result.expect("tool call succeeded");
        assert_eq!(tool_result.is_error, Some(false));
    }
}

/// Test server graceful shutdown behavior
#[tokio::test]
async fn server_graceful_shutdown() {
    let server = Arc::new(
        McpServer::new("test-server".to_string(), "1.0.0".to_string())
            .expect("Failed to create MCP server"),
    );

    // Simulate connection state changes during shutdown
    {
        let mut state = server.connection_state.write().await;
        *state = docs_mcp::mcp::server::ConnectionState::Ready;
    }

    assert_eq!(
        server.connection_state().await,
        docs_mcp::mcp::server::ConnectionState::Ready
    );

    {
        let mut state = server.connection_state.write().await;
        *state = docs_mcp::mcp::server::ConnectionState::Closed;
    }

    assert_eq!(
        server.connection_state().await,
        docs_mcp::mcp::server::ConnectionState::Closed
    );
}

/// Test production-ready error handling and resilience
#[tokio::test]
async fn production_error_handling() {
    use docs_mcp::mcp::server::MessageHandler;

    let server = Arc::new(
        McpServer::new("test-server".to_string(), "1.0.0".to_string())
            .expect("Failed to create MCP server"),
    );

    let handler = MessageHandler::new(Arc::clone(&server));

    // Test handling of malformed tool call parameters
    let malformed_params = Some(json!({
        "invalid": "parameters"
        // Missing required 'name' field
    }));

    let result = handler.handle_call_tool(malformed_params).await;
    assert!(result.is_err());

    // Test error recovery with valid parameters after error
    let valid_params = Some(json!({
        "name": "nonexistent_tool",
        "arguments": {}
    }));

    let result2 = handler.handle_call_tool(valid_params).await;
    assert!(result2.is_err());

    // Verify server is still functional after errors
    let health_status = server.health_status().await;
    assert_eq!(
        health_status.connection_state,
        docs_mcp::mcp::server::ConnectionState::Uninitialized
    );
}

/// Test server health monitoring under stress
#[tokio::test]
async fn server_resilience_under_load() {
    let (_temp_dir, _config, database) = setup_test_environment().await;

    let server = Arc::new(
        McpServer::new("stress-test-server".to_string(), "1.0.0".to_string())
            .expect("Failed to create MCP server"),
    );

    // Register tools
    let list_handler = ListSitesHandler::new(Arc::clone(&database));
    let list_tool = ListSitesHandler::tool_definition();

    server
        .register_tool(list_tool, list_handler)
        .await
        .expect("Failed to register tool");

    // Simulate high load with concurrent operations
    let mut handles = Vec::new();

    for i in 0..50 {
        let server_clone = Arc::clone(&server);
        let handle = tokio::spawn(async move {
            let health_status = server_clone.health_status().await;
            assert_eq!(health_status.tools_registered, 1);

            let statistics = server_clone.server_statistics().await;
            assert_eq!(statistics.server_info.name, "stress-test-server");

            i // Return task number for verification
        });
        handles.push(handle);
    }

    // Wait for all operations and verify they completed successfully
    let mut completed_tasks = Vec::new();
    for handle in handles {
        let result = handle.await.expect("Task should complete successfully");
        completed_tasks.push(result);
    }

    // Verify all tasks completed
    assert_eq!(completed_tasks.len(), 50);
    completed_tasks.sort_unstable();
    assert_eq!(completed_tasks, (0..50).collect::<Vec<_>>());

    // Verify server is still healthy after stress test
    let final_health = server.health_status().await;
    assert_eq!(final_health.tools_registered, 1);
}
