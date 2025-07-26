//! MCP Server Implementation
//!
//! This module provides the core MCP server framework with connection handling,
//! message routing, and protocol compliance.

use crate::mcp::protocol::*;
use crate::mcp::validation::McpValidator;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// MCP Server state and configuration
pub struct McpServer {
    /// Server implementation information
    pub server_info: Implementation,
    /// Server capabilities
    pub capabilities: ServerCapabilities,
    /// Registered tools
    pub tools: Arc<RwLock<HashMap<String, Tool>>>,
    /// Registered resources
    pub resources: Arc<RwLock<HashMap<String, Resource>>>,
    /// Tool handlers
    pub tool_handlers: Arc<RwLock<HashMap<String, Box<dyn ToolHandler>>>>,
    /// Resource handlers
    pub resource_handlers: Arc<RwLock<HashMap<String, Box<dyn ResourceHandler>>>>,
    /// Connection state
    pub connection_state: Arc<RwLock<ConnectionState>>,
    /// Message validator
    pub validator: Arc<McpValidator>,
}

/// Connection state tracking
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Uninitialized,
    Initializing,
    Ready,
    Closed,
}

/// Tool handler trait for implementing tool execution
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn handle(&self, params: CallToolParams) -> Result<CallToolResult>;
}

/// Resource handler trait for implementing resource access
#[async_trait]
pub trait ResourceHandler: Send + Sync {
    async fn handle(&self, uri: &str) -> Result<Value>;
}

/// Message handler for processing incoming messages
pub struct MessageHandler {
    server: Arc<McpServer>,
}

impl McpServer {
    /// Create a new MCP server
    #[inline]
    pub fn new(name: String, version: String) -> Result<Self> {
        let server_info = Implementation { name, version };

        let capabilities = ServerCapabilities {
            experimental: None,
            logging: Some(LoggingCapability {}),
            prompts: None,
            resources: Some(ResourcesCapability {
                subscribe: Some(false),
                list_changed: Some(false),
            }),
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
        };

        let validator = McpValidator::new()?;

        Ok(Self {
            server_info,
            capabilities,
            tools: Arc::new(RwLock::new(HashMap::new())),
            resources: Arc::new(RwLock::new(HashMap::new())),
            tool_handlers: Arc::new(RwLock::new(HashMap::new())),
            resource_handlers: Arc::new(RwLock::new(HashMap::new())),
            connection_state: Arc::new(RwLock::new(ConnectionState::Uninitialized)),
            validator: Arc::new(validator),
        })
    }

    /// Register a tool with the server
    #[inline]
    pub async fn register_tool<H>(&self, tool: Tool, handler: H) -> Result<()>
    where
        H: ToolHandler + 'static,
    {
        let tool_name = tool.name.clone();

        {
            let mut tools = self.tools.write().await;
            tools.insert(tool_name.clone(), tool);
        }

        {
            let mut handlers = self.tool_handlers.write().await;
            handlers.insert(tool_name.clone(), Box::new(handler));
        }

        debug!("Registered tool: {}", tool_name);
        Ok(())
    }

    /// Register a resource with the server
    #[inline]
    pub async fn register_resource<H>(&self, resource: Resource, handler: H) -> Result<()>
    where
        H: ResourceHandler + 'static,
    {
        let resource_uri = resource.uri.clone();

        {
            let mut resources = self.resources.write().await;
            resources.insert(resource_uri.clone(), resource);
        }

        {
            let mut handlers = self.resource_handlers.write().await;
            handlers.insert(resource_uri.clone(), Box::new(handler));
        }

        debug!("Registered resource: {}", resource_uri);
        Ok(())
    }

    /// Start the server using stdio transport
    #[inline]
    pub async fn serve_stdio(self: Arc<Self>) -> Result<()> {
        info!("Starting MCP server with stdio transport");

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut reader = BufReader::new(stdin);

        // Read and process messages from stdin
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    info!("EOF reached, closing connection");
                    break;
                }
                Ok(_) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // First parse as raw JSON
                    let raw_value: Value = match serde_json::from_str(line) {
                        Ok(value) => value,
                        Err(e) => {
                            error!("Failed to parse JSON: {}", e);
                            let error_response =
                                JsonRpcErrorResponse::new(JsonRpcError::parse_error(), None);
                            self.send_message(
                                &mut stdout,
                                &JsonRpcMessage::ErrorResponse(error_response),
                            )
                            .await?;
                            continue;
                        }
                    };

                    // Validate and parse as MCP message
                    match self.validator.validate_raw_message(&raw_value) {
                        Ok(message) => {
                            let handler = MessageHandler::new(Arc::clone(&self));
                            if let Err(e) = handler.process_message(message, &mut stdout).await {
                                error!("Error processing message: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Message validation failed: {}", e);
                            let error_response =
                                JsonRpcErrorResponse::new(JsonRpcError::invalid_request(), None);
                            self.send_message(
                                &mut stdout,
                                &JsonRpcMessage::ErrorResponse(error_response),
                            )
                            .await?;
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading from stdin: {}", e);
                    break;
                }
            }
        }

        // Update connection state
        {
            let mut state = self.connection_state.write().await;
            *state = ConnectionState::Closed;
        }

        info!("MCP server stopped");
        Ok(())
    }

    /// Send a message to the client
    async fn send_message<W>(&self, writer: &mut W, message: &JsonRpcMessage) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let json = serde_json::to_string(message)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }

    /// Get current connection state
    #[inline]
    pub async fn connection_state(&self) -> ConnectionState {
        self.connection_state.read().await.clone()
    }
}

impl Clone for McpServer {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            server_info: self.server_info.clone(),
            capabilities: self.capabilities.clone(),
            tools: Arc::clone(&self.tools),
            resources: Arc::clone(&self.resources),
            tool_handlers: Arc::clone(&self.tool_handlers),
            resource_handlers: Arc::clone(&self.resource_handlers),
            connection_state: Arc::clone(&self.connection_state),
            validator: Arc::clone(&self.validator),
        }
    }
}

impl MessageHandler {
    /// Create a new message handler
    #[inline]
    pub fn new(server: Arc<McpServer>) -> Self {
        Self { server }
    }

    /// Process an incoming message
    #[inline]
    pub async fn process_message<W>(&self, message: JsonRpcMessage, writer: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        match message {
            JsonRpcMessage::Request(request) => self.handle_request(request, writer).await,
            JsonRpcMessage::Notification(notification) => {
                self.handle_notification(notification).await
            }
            JsonRpcMessage::Response(_) | JsonRpcMessage::ErrorResponse(_) => {
                warn!("Received unexpected response message from client");
                Ok(())
            }
        }
    }

    /// Handle a JSON-RPC request
    async fn handle_request<W>(&self, request: JsonRpcRequest, writer: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "tools/list" => self.handle_list_tools().await,
            "tools/call" => self.handle_call_tool(request.params).await,
            "resources/list" => self.handle_list_resources().await,
            "resources/read" => self.handle_read_resource(request.params).await,
            "ping" => self.handle_ping().await,
            _ => {
                let error = JsonRpcError::method_not_found();
                return self
                    .send_error_response(writer, error, Some(request.id))
                    .await;
            }
        };

        match response {
            Ok(result) => {
                let response = JsonRpcResponse::new(result, request.id);
                self.send_response(writer, JsonRpcMessage::Response(response))
                    .await
            }
            Err(e) => {
                error!("Error handling request {}: {}", request.method, e);
                let error = JsonRpcError::internal_error(Some(e.to_string()));
                self.send_error_response(writer, error, Some(request.id))
                    .await
            }
        }
    }

    /// Handle a JSON-RPC notification
    async fn handle_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        match notification.method.as_str() {
            "initialized" => self.handle_initialized().await,
            "notifications/cancelled" => {
                debug!("Received cancellation notification");
                Ok(())
            }
            _ => {
                warn!("Unknown notification method: {}", notification.method);
                Ok(())
            }
        }
    }

    /// Handle initialize request
    #[inline]
    pub async fn handle_initialize(&self, params: Option<Value>) -> Result<Value> {
        let params: InitializeParams = match params {
            Some(p) => serde_json::from_value(p)?,
            None => return Err(anyhow!("Initialize request missing parameters")),
        };

        // Check protocol version compatibility
        if !self
            .server
            .validator
            .is_protocol_version_supported(&params.protocol_version)
        {
            let supported = self.server.validator.supported_protocol_versions();
            return Err(anyhow!(
                "Unsupported protocol version: {}. Supported: {}",
                params.protocol_version,
                supported.join(", ")
            ));
        }

        // Update connection state
        {
            let mut state = self.server.connection_state.write().await;
            *state = ConnectionState::Initializing;
        }

        let result = InitializeResult {
            protocol_version: MCP_VERSION.to_string(),
            capabilities: self.server.capabilities.clone(),
            server_info: self.server.server_info.clone(),
            instructions: Some("Documentation search MCP server".to_string()),
        };

        info!("Client initialized: {}", params.client_info.name);
        Ok(serde_json::to_value(result)?)
    }

    /// Handle initialized notification
    async fn handle_initialized(&self) -> Result<()> {
        // Update connection state to ready
        {
            let mut state = self.server.connection_state.write().await;
            *state = ConnectionState::Ready;
        }

        info!("Server ready to handle requests");
        Ok(())
    }

    /// Handle list tools request
    #[inline]
    pub async fn handle_list_tools(&self) -> Result<Value> {
        let tools = self.server.tools.read().await;
        let tools_vec: Vec<Tool> = tools.values().cloned().collect();

        let result = ListToolsResult { tools: tools_vec };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle call tool request
    #[inline]
    pub async fn handle_call_tool(&self, params: Option<Value>) -> Result<Value> {
        let params: CallToolParams = match params {
            Some(p) => serde_json::from_value(p)?,
            None => return Err(anyhow!("Tool call request missing parameters")),
        };

        let handlers = self.server.tool_handlers.read().await;
        let handler = handlers
            .get(&params.name)
            .ok_or_else(|| anyhow!("Tool not found: {}", params.name))?;

        let result = handler.handle(params).await?;
        Ok(serde_json::to_value(result)?)
    }

    /// Handle list resources request
    async fn handle_list_resources(&self) -> Result<Value> {
        let resources = self.server.resources.read().await;
        let resources_vec: Vec<Resource> = resources.values().cloned().collect();

        let result = ListResourcesResult {
            resources: resources_vec,
        };
        Ok(serde_json::to_value(result)?)
    }

    /// Handle read resource request
    #[expect(clippy::unused_async, reason = "this function is WIP")]
    async fn handle_read_resource(&self, _params: Option<Value>) -> Result<Value> {
        // This would need to be implemented based on the specific resource request format
        // For now, return a placeholder
        Ok(serde_json::json!({"content": []}))
    }

    /// Handle ping request
    #[inline]
    #[expect(clippy::unused_async, reason = "this function is WIP")]
    pub async fn handle_ping(&self) -> Result<Value> {
        Ok(serde_json::json!({}))
    }

    /// Send a response message
    async fn send_response<W>(&self, writer: &mut W, message: JsonRpcMessage) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        self.server.send_message(writer, &message).await
    }

    /// Send an error response
    async fn send_error_response<W>(
        &self,
        writer: &mut W,
        error: JsonRpcError,
        id: Option<RequestId>,
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let error_response = JsonRpcErrorResponse::new(error, id);
        let message = JsonRpcMessage::ErrorResponse(error_response);
        self.server.send_message(writer, &message).await
    }
}
