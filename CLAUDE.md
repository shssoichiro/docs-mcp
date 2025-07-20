# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust project named `docs-mcp` that appears to be in the early stages of development. The project uses Rust edition 2024 and currently contains a simple "Hello, world!" application.

## Development Commands

### Building
```bash
cargo build          # Build the project
cargo build --release # Build optimized release version
```

### Running
```bash
cargo run            # Compile and run the project
```

### Testing
```bash
cargo test           # Run all tests
cargo test <test_name> # Run a specific test
```

### Linting and Formatting
```bash
cargo check          # Quick compile check without producing executable
cargo clippy         # Run Clippy linter
cargo fmt            # Format code according to Rust style guidelines
```

## Project Structure

- `src/main.rs` - Main entry point with basic "Hello, world!" implementation
- `Cargo.toml` - Project configuration and dependencies
- `Cargo.lock` - Dependency lock file (auto-generated)

## Architecture Notes

This is currently a minimal Rust binary project. The codebase structure suggests this may be intended as an MCP (Model Context Protocol) documentation tool, but the implementation is in its initial stages.

As the project evolves, key architectural decisions will likely include:
- MCP protocol implementation patterns
- Documentation generation strategies  
- Error handling and logging approaches
- Configuration management