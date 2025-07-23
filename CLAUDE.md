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
3. **Content Processing**: HTML extraction → heading hierarchy detection → semantic chunking (500-800 tokens) → Ollama embedding generation
4. **Storage**: Metadata in SQLite, vectors in LanceDB at `~/.docs-mcp/`
5. **Search**: MCP server provides semantic search via vector similarity

### Embedding Pipeline

- **Chunk Processing**: ContentChunk objects with preserved metadata (heading paths, token counts)
- **Ollama Integration**: Batch processing with configurable sizes and retry logic
- **Model Validation**: Ensures nomic-embed-text:latest model availability before processing
- **Error Recovery**: Exponential backoff for transient failures, graceful degradation for individual chunk failures

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

### Ollama API Client Implementation (`src/embeddings/ollama.rs`)

The Ollama client provides a robust interface for generating embeddings with comprehensive error handling and performance optimization:

#### Core Features

- **HTTP Client**: Uses ureq 3.0 with Agent-based connection pooling and timeout configuration
- **Model Management**: Validates model availability, lists available models, and performs health checks
- **Batch Processing**: Processes multiple text chunks efficiently with configurable batch sizes
- **Error Handling**: Distinguishes between retryable (5xx, transport) and non-retryable (4xx) errors
- **Retry Logic**: Exponential backoff for transient failures with configurable retry attempts
- **Connection Health**: Ping functionality and comprehensive health monitoring

#### API Client Architecture

- `OllamaClient` struct with configuration-based initialization
- Builder pattern for timeout and retry configuration
- Integration with existing `Config` system for host, port, model, and batch size settings
- Proper resource management with connection reuse and cleanup

#### Embedding Generation

- **Single Embeddings**: Generate embeddings for individual text strings
- **Batch Processing**: Process multiple texts in configurable batch sizes (default: 64)
- **ContentChunk Integration**: Seamless integration with chunking system, preserving metadata
- **Model Validation**: Ensures requested model (nomic-embed-text:latest) is available

#### Error Recovery Patterns

- Server errors (5xx) and transport errors trigger exponential backoff retry
- Client errors (4xx) fail immediately without retry
- Connection failures handled gracefully with detailed error reporting
- Service unavailability detected and reported with actionable information

#### Testing Coverage

- **Unit Tests**: 3 tests covering client configuration and data structures
- **Integration Tests**: 8 comprehensive tests with real Ollama instance including:
  - Health check and model validation
  - Single and batch embedding generation
  - ContentChunk processing and metadata preservation
  - Large batch processing with similarity validation
  - Error recovery with invalid models
  - Empty input handling

### LanceDB Vector Storage Implementation (`src/database/lancedb/`)

The LanceDB integration provides a complete vector storage and search system with production-ready features:

#### Core Architecture

- **VectorStore struct**: Main interface for vector database operations with dynamic dimension support
- **EmbeddingRecord/ChunkMetadata**: Complete data structures matching SPEC.md requirements
- **Arrow Integration**: Proper FixedSizeListArray usage for efficient vector storage
- **Async Design**: Full async/await support with tokio compatibility

#### Dynamic Vector Dimensions

- **Auto-Detection**: Automatically detects vector dimensions from first batch of embeddings
- **Schema Recreation**: Dynamically recreates database schema when dimensions change
- **Test/Production Support**: Seamlessly handles 5-dimensional test vectors and 768-dimensional production vectors
- **Dimension Persistence**: Remembers vector dimensions across database sessions

#### Vector Storage and Retrieval

- **Batch Processing**: Efficient storage of multiple embeddings with metadata preservation
- **Metadata Integration**: Complete ChunkMetadata storage including heading paths, content, tokens
- **UUID Management**: Automatic ID generation and management for vector records
- **Schema Validation**: Ensures data consistency with Arrow schema validation

#### Similarity Search Features

- **Cosine Similarity**: Fast vector similarity search using LanceDB's optimized algorithms
- **Relevance Scoring**: Converts distance metrics to intuitive similarity scores (1.0 - distance)
- **Site Filtering**: Filter search results by site_id using SQL-like predicates
- **Result Limiting**: Configurable result limits with performance optimization
- **Metadata Queries**: Search and filter by any metadata field

#### Database Maintenance and Optimization

- **Vector Indexing**: Creates performance-optimized vector indexes (requires 256+ records for PQ training)
- **Database Optimization**: Compaction and reorganization for optimal performance
- **Corruption Recovery**: Automatic detection and recovery from database corruption
- **Site Deletion**: Clean removal of all vectors and metadata for specific sites
- **Health Checking**: Comprehensive database integrity validation

#### Error Handling and Recovery

- **Corruption Detection**: Automatic detection of database corruption with backup/restore
- **Schema Validation**: Ensures Arrow schema consistency across operations
- **Connection Recovery**: Robust connection management with retry logic
- **Transaction Safety**: Atomic operations with proper error rollback

#### Testing Coverage

- **Unit Tests**: 8 comprehensive unit tests covering all vector operations
- **Integration Tests**: 9 integration tests with realistic 768-dimensional data:
  - Complete documentation dataset storage and search
  - Search relevance ranking and accuracy validation
  - Large batch processing (300+ records) with performance testing
  - Metadata preservation and filtering validation
  - Site deletion integrity and cleanup verification
  - Vector index creation and performance optimization
  - Database optimization and maintenance operations
  - Corruption recovery and persistence testing
  - Concurrent access simulation and safety validation

#### Performance Characteristics

- **Large Dataset Support**: Tested with 300+ records and 768-dimensional vectors
- **Memory Efficiency**: Optimized memory usage during batch operations
- **Search Performance**: Sub-second search response times with proper indexing
- **Scalability**: Designed for production workloads with thousands of documents

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
- **Integration Tests**: Cross-module functionality or functionality which tests against a real server (`tests` directory)
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
- ✅ **Ollama API Client and Embedding Generation** (`src/embeddings/ollama.rs`)
  - ✅ Complete HTTP client using ureq 3.0 with proper timeout configuration
  - ✅ Full embedding generation for single texts and batch processing
  - ✅ Model availability checking and validation with health checks
  - ✅ Comprehensive error handling for HTTP status codes and transport errors
  - ✅ Retry logic with exponential backoff for transient failures
  - ✅ Rate limiting compliance with configurable batch sizes
  - ✅ Integration with ContentChunk system preserving metadata
  - ✅ 8 comprehensive integration tests with real Ollama instance
- ✅ **LanceDB Vector Storage System** (`src/database/lancedb/`)
  - ✅ Complete vector database integration with Arrow/LanceDB 0.21
  - ✅ Dynamic vector dimension support (auto-detects 5-dim for tests, 768-dim for production)
  - ✅ Comprehensive vector storage with metadata preservation (EmbeddingRecord/ChunkMetadata)
  - ✅ Vector similarity search with cosine similarity and relevance scoring
  - ✅ Site-based filtering and flexible metadata queries
  - ✅ Batch processing for efficient embedding storage
  - ✅ Database maintenance: optimization, indexing, corruption recovery
  - ✅ Vector index creation for improved search performance (requires 256+ records)
  - ✅ 8 unit tests and 9 integration tests covering all functionality
  - ✅ Production-ready with 768-dimensional nomic-embed-text compatibility
- ✅ **Complete Background Indexing System** (`src/indexer/mod.rs`)
  - ✅ File locking mechanism with `~/.docs-mcp/.indexer.lock` for process coordination
  - ✅ Heartbeat system with SQLite timestamp updates every 30 seconds
  - ✅ Complete indexing pipeline from crawled content to embeddings
  - ✅ Cross-database consistency validation between SQLite and LanceDB
  - ✅ Auto-start/termination logic with queue management
  - ✅ Resource management and error recovery throughout pipeline
  - ✅ 3 comprehensive unit tests covering indexer creation, lock files, and status
- ✅ **Cross-Database Consistency Validation** (`src/indexer/consistency.rs`)
  - ✅ ConsistencyValidator with comprehensive validation between SQLite and LanceDB
  - ✅ Orphaned embedding cleanup and missing embedding regeneration
  - ✅ Site-level consistency checking with detailed reporting
  - ✅ 3 unit tests covering consistency reports and validation logic
- ✅ **Enhanced CLI Commands** (`src/commands.rs`)
  - ✅ Comprehensive `docs-mcp list` with site statistics, progress, and heartbeat monitoring
  - ✅ Complete `docs-mcp status` with pipeline health checking and actionable next steps
  - ✅ Database connectivity checking, Ollama health monitoring, and consistency validation
  - ✅ Real-time progress tracking and error reporting

### In Progress / Planned Components

- 🚧 MCP server implementation
- 🚧 Cleanup procedures for failed indexing operations

### CLI Commands Implementation Status

- ✅ `docs-mcp config [--show]`: Interactive setup of Ollama connection or show current config
- ✅ `docs-mcp add <url> [--name <name>]`: Add new documentation site (fully implemented)
- ✅ `docs-mcp list`: List all indexed documentation sites (comprehensive implementation with statistics and monitoring)
- ✅ `docs-mcp status`: Show detailed pipeline status with health checks and consistency validation
- 🚧 `docs-mcp delete <site>`: Delete a documentation site (placeholder implementation)
- 🚧 `docs-mcp update <site>`: Update/re-index a documentation site (placeholder implementation)
- 🚧 `docs-mcp serve [--port <port>]`: Start MCP server and background indexer (placeholder)
