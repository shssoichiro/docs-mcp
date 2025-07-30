# docs-mcp

A documentation indexing and search system that provides an MCP (Model Context Protocol) server for AI assistants to search locally-indexed developer documentation. The system crawls documentation websites, indexes their content using embeddings, and provides semantic search capabilities through an MCP interface.

## Features

- **Intelligent Web Crawling**: Comprehensive crawling with JavaScript rendering support for modern documentation sites
- **Semantic Search**: Vector-based search using Ollama embeddings with relevance scoring
- **MCP Integration**: Full Model Context Protocol server for AI assistant integration
- **Local Storage**: Uses SQLite for metadata and LanceDB for vector embeddings
- **Background Processing**: Automatic indexing with process coordination and queue management
- **Production Ready**: Robust error handling, timeout management, and resource cleanup

## Installation

### Prerequisites

1. **Rust**: Install Rust 1.86+ from [rustup.rs](https://rustup.rs/)
2. **Ollama**: Install Ollama from [ollama.ai](https://ollama.ai/) and pull your preferred embedding model. We recommend [`nomic-embed-text`](https://ollama.com/library/nomic-embed-text) as it is high-quality while being memory-efficient.

```bash
ollama pull nomic-embed-text:latest
```

### Build from Source

```bash
git clone https://github.com/shssoichiro/docs-mcp.git
cd docs-mcp
cargo build --release
```

The binary will be available at `target/release/docs-mcp`.

## Quick Start

### 1. Initial Configuration

Set up Ollama connection and settings:

```bash
docs-mcp config
```

This will interactively prompt for:

- Ollama host (default: `localhost`)
- Ollama port (default: `11434`)
- Embedding model (default: `nomic-embed-text`)
- Batch size for processing (default: `64`)

### 2. Add Documentation Sites

Index your first documentation site:

```bash
docs-mcp add https://doc.rust-lang.org/ --name "Rust Documentation"
```

The system will:

- Crawl the site respecting robots.txt
- Extract and chunk content semantically
- Generate embeddings using Ollama
- Store vectors in LanceDB for search

### 3. Check Status

Monitor indexing progress:

```bash
docs-mcp status
```

### 4. Start MCP Server

Launch the MCP server for AI assistant integration:

```bash
docs-mcp serve
```

The server uses stdio transport and provides tools:

- `search_docs`: Semantic search across indexed documentation
- `list_sites`: List available documentation sites

TODO: Show how to integrate with Claude Code and other popular coding agents.

## Usage

### Command Reference

#### Configuration

```bash
# Interactive configuration setup
docs-mcp config

# Show current configuration
docs-mcp config --show
```

#### Site Management

```bash
# Add a new documentation site
docs-mcp add <url> [--name <name>]

# List all indexed sites
docs-mcp list

# Update/re-index a site
docs-mcp update <site_id_or_name>

# Delete a site
docs-mcp delete <site_id_or_name>
```

#### System Operations

```bash
# Show detailed pipeline status
docs-mcp status

# Start MCP server (stdio transport)
docs-mcp serve
```

### MCP Integration

The MCP server provides two main tools for AI assistants:

#### search_docs

Search across indexed documentation using semantic similarity:

```json
{
  "name": "search_docs",
  "arguments": {
    "query": "How to handle async errors in Rust",
    "limit": 10,
    "site_id": 1,
    "sites_filter": "rust.*"
  }
}
```

Parameters:

- `query` (required): Natural language search query
- `limit` (optional): Maximum number of results (default: 10)
- `site_id` (optional): Search specific site by ID
- `sites_filter` (optional): Regex pattern to filter sites

#### list_sites

List all indexed documentation sites:

```json
{
  "name": "list_sites",
  "arguments": {}
}
```

Returns sites with metadata including name, version, URL, status, and page count.

## Advanced Configuration

Configuration is stored in TOML format at:

- Unix: `~/.docs-mcp/config.toml`
- Windows: `%APPDATA%/docs-mcp/config.toml`

### Example Configuration

```toml
[ollama]
host = "localhost"
port = 11434
model = "nomic-embed-text"
batch_size = 64

[browser]
enabled = true
pool_size = 2
tabs_per_browser = 4
timeout_seconds = 30
headless = true
window_width = 1920
window_height = 1080
```

### Browser Configuration

For sites requiring JavaScript rendering:

- `enabled`: Enable headless Chrome rendering
- `pool_size`: Number of browser instances (1-10)
- `tabs_per_browser`: Tabs per browser (1-10)
- `timeout_seconds`: Page load timeout (1-300)
- `headless`: Run browsers in headless mode
- `window_width/height`: Viewport dimensions

## Development

### Building

```bash
cargo build           # Debug build
cargo build --release # Optimized release
```

### Testing

```bash
cargo test           # Run all tests
cargo test config::  # Run config module tests
```

### Code Quality

```bash
cargo check          # Quick compile check
cargo clippy         # Linting
cargo fmt            # Format code
just precommit       # Run all pre-commit checks
```

## Troubleshooting

### Common Issues

#### Ollama Connection Failed

```bash
# Check Ollama is running
ollama list

# Verify model is available
ollama pull nomic-embed-text

# Test configuration
docs-mcp config --show
```

#### JavaScript Rendering Issues

```bash
# Check Chrome/Chromium installation
which google-chrome || which chromium

# Disable browser rendering if needed
# Edit ~/.docs-mcp/config.toml:
[browser]
enabled = false
```

#### Database Corruption

```bash
# Check database status
docs-mcp status

# The system automatically detects and recovers from corruption
# Or manually delete and re-index:
rm -rf ~/.docs-mcp/
docs-mcp config
```

#### Slow Indexing

```bash
# Reduce batch size in config
[ollama]
batch_size = 32

# Check system resources, and ensure GPU-enabled Ollama is installed
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run pre-commit checks (`just precommit`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

### Development Guidelines

- Follow SOLID principles and Rust best practices
- Write comprehensive tests for new functionality
- Update documentation for user-facing changes
- Run `just precommit` before committing
- Use meaningful commit messages

## Support

- **Issues**: Report bugs and feature requests via GitHub Issues
- **Discussions**: Join conversations in GitHub Discussions

## AI Disclosure

This repository was developed with the assistance of an AI coding agent. All code was reviewed by a human developer.

---

Built with ❤️ in Rust, in collaboration with Claude
