//! MCP (Model Context Protocol) Server Implementation
//!
//! This module provides a complete MCP server implementation following the
//! JSON-RPC 2.0 specification and MCP protocol version 2025-06-18.

pub mod tools;

#[cfg(test)]
mod tests;

pub use tools::{ListSitesHandler, SearchDocsHandler};
