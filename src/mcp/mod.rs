//! MCP (Model Context Protocol) Server Implementation
//!
//! This module provides a complete MCP server implementation following the
//! JSON-RPC 2.0 specification and MCP protocol version 2025-06-18.

pub mod errors;
pub mod protocol;
pub mod server;
pub mod tools;
pub mod validation;

#[cfg(test)]
mod tests;

pub use errors::{ErrorHandler, McpError, McpResult};
pub use protocol::*;
pub use server::{ConnectionState, McpServer, ResourceHandler, ToolHandler};
pub use tools::{ListSitesHandler, SearchDocsHandler, ToolRegistry};
pub use validation::{McpValidator, protocol_version_error, validation_error};
