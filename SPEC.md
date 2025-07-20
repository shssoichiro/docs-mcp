# Documentation MCP Server Specification

## Project Overview

This project is a documentation indexing and search system that provides an MCP (Model Context Protocol) server for AI assistants to search locally-indexed developer documentation. The system crawls documentation websites, indexes their content using embeddings, and provides semantic search capabilities through an MCP interface.

## Core Requirements

### Functional Requirements

1. **Documentation Crawling**: Crawl and extract content from various documentation formats including HTML, dynamically-loaded JavaScript content, and plain text files
2. **Local Indexing**: Generate embeddings using Ollama and store them locally with metadata
3. **Semantic Search**: Provide relevance-ranked search results through MCP interface
4. **CLI Management**: Command-line interface for managing indexed sites
5. **Background Processing**: Automatic background indexing with queue management
6. **Cross-Platform**: Support Linux, macOS, and Windows

### Non-Functional Requirements

1. **Performance**: Fast search responses and efficient indexing
2. **Reliability**: Resume interrupted indexing operations
3. **Usability**: Simple setup and configuration process
4. **Storage**: File-based storage requiring no external database servers

## Architecture

### Technology Stack

- **Language**: Rust (edition 2024)
- **CLI Framework**: Clap
- **Metadata Database**: SQLite
- **Vector Database**: LanceDB
- **Embedding Provider**: Ollama (local)
- **Web Crawling**: Chromium-based headless browser for JavaScript rendering

### Project Structure

```
src/
├── main.rs              # CLI entry point with clap commands
├── config/              # Configuration management
│   ├── mod.rs
│   └── settings.rs      # TOML config file handling
├── database/
│   ├── mod.rs           # Database module exports
│   ├── sqlite/          # SQLite operations (metadata, queue, status)
│   │   ├── mod.rs
│   │   ├── models.rs    # Data structures
│   │   └── queries.rs   # SQL operations
│   └── lancedb/         # LanceDB operations (embeddings storage/search)
│       ├── mod.rs
│       └── vector_store.rs
├── crawler/             # Web crawling and content extraction
│   ├── mod.rs
│   ├── extractor.rs     # Content extraction and chunking
│   ├── robots.rs        # robots.txt handling
│   └── browser.rs       # Headless browser management
├── embeddings/          # Ollama integration and embedding generation
│   ├── mod.rs
│   ├── ollama.rs        # Ollama API client
│   └── chunking.rs      # Content chunking strategies
├── indexer/             # Background indexing process coordination
│   ├── mod.rs
│   ├── queue.rs         # Indexing queue management
│   ├── process.rs       # Background process coordination
│   └── progress.rs      # Progress tracking
├── mcp/                 # MCP server implementation
│   ├── mod.rs
│   ├── server.rs        # MCP server
│   └── tools.rs         # MCP tool definitions
└── lib.rs               # Common types and utilities
```

### Data Storage

#### Configuration File Location
- Linux/macOS: `~/.docs-mcp/config.toml`
- Windows: `%APPDATA%/docs-mcp/config.toml`

#### Database Files Location
- SQLite database: `~/.docs-mcp/metadata.db`
- LanceDB files: `~/.docs-mcp/embeddings/`
- Lock file: `~/.docs-mcp/.indexer.lock`

#### Configuration File Format (TOML)
```toml
[ollama]
host = "localhost"
port = 11434
model = "nomic-embed-text"
batch_size = 64
```

### Database Schema

#### SQLite Tables

**sites**
```sql
CREATE TABLE sites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    base_url TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    indexed_date DATETIME,
    status TEXT NOT NULL, -- 'pending', 'indexing', 'completed', 'failed'
    progress_percent INTEGER DEFAULT 0,
    total_pages INTEGER DEFAULT 0,
    indexed_pages INTEGER DEFAULT 0,
    error_message TEXT,
    created_date DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_heartbeat DATETIME
);
```

**crawl_queue**
```sql
CREATE TABLE crawl_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL,
    url TEXT NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'processing', 'completed', 'failed'
    retry_count INTEGER DEFAULT 0,
    error_message TEXT,
    created_date DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (site_id) REFERENCES sites (id)
);
```

**indexed_chunks**
```sql
CREATE TABLE indexed_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL,
    url TEXT NOT NULL,
    page_title TEXT,
    heading_path TEXT, -- breadcrumb like "Page Title > Section > Subsection"
    chunk_content TEXT NOT NULL,
    chunk_index INTEGER NOT NULL, -- order within the page
    vector_id TEXT NOT NULL, -- reference to LanceDB vector
    indexed_date DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (site_id) REFERENCES sites (id)
);
```

#### LanceDB Schema
```rust
struct EmbeddingRecord {
    vector_id: String,        // UUID
    embedding: Vec<f32>,      // embedding vector
    chunk_content: String,    // text content
    metadata: ChunkMetadata,  // site_id, url, title, etc.
}

struct ChunkMetadata {
    site_id: i64,
    url: String,
    page_title: String,
    heading_path: String,
    chunk_index: i32,
}
```

## CLI Interface

### Commands

#### Configuration
```bash
docs-mcp config
```
Interactive configuration setup:
1. Prompt for Ollama host (default: localhost)
2. Prompt for Ollama port (default: 11434)
3. Prompt for embedding model (default: nomic-embed-text)
4. Test connection to Ollama
5. Check if model exists, offer to pull if missing
6. Save configuration

#### Site Management
```bash
# Add new site to index
docs-mcp add <url> <name> [version]
# version defaults to "latest" if not specified

# List all indexed sites
docs-mcp list
# Shows table: ID | Name | Version | URL | Status | Progress | Pages | Indexed Date

# Delete site by name or URL
docs-mcp delete <name_or_url> [version]
# If name matches multiple sites, prompt for version

# Update/re-index site
docs-mcp update <name_or_url> [version]

# Show detailed status
docs-mcp status <name_or_url> [version]
# Shows: detailed progress, error messages, crawl statistics
```

#### MCP Server
```bash
docs-mcp serve [--port PORT]
# Starts MCP server (default port: 8080)
# Also starts background indexer if not running
```

## Crawling Behavior

### URL Filtering Rules
1. Only crawl URLs reachable from base URL (directly or recursively)
2. Only crawl URLs that match or begin with the base URL (excluding trailing filenames)
   - Example: Base URL `https://docs.rs/regex/1.10.6/regex/` only crawls URLs starting with `https://docs.rs/regex/1.10.6/regex/`

### Crawling Configuration
- **Rate Limiting**: 250ms delay between requests
- **User Agent**: Chromium-like user agent string
- **Processing**: Sequential (not concurrent) per site
- **Timeout**: 30 seconds per page load
- **Retry Logic**: Up to 3 retries with 30-second delays for retryable errors (timeouts, 5xx errors)
- **Robots.txt**: Respect robots.txt rules; report as error if site becomes uncrawlable

### Content Extraction

#### Supported Formats
- Rendered HTML (including JavaScript-generated content)
- Plain text files
- Preserve and utilize page titles and heading structures

#### Content Chunking Strategy
1. **Semantic Chunking**: Split by heading hierarchy
2. **Target Size**: 500-800 tokens per chunk
3. **Context Preservation**: Include page title and heading breadcrumb in each chunk
4. **Smart Splitting**: For oversized sections (>1000 tokens), split at paragraph boundaries while preserving code blocks
5. **Overlap Strategy**: Include heading hierarchy in adjacent chunks for context

#### Chunk Format Example
```
Page Title: Python Documentation > Functions > Built-in Functions

# print()

The print() function outputs text to the console...
[content continues]
```

## Background Processing

### Process Coordination
- **Process Discovery**: File locking with `.indexer.lock` file + heartbeat mechanism
- **Lock File**: Exclusive lock held by background indexer process
- **Heartbeat**: Update SQLite with timestamp every 30 seconds
- **Stale Detection**: If lock exists but heartbeat >60 seconds old, start new indexer

### Indexing Queue Management
1. **Queue Processing**: Sequential processing of sites
2. **Progress Tracking**: Save progress after each successfully crawled page
3. **Resume Capability**: Resume indexing from last saved state after interruption
4. **Auto-termination**: Background process exits when queue is empty
5. **Auto-start**: Background process starts when CLI commands run or MCP server starts

### Embedding Generation
- **Batch Processing**: Process 32-64 chunks per batch
- **Error Handling**: 
  - Recoverable errors (Ollama down): Retry every minute
  - Unrecoverable errors: Mark site as failed, store error message
- **Progress Updates**: Update progress in SQLite after each batch

## MCP Server Interface

### Tools Provided

#### search_docs
```json
{
  "name": "search_docs",
  "description": "Search indexed documentation",
  "parameters": {
    "query": {
      "type": "string",
      "description": "Search query"
    },
    "site_id": {
      "type": "integer",
      "description": "Optional: Search specific site by ID"
    },
    "sites_filter": {
      "type": "string", 
      "description": "Optional: Regex pattern to filter sites (e.g., 'docs.rs')"
    },
    "limit": {
      "type": "integer",
      "description": "Maximum number of results (default: 10)"
    }
  }
}
```

**Response Format:**
```json
{
  "results": [
    {
      "content": "chunk content with context",
      "url": "source URL",
      "page_title": "page title",
      "heading_path": "breadcrumb path",
      "site_name": "site name",
      "site_version": "version",
      "relevance_score": 0.85
    }
  ]
}
```

#### list_sites
```json
{
  "name": "list_sites",
  "description": "List available documentation sites",
  "parameters": {}
}
```

**Response Format:**
```json
{
  "sites": [
    {
      "id": 1,
      "name": "Rust Standard Library",
      "version": "1.75",
      "url": "https://doc.rust-lang.org/std/",
      "status": "completed",
      "indexed_date": "2024-01-15T10:30:00Z",
      "page_count": 1250
    }
  ]
}
```

### Server Behavior
- **Filtering**: Only show completed sites to MCP clients
- **Concurrent Operation**: MCP server runs while indexing is in progress
- **Search Results**: Return results sorted by relevance score

## Error Handling

### Crawling Errors

#### Retryable Errors
- Network timeouts
- HTTP 5xx server errors  
- Temporary connection failures

**Handling**: Retry up to 3 times with 30-second delays, then mark page as failed

#### Non-Retryable Errors
- HTTP 4xx client errors (except 429 rate limiting)
- robots.txt restrictions
- Invalid URLs

**Handling**: Log error, mark page as failed, continue with next page

### Embedding Errors

#### Recoverable Errors
- Ollama service unavailable
- Temporary model loading issues
- Network connectivity issues

**Handling**: Retry every minute indefinitely while updating status

#### Unrecoverable Errors
- Model not found and auto-pull disabled
- Invalid chunk content
- Persistent API errors

**Handling**: Mark site indexing as failed, store detailed error message

### Database Errors
- **SQLite Lock Errors**: Retry with exponential backoff
- **Corruption**: Attempt recovery, report to user if unsuccessful
- **Disk Space**: Check available space before operations, warn user

### Configuration Errors
- **Invalid Ollama Config**: Validate during setup, re-run config command
- **Missing Config**: Prompt user to run config command
- **Permission Issues**: Clear error messages with suggested fixes

## Testing Strategy

### Unit Tests
- **Database Operations**: Test SQLite and LanceDB operations
- **Content Extraction**: Test chunking algorithms with various HTML structures
- **URL Filtering**: Test crawling scope rules
- **Configuration**: Test TOML parsing and validation

### Integration Tests
- **End-to-End Crawling**: Test with mock documentation sites
- **Embedding Pipeline**: Test with local Ollama instance
- **MCP Interface**: Test MCP tool implementations
- **CLI Commands**: Test all CLI operations

### Test Data
- **Mock Sites**: Create test HTML with various structures
- **Sample Embeddings**: Pre-computed embeddings for consistent search testing
- **Edge Cases**: Test with large sites, malformed HTML, network failures

### Performance Tests
- **Crawling Speed**: Measure pages/minute with rate limiting
- **Search Performance**: Measure query response times
- **Memory Usage**: Monitor memory consumption during large site indexing
- **Concurrent Operations**: Test MCP server performance during indexing

## Implementation Phases

### Phase 1: Core Infrastructure
1. Project setup with Clap CLI framework
2. Configuration management system
3. SQLite database schema and operations
4. Basic crawling infrastructure

### Phase 2: Content Processing
1. Web crawler with JavaScript rendering
2. Content extraction and chunking
3. robots.txt compliance
4. URL filtering logic

### Phase 3: Embedding Integration
1. Ollama client implementation
2. Embedding generation pipeline
3. LanceDB integration
4. Batch processing system

### Phase 4: Background Processing
1. Process coordination with file locking
2. Indexing queue management
3. Progress tracking and resume capability
4. Error handling and retry logic

### Phase 5: MCP Server
1. MCP protocol implementation
2. Search functionality
3. Site listing tools
4. Result formatting and filtering

### Phase 6: CLI Polish
1. All CLI commands implementation
2. Interactive configuration setup
3. Status reporting and progress display
4. Error message improvements

## Dependencies

### Required Crates
```toml
[dependencies]
clap = { version = "4.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
reqwest = { version = "0.11", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite"] }
lancedb = "0.4"
headless_chrome = "1.0"
scraper = "0.18"
url = "2.5"
regex = "1.10"
uuid = { version = "1.6", features = ["v4"] }
dirs = "5.0"
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
```

## Security Considerations

- **Input Validation**: Validate all URLs and user inputs
- **File System Access**: Restrict file operations to designated directories
- **Network Security**: Implement reasonable timeout and rate limiting
- **Dependency Management**: Regular security audits of dependencies
- **Configuration Security**: Secure storage of configuration files

## Future Enhancements

1. **Multiple Embedding Providers**: Support for OpenAI, Cohere, etc.
2. **Advanced Filtering**: Content-type based filtering, date ranges
3. **Export/Import**: Backup and restore indexed sites
4. **Web UI**: Optional web interface for management
5. **Distributed Indexing**: Support for multiple machines
6. **Custom Chunking**: User-configurable chunking strategies
7. **Incremental Updates**: Smart re-indexing of changed content
8. **Authentication**: Support for authenticated documentation sites