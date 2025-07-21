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
cargo run -- config  # Run interactive configuration setup
cargo run -- config --show # Display current configuration
cargo run -- --help  # Show CLI help
```

### Testing

```bash
cargo test           # Run all tests
cargo test <test_name> # Run a specific test
cargo test config::   # Run all config module tests
cargo test integration_tests::config_file_persistence # Run specific test
```

### Linting and Formatting

```bash
cargo check          # Quick compile check without producing executable
cargo clippy         # Run Clippy linter (extensive lint rules configured)
cargo clippy --tests --benches -- -D warnings # Run clippy with stricter warnings for tests
cargo fmt            # Format code according to Rust style guidelines
just precommit       # Run all pre-commit checks (fmt, clippy, test)
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
- **CLI Framework**: Clap (with derive features for command parsing)
- **Metadata Database**: SQLite with sqlx (stores sites, crawl queue, indexed chunks)
- **Vector Database**: LanceDB (stores embeddings for semantic search)
- **Embedding Provider**: Ollama (local, using nomic-embed-text model)
- **Web Crawling**: scraper crate for HTML extraction with semantic content processing
- **HTTP Client**: ureq for web requests
- **Configuration**: TOML with serde for serialization

### Data Flow

1. **Configuration**: TOML config in `~/.docs-mcp/config.toml`
2. **Crawling**: Sequential crawling with rate limiting (250ms between requests)
3. **Content Processing**: HTML extraction → heading hierarchy detection → semantic chunking (500-800 tokens) → embedding generation
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

### Site Crawling Implementation (`src/crawler/mod.rs`)

The site crawler implements a complete breadth-first crawling system with the following key features:

- **Breadth-First Algorithm**: Uses SQLite queue to process URLs in discovery order
- **URL Deduplication**: Prevents crawling the same URL multiple times using HashSet
- **Robots.txt Compliance**: Fetches and respects robots.txt rules before crawling
- **Progress Tracking**: Real-time updates to site status and progress percentages
- **Error Handling**: Distinguishes between retryable (5xx, timeouts) and non-retryable (4xx) errors
- **Rate Limiting**: 250ms delay between requests (configurable)
- **Queue Management**: Stores crawl progress in SQLite for resume capability

#### Integration Tests with Mock Servers

Comprehensive integration tests using `wiremock` library cover:

- **Basic crawling workflow**: Multi-page site with internal links
- **Robots.txt compliance**: Proper blocking and respect for robots.txt rules
- **Error handling**: 404 responses and HTTP error codes
- **Content extraction**: Complex HTML structures with headings and code blocks
- **Database operations**: Full SQLite integration with shared cache for testing

### Content Extraction and Processing

#### HTML Content Extraction (`src/crawler/extractor.rs`)

The content extraction system provides sophisticated HTML parsing and content structuring:

- **Smart Content Detection**: Automatically identifies main content areas using semantic selectors (`main`, `[role="main"]`, `.content`, etc.)
- **Heading Hierarchy Preservation**: Builds breadcrumb paths like "Page Title > Section > Subsection" from H1-H6 structure
- **Code Block Identification**: Detects and preserves code blocks, syntax highlighting, and preformatted content
- **Text Normalization**: Handles whitespace cleanup, Unicode normalization, and malformed HTML gracefully
- **Metadata Extraction**: Extracts page titles, meta tags, and structured document information
- **Configurable Filtering**: Optional inclusion/exclusion of navigation, footer, and auxiliary content

#### Content Chunking (`src/embeddings/chunking.rs`)

The chunking system implements semantic content splitting for optimal embedding generation:

- **Token-Aware Chunking**: Targets 650 tokens per chunk (configurable: 500-800 target, 1000 max, 100 min)
- **Semantic Boundaries**: Splits on heading boundaries, paragraph breaks, and sentence boundaries when possible
- **Code Block Preservation**: Never splits within code blocks, handling them as atomic units
- **Contextual Chunks**: Each chunk includes page title and heading path for context
- **Overlap Support**: Configurable overlap between adjacent chunks (default: 50 tokens)
- **Multiple Splitting Strategies**: Paragraph-based → sentence-based → word-based as fallbacks
- **Smart Merging**: Automatically merges small chunks and handles oversized content

#### Key Features

- **Malformed HTML Handling**: Gracefully processes broken or incomplete HTML structures
- **Performance Optimized**: Efficient parsing and chunking for large documentation sites
- **Test Coverage**: Comprehensive test suite with 72 passing tests covering various document structures
- **Memory Efficient**: Processes content without excessive memory allocation

### URL Filtering

Only crawls URLs that match or begin with the base URL (excluding trailing filenames), ensuring scope compliance.

### MCP Interface

Provides two main tools:

- `search_docs`: Semantic search with site filtering and relevance scoring
- `list_sites`: Lists available indexed documentation sites

### CLI Commands Structure

- `docs-mcp config [--show]`: Interactive setup of Ollama connection or show current config
- `docs-mcp add <url> [--name <name>]`: Add new documentation site
- `docs-mcp list`: List all indexed documentation sites
- `docs-mcp delete <site>`: Delete a documentation site
- `docs-mcp update <site>`: Update/re-index a documentation site
- `docs-mcp serve [--port <port>]`: Start MCP server and background indexer (default port: 3000)

**Note**: Most CLI commands beyond `config` are currently placeholders and not yet implemented.

## Configuration System

### Architecture

The configuration system uses a layered approach:

- **TOML Format**: Human-readable configuration files
- **Cross-Platform Paths**: `~/.docs-mcp/config.toml` (Unix) or `%APPDATA%/docs-mcp/config.toml` (Windows)
- **Interactive Setup**: User-friendly prompts with validation and defaults
- **Graceful Fallbacks**: Missing config files use sensible defaults

### Key Components

- `Config` struct: Main configuration container with TOML serde support
- `OllamaConfig`: Ollama-specific settings (host, port, model, batch_size)
- `ConfigError`: Comprehensive error types with context
- Interactive prompts with real-time validation and Ollama connectivity testing

### Implementation Details

- **Validation Strategy**: Multi-layered validation at parse-time, setter methods, and save operations
- **Error Handling**: Uses `anyhow` for context and `thiserror` for typed errors
- **URL Generation**: Builds and validates Ollama connection URLs
- **Setter Pattern**: Validates before applying changes to prevent invalid states

## Error Handling Architecture

### Error Types

The system uses a centralized `DocsError` enum in `lib.rs` that categorizes errors by domain:

- `Config`: Configuration validation and file operations
- `Database`: SQLite and LanceDB operations
- `Network`: HTTP requests and connectivity issues
- `Embedding`: Ollama integration and embedding generation
- `Crawler`: Web crawling and content extraction
- `Mcp`: MCP server and protocol handling

### Error Propagation

- Uses `anyhow::Result` for most operations with rich context
- Module-specific error types (like `ConfigError`) for detailed error information
- Graceful degradation: system continues operating when individual components fail

## Testing Strategy

### Test Organization

- **Unit Tests**: In each module (`#[cfg(test)] mod tests`)
- **Integration Tests**: Cross-module functionality (`config/tests.rs`)
- **Cross-Platform Tests**: Platform-specific behavior validation
- **Edge Case Testing**: Boundary conditions, invalid inputs, error scenarios

### Key Test Patterns

- **Comprehensive Config Testing**: 20+ test cases covering TOML parsing, validation, cross-platform paths
- **Mock-Free Design**: Uses real filesystem operations with temporary directories
- **Property-Based Validation**: Tests boundary conditions (port ranges 1-65535, batch sizes 1-1000)
- **Error Message Validation**: Ensures meaningful error messages for user feedback

## Code Quality Standards

### Clippy Configuration

Extensive clippy rules are configured in `Cargo.toml` covering:

- **Performance**: Detects inefficient patterns and unnecessary allocations
- **Correctness**: Identifies potential bugs and unsafe patterns
- **Readability**: Enforces consistent code style and clear intent
- **Safety**: Prevents common Rust pitfalls and undefined behavior

### Key Patterns

- **SOLID Principles**: Single responsibility, validation separation, dependency injection
- **Inline Annotations**: Publicly exported functions marked with `#[inline]` to enable inline analysis in the CLI binary
- **Comprehensive Validation**: All user inputs validated before processing
- **Resource Management**: Proper cleanup and error recovery throughout

## Working with sqlx

- Use an in-memory database as much as possible for integration tests
- Use query macros for additional type safety. There is a database at `.sqlx/test.db` which sqlx will check against for its query verification.
  - To run migrations against this test DB, use `cargo sqlx migrate run --source ./src/database/sqlite/migrations`
- Assume all datetimes in the SQLite database are UTC. sqlx's sqlite adapter does not provide support for `DateTime<Utc>`, so we are using `NaiveDateTime` instead and storing all datetimes in the database as UTC.
- The initial schema is defined in `src/database/sqlite/migrations/001_initial_schema.sql` with comprehensive constraints and indexes

## Quality Assurance

### Pre-commit Workflow

The project uses `just precommit` which runs:

1. `cargo fmt` - Automatic code formatting
2. `cargo clippy --tests --benches -- -D warnings` - Strict linting with test coverage
3. `cargo test` - Full test suite

**Always run `just precommit` before committing changes** to ensure code quality standards.

## Implementation Status

### Completed Components

- ✅ CLI framework with clap command parsing and comprehensive tests
- ✅ Configuration system with TOML support, interactive setup, and validation
- ✅ SQLite database schema with migrations and constraints
- ✅ Error handling architecture with centralized `DocsError` enum
- ✅ Project structure and module organization
- ✅ Database operations (SQLite models and queries)
- ✅ Web crawler HTTP client with rate limiting and retry logic
- ✅ HTML content extraction with heading hierarchy preservation
- ✅ Semantic content chunking with code block preservation
- ✅ robots.txt handling and URL filtering
- ✅ **Site crawling integration and orchestration**
  - ✅ Complete breadth-first crawling implementation
  - ✅ URL queue management with SQLite integration
  - ✅ Progress tracking and site status updates
  - ✅ "docs-mcp add" command implementation
  - ✅ Comprehensive error handling and recovery
  - ✅ Integration tests with mock HTTP servers

### In Progress / Planned Components

- 🚧 Ollama integration and embedding generation
- 🚧 Background indexing process coordination
- 🚧 MCP server implementation
- 🚧 Vector database (LanceDB) integration

### CLI Commands Implementation Status

- ✅ `docs-mcp config [--show]`: Interactive setup of Ollama connection or show current config
- ✅ `docs-mcp add <url> [--name <name>]`: Add new documentation site (fully implemented)
- ✅ `docs-mcp list`: List all indexed documentation sites (basic implementation)
- 🚧 `docs-mcp delete <site>`: Delete a documentation site (placeholder)
- 🚧 `docs-mcp update <site>`: Update/re-index a documentation site (placeholder)
- 🚧 `docs-mcp serve [--port <port>]`: Start MCP server and background indexer (placeholder)
