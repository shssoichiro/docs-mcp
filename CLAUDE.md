# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a documentation indexing and search system that provides an MCP (Model Context Protocol) server for AI assistants to search locally-indexed developer documentation. The system crawls documentation websites, indexes their content using embeddings, and provides semantic search capabilities through an MCP interface.

## Development Commands

### Building & Running

```bash
cargo build          # Build the project
cargo run            # Compile and run the project
cargo run -- config  # Run interactive configuration setup
cargo run -- --help  # Show CLI help
```

### Testing & Quality

```bash
cargo test           # Run all tests
cargo clippy         # Run Clippy linter
cargo fmt            # Format code
just precommit       # Run all pre-commit checks (fmt, clippy, test)
```

## Project Structure

- `src/main.rs` - CLI entry point with clap commands
- `src/config/` - TOML configuration management and settings
- `src/database/` - Dual database system (SQLite for metadata, LanceDB for vectors)
- `src/crawler/` - Web crawling with JavaScript rendering support
- `src/embeddings/` - Ollama integration and content chunking
- `src/indexer/` - Background process coordination and queue management
- `src/mcp/` - MCP server tool definitions

## Architecture Overview

### Technology Stack

- **Language**: Rust (edition 2024)
- **Metadata Database**: SQLite with sqlx
- **Vector Database**: LanceDB (stores embeddings for semantic search)
- **Embedding Provider**: Ollama (local, using nomic-embed-text model)
- **Web Crawling**: scraper + headless_chrome for JavaScript rendering
- **Configuration**: TOML with serde

### Data Flow

1. **Configuration**: TOML config in `~/.docs-mcp/config.toml`
2. **Crawling**: Sequential crawling with rate limiting and JavaScript rendering
3. **Content Processing**: HTML extraction → heading hierarchy → semantic chunking → Ollama embeddings
4. **Storage**: Metadata in SQLite, vectors in LanceDB at `~/.docs-mcp/`
5. **Search**: MCP server provides semantic search via vector similarity

## Key Implementation Details

### Crawling System

- **Breadth-first crawling** with SQLite queue for URL management
- **Robots.txt compliance** and URL deduplication
- **JavaScript rendering** for dynamic content sites
- **Rate limiting** (250ms between requests) and error recovery

### Content Processing

- **Smart content extraction** from HTML using semantic selectors
- **Heading hierarchy preservation** for contextual chunking
- **Token-aware chunking** (500-800 tokens per chunk)
- **Code block preservation** without splitting

### Browser Integration

- **Browser pool management** with resource limiting
- **JavaScript detection** for React, Vue, Angular sites
- **Fallback architecture** (JS rendering → HTTP client)
- **Resource cleanup** with proper Drop implementations

### Vector Storage (LanceDB)

- **Dynamic vector dimensions** with auto-detection
- **Cosine similarity search** with relevance scoring
- **Site filtering** and metadata queries
- **Database optimization** and corruption recovery

### CLI Commands

- `docs-mcp config [--show]`: Configure Ollama connection
- `docs-mcp add <url> [--name <name>]`: Add documentation site
- `docs-mcp list`: List indexed sites
- `docs-mcp delete <site>`: Delete site with cleanup
- `docs-mcp update <site>`: Re-index site
- `docs-mcp status`: Show pipeline status
- `docs-mcp serve`: Start MCP server

### MCP Tools

- **search_docs**: Semantic search with site filtering
- **list_sites**: List available indexed documentation sites

## Configuration System

- **TOML format** with cross-platform paths
- **Interactive setup** with validation
- **Graceful fallbacks** for missing configs

## Error Handling

Centralized `DocsError` enum categorizing errors by domain:

- `Config`, `Database`, `Network`, `Embedding`, `Crawler`, `Mcp`

## Working with sqlx

- Use in-memory database for integration tests
- Query macros for type safety (`.sqlx/test.db` for verification)
- All datetimes stored as UTC using `NaiveDateTime`
- Migrations in `src/database/sqlite/migrations/`

## Quality Assurance

**Always run `just precommit` before committing** to ensure:

1. `cargo fmt` - Code formatting
2. `cargo clippy --tests --benches -- -D warnings` - Strict linting
3. `cargo test` - Full test suite
