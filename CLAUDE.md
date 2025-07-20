# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a documentation indexing and search system that provides an MCP (Model Context Protocol) server for AI assistants to search locally-indexed developer documentation. The system crawls documentation websites, indexes their content using embeddings, and provides semantic search capabilities through an MCP interface.

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

The project follows a modular architecture split across several main components:

- `src/main.rs` - CLI entry point with clap commands
- `src/config/` - TOML configuration management and settings
- `src/database/` - Dual database system (SQLite for metadata, LanceDB for vectors)
- `src/crawler/` - Web crawling with JavaScript rendering support
- `src/embeddings/` - Ollama integration and content chunking
- `src/indexer/` - Background process coordination and queue management
- `src/mcp/` - MCP server implementation and tool definitions

## Architecture Overview

### Technology Stack
- **Language**: Rust (edition 2024)
- **CLI Framework**: Clap
- **Metadata Database**: SQLite (stores sites, crawl queue, indexed chunks)
- **Vector Database**: LanceDB (stores embeddings for semantic search)
- **Embedding Provider**: Ollama (local, using nomic-embed-text model)
- **Web Crawling**: Headless Chrome for JavaScript rendering

### Data Flow
1. **Configuration**: TOML config in `~/.docs-mcp/config.toml`
2. **Crawling**: Sequential crawling with rate limiting (250ms between requests)
3. **Content Processing**: HTML extraction → semantic chunking (500-800 tokens) → embedding generation
4. **Storage**: Metadata in SQLite, vectors in LanceDB at `~/.docs-mcp/`
5. **Search**: MCP server provides semantic search via vector similarity

### Key Architectural Patterns

#### Background Processing
- File locking mechanism (`~/.docs-mcp/.indexer.lock`) for process coordination
- Heartbeat system with SQLite timestamp updates every 30 seconds
- Queue-based indexing with resume capability after interruption
- Auto-start/termination of background processes

#### Database Design
- **Dual database approach**: SQLite for structured metadata, LanceDB for vector embeddings
- **Referential integrity**: `vector_id` links SQLite chunks to LanceDB embeddings
- **Progress tracking**: Real-time status updates and percentage completion

#### Error Handling Strategy
- **Retryable vs Non-retryable**: Network timeouts retry 3x, 4xx errors skip
- **Graceful degradation**: Continue indexing other pages when individual pages fail
- **Recovery patterns**: Embedding generation retries indefinitely for Ollama issues

## Key Implementation Notes

### Content Chunking
Uses semantic chunking strategy preserving heading hierarchy with context breadcrumbs like "Page Title > Section > Subsection" included in each chunk.

### URL Filtering
Only crawls URLs that match or begin with the base URL (excluding trailing filenames), ensuring scope compliance.

### MCP Interface
Provides two main tools:
- `search_docs`: Semantic search with site filtering and relevance scoring
- `list_sites`: Lists available indexed documentation sites

### CLI Commands Structure
- `docs-mcp config`: Interactive setup of Ollama connection
- `docs-mcp add/list/delete/update`: Site management
- `docs-mcp serve`: Start MCP server and background indexer