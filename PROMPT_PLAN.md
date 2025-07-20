# Documentation MCP Server - Implementation Plan

## Overview

This document provides a detailed, step-by-step implementation plan for building the documentation MCP server. Each step is designed to be small, testable, and build incrementally on previous work. The plan prioritizes early testing, incremental progress, and avoiding big complexity jumps.

## Phase 1: Foundation & Core Infrastructure

### Step 1.1: Project Setup and Basic CLI Framework

**Objective**: Establish basic project structure with CLI framework and error handling.

```
Set up the initial Rust project with proper dependencies and a basic CLI structure using Clap. Create the foundational error handling and logging infrastructure.

Requirements:
- Update Cargo.toml with essential dependencies (clap, anyhow, thiserror, tracing)
- Create basic CLI structure with subcommands (config, add, list, delete, serve)
- Implement structured error types using thiserror
- Set up tracing/logging infrastructure
- Create basic project module structure (lib.rs with common types)
- Add unit tests for CLI command parsing
- Ensure all commands compile but show "not implemented" messages

Deliverables:
- Updated Cargo.toml with core dependencies
- main.rs with complete CLI structure using Clap derive macros
- lib.rs with common error types and result types
- Basic module structure (empty mod.rs files for future modules)
- Unit tests verifying CLI command parsing works correctly
- All commands should compile and run but show helpful "not implemented" messages

Testing:
- Test that all CLI commands parse correctly
- Test help messages are displayed properly
- Test invalid command combinations show appropriate errors
- Verify logging infrastructure works
```

### Step 1.2: Configuration Management System

**Objective**: Implement TOML-based configuration with cross-platform file handling.

```
Create a robust configuration management system that handles TOML files across different operating systems with proper defaults and validation.

Requirements:
- Create config module with settings.rs
- Implement Config struct matching SPEC.md format (Ollama host, port, model, batch_size)
- Add cross-platform configuration directory discovery using dirs crate
- Implement configuration file reading/writing with proper error handling
- Add configuration validation (URL format, port ranges, etc.)
- Implement "docs-mcp config" interactive command
- Create unit tests for all configuration operations
- Handle missing config files gracefully

Deliverables:
- src/config/mod.rs and src/config/settings.rs
- Config struct with serde derive macros for TOML parsing
- Cross-platform config directory handling (~/.docs-mcp/ or %APPDATA%/docs-mcp/)
- Interactive config command that prompts for values with defaults
- Validation functions for all config values
- Unit tests covering config reading, writing, validation, and error cases
- Integration test verifying config persistence across platforms

Testing:
- Test config file creation in correct directories on different platforms
- Test TOML parsing with valid and invalid configurations
- Test interactive config prompts with various user inputs
- Test validation functions with edge cases
- Test missing/corrupted config file handling
```

### Step 1.3: SQLite Database Foundation

**Objective**: Establish SQLite database with schema migration and basic operations.

```
Implement the SQLite database layer with proper schema management, migrations, and basic CRUD operations for the core tables.

Requirements:
- Add sqlx dependency with SQLite features
- Create database/sqlite module structure
- Implement all three tables from SPEC.md (sites, crawl_queue, indexed_chunks)
- Create database connection management with connection pooling
- Implement schema migration system
- Create basic model structs with proper typing
- Add database initialization and migration functions
- Create comprehensive unit tests for all database operations
- Implement proper transaction handling

Deliverables:
- src/database/sqlite/mod.rs with connection management
- src/database/sqlite/models.rs with all data structures
- src/database/sqlite/queries.rs with CRUD operations
- SQL migration files for schema creation
- Database initialization that creates ~/.docs-mcp/metadata.db
- Unit tests for all database operations including edge cases
- Integration tests verifying schema migrations work correctly

Testing:
- Test database creation and connection
- Test all CRUD operations for each table
- Test foreign key constraints
- Test transaction rollback scenarios
- Test concurrent access patterns
- Test schema migration from empty database
- Test database file permissions and error handling
```

## Phase 2: Content Processing Foundation

### Step 2.1: URL Validation and Basic HTTP Client

**Objective**: Implement URL validation and basic HTTP operations for web crawling.

```
Create a foundation for web crawling with proper URL validation, robots.txt handling, and basic HTTP client functionality.

Requirements:
- Create crawler module structure
- Implement comprehensive URL validation and normalization
- Create HTTP client with appropriate headers and timeout handling
- Implement robots.txt fetching and parsing
- Add URL filtering logic based on base URL rules from SPEC.md
- Create rate limiting mechanism (250ms between requests)
- Implement retry logic for network failures
- Add comprehensive unit tests for all URL operations
- Mock HTTP responses for testing

Deliverables:
- src/crawler/mod.rs with HTTP client setup
- src/crawler/robots.rs with robots.txt parsing
- URL validation functions with comprehensive edge case handling
- HTTP client with proper User-Agent, timeouts, and retry logic
- Rate limiting implementation using tokio time utilities
- Unit tests covering URL validation, robots.txt parsing, and HTTP operations
- Mock server setup for testing HTTP operations

Testing:
- Test URL validation with various formats and edge cases
- Test robots.txt parsing with different robot file formats
- Test rate limiting accuracy
- Test retry logic with simulated network failures
- Test URL filtering rules with complex base URLs
- Test timeout handling and error propagation
```

### Step 2.2: HTML Content Extraction and Basic Chunking

**Objective**: Implement HTML parsing and basic content extraction without JavaScript rendering.

```
Build the content extraction pipeline focusing on HTML parsing, text extraction, and basic chunking strategies.

Requirements:
- Add scraper dependency for HTML parsing
- Create content extraction functions for HTML documents
- Implement heading hierarchy detection and preservation
- Create basic chunking algorithm targeting 500-800 tokens
- Implement text cleaning and normalization
- Add support for preserving code blocks during chunking
- Create content structure for page titles and heading paths
- Implement comprehensive unit tests with various HTML structures
- Handle malformed HTML gracefully

Deliverables:
- src/crawler/extractor.rs with HTML parsing functions
- src/embeddings/chunking.rs with chunking algorithms
- Content extraction that preserves heading hierarchy
- Chunking algorithm that respects token limits and semantic boundaries
- Text normalization functions
- Unit tests with various HTML document structures
- Test data including complex documentation HTML examples

Testing:
- Test HTML parsing with various documentation site structures
- Test chunking with different content sizes and heading levels
- Test preservation of code blocks and special formatting
- Test heading hierarchy extraction accuracy
- Test token counting accuracy for chunk sizing
- Test handling of malformed or incomplete HTML
- Test text normalization with various Unicode content
```

### Step 2.3: Basic Site Crawling Without JavaScript

**Objective**: Implement basic site crawling for static HTML content.

```
Create a working crawler that can crawl static HTML sites, respecting robots.txt and implementing the URL filtering rules.

Requirements:
- Integrate URL validation, robots.txt checking, and content extraction
- Implement breadth-first crawling algorithm
- Add URL deduplication and visited URL tracking
- Implement crawl queue management using SQLite
- Add progress tracking and status updates
- Create error handling for various HTTP response codes
- Implement the "docs-mcp add" command for adding sites
- Add comprehensive integration tests with mock sites
- Ensure proper cleanup and resource management

Deliverables:
- Complete crawler implementation in src/crawler/mod.rs
- Integration of all crawler components
- Working "docs-mcp add" command that crawls static sites
- SQLite integration for storing crawl progress and results
- Progress tracking with percentage completion
- Integration tests using mock HTTP servers
- Error handling for various failure scenarios

Testing:
- Test complete crawling workflow with mock documentation sites
- Test URL filtering with complex site structures
- Test robots.txt compliance
- Test crawl queue management and persistence
- Test progress tracking accuracy
- Test error handling and recovery scenarios
- Test resource cleanup after crawling completion
```

## Phase 3: Embedding Integration

### Step 3.1: Ollama Client Implementation

**Objective**: Create a robust Ollama API client for generating embeddings.

```
Implement the Ollama API client with proper error handling, batch processing, and connection management.

Requirements:
- Create embeddings module with Ollama client
- Implement embedding generation API calls
- Add model availability checking and validation
- Implement batch processing for multiple text chunks
- Add connection testing and health checking
- Create comprehensive error handling for API failures
- Implement retry logic for transient failures
- Add unit tests with mock Ollama server
- Handle rate limiting and API quotas

Deliverables:
- src/embeddings/ollama.rs with complete API client
- Batch processing functionality for chunk embeddings
- Health checking and model validation functions
- Retry logic with exponential backoff for transient failures
- Unit tests with mock Ollama responses
- Integration tests requiring local Ollama instance
- Error handling for all Ollama API error scenarios

Testing:
- Test embedding generation with various text inputs
- Test batch processing with different batch sizes
- Test connection handling and timeout scenarios
- Test retry logic with simulated API failures
- Test model validation and availability checking
- Test error handling for invalid API responses
- Test rate limiting compliance
```

### Step 3.2: LanceDB Integration

**Objective**: Implement vector storage and search using LanceDB.

```
Create the vector database layer with LanceDB for storing and searching embeddings.

Requirements:
- Add lancedb dependency and create vector store module
- Implement vector database initialization in ~/.docs-mcp/embeddings/
- Create embedding storage functions with metadata
- Implement vector similarity search
- Add batch insertion for multiple embeddings
- Create proper data structures matching SPEC.md schema
- Implement database cleanup and maintenance functions
- Add comprehensive unit tests and integration tests
- Handle database corruption and recovery scenarios

Deliverables:
- src/database/lancedb/mod.rs and vector_store.rs
- EmbeddingRecord and ChunkMetadata structs
- Vector database initialization and management
- Embedding insertion and search functionality
- Batch processing for efficient storage
- Unit tests for all vector operations
- Integration tests with real embedding data

Testing:
- Test vector database creation and initialization
- Test embedding storage and retrieval
- Test similarity search with various queries
- Test batch insertion performance and accuracy
- Test metadata storage and filtering
- Test database maintenance and cleanup operations
- Test error handling for database corruption scenarios
```

### Step 3.3: End-to-End Embedding Pipeline

**Objective**: Integrate crawling and embedding generation into a complete indexing pipeline.

```
Connect the crawling and embedding systems to create a complete pipeline from URL to searchable embeddings.

Requirements:
- Integrate content extraction, chunking, and embedding generation
- Implement end-to-end indexing workflow
- Add proper error handling and progress tracking
- Create database consistency checks and validation
- Implement the "docs-mcp list" and "docs-mcp status" commands
- Add cleanup for failed indexing operations
- Create comprehensive integration tests
- Ensure data consistency between SQLite and LanceDB
- Implement proper resource management and cleanup

Deliverables:
- Complete integration between crawler and embedding systems
- Working "docs-mcp list" and "docs-mcp status" commands
- End-to-end indexing pipeline with error recovery
- Data consistency validation functions
- Integration tests covering complete indexing workflow
- Progress tracking and status reporting
- Cleanup procedures for partial or failed indexing

Testing:
- Test complete workflow from URL to searchable embeddings
- Test data consistency between SQLite metadata and LanceDB vectors
- Test error recovery and partial failure scenarios
- Test progress tracking accuracy throughout pipeline
- Test resource usage and memory management
- Test cleanup procedures for various failure modes
- Test concurrent access patterns and data integrity
```

## Phase 4: Background Processing

### Step 4.1: Process Coordination and File Locking

**Objective**: Implement background process coordination with file locking and heartbeat mechanism.

```
Create the background process coordination system using file locking and heartbeat monitoring.

Requirements:
- Implement file locking mechanism using ~/.docs-mcp/.indexer.lock
- Create heartbeat system with SQLite timestamp updates
- Add process discovery and stale process detection
- Implement background indexer process management
- Create proper signal handling and graceful shutdown
- Add process status monitoring and reporting
- Implement comprehensive tests for process coordination
- Handle edge cases like system crashes and forced termination

Deliverables:
- src/indexer/process.rs with process coordination logic
- File locking implementation with exclusive locks
- Heartbeat mechanism with 30-second updates
- Stale process detection (>60 seconds without heartbeat)
- Background process spawning and management
- Signal handling for graceful shutdown
- Unit tests for process coordination scenarios

Testing:
- Test file locking with multiple concurrent processes
- Test heartbeat mechanism and stale detection
- Test process cleanup and lock file removal
- Test signal handling and graceful shutdown
- Test edge cases like system crashes and lock file corruption
- Test process status monitoring accuracy
- Test concurrent access to shared resources
```

### Step 4.2: Indexing Queue Management

**Objective**: Implement robust queue management for background indexing operations.

```
Create a comprehensive queue management system for handling indexing tasks in the background.

Requirements:
- Implement queue processing using crawl_queue table
- Add priority-based queue ordering
- Create resume capability for interrupted indexing
- Implement retry logic with exponential backoff
- Add queue status monitoring and reporting
- Create queue cleanup and maintenance functions
- Implement comprehensive error handling and recovery
- Add queue performance monitoring and optimization

Deliverables:
- src/indexer/queue.rs with complete queue management
- Priority-based queue processing
- Resume capability for interrupted operations
- Retry logic with configurable parameters
- Queue monitoring and status reporting
- Maintenance functions for queue cleanup
- Unit tests covering all queue operations

Testing:
- Test queue processing with various priority scenarios
- Test resume capability after interruption
- Test retry logic with different failure types
- Test queue status monitoring accuracy
- Test queue cleanup and maintenance operations
- Test concurrent queue access patterns
- Test queue performance under load
```

### Step 4.3: Complete Background Indexer

**Objective**: Integrate all background processing components into a working indexer.

```
Combine process coordination, queue management, and indexing pipeline into a complete background indexing system.

Requirements:
- Integrate process coordination with queue management
- Implement auto-start and auto-termination logic
- Add comprehensive progress tracking and reporting
- Create background indexer CLI integration
- Implement status monitoring and health checks
- Add performance optimization and resource management
- Create complete integration tests
- Ensure proper cleanup and resource management

Deliverables:
- Complete background indexer integration
- Auto-start/termination logic
- Progress tracking and status reporting
- CLI integration for background operations
- Health monitoring and performance metrics
- Integration tests for complete background system
- Resource management and cleanup procedures

Testing:
- Test complete background indexing workflow
- Test auto-start and termination logic
- Test progress tracking accuracy and reporting
- Test health monitoring and error detection
- Test resource management under various load conditions
- Test integration with CLI commands
- Test cleanup procedures for various scenarios
```

## Phase 5: MCP Server Implementation

### Step 5.1: Basic MCP Protocol Implementation

**Objective**: Implement the core MCP server protocol and communication.

```
Create the foundation for the MCP server with proper protocol implementation and communication handling.

Requirements:
- Research and implement basic MCP protocol structure
- Create MCP server framework with proper message handling
- Implement tool registration and discovery
- Add proper JSON schema validation for MCP messages
- Create connection management and client handling
- Implement basic error handling and protocol compliance
- Add comprehensive unit tests for protocol implementation
- Ensure compatibility with MCP client specifications

Deliverables:
- src/mcp/mod.rs and server.rs with MCP protocol implementation
- Message handling and routing infrastructure
- Tool registration and discovery system
- JSON schema validation for MCP messages
- Connection management for MCP clients
- Unit tests for protocol compliance
- Basic error handling and response formatting

Testing:
- Test MCP protocol message parsing and validation
- Test tool registration and discovery mechanisms
- Test client connection handling and management
- Test error handling and response formatting
- Test protocol compliance with MCP specifications
- Test message routing and handler dispatch
- Test concurrent client connection handling
```

### Step 5.2: Search Tool Implementation

**Objective**: Implement the search_docs MCP tool with vector search capabilities.

```
Create the search_docs tool that provides semantic search functionality through the MCP interface.

Requirements:
- Implement search_docs tool matching SPEC.md JSON schema
- Integrate with LanceDB vector search functionality
- Add query processing and embedding generation for search terms
- Implement result ranking and relevance scoring
- Create response formatting matching specification
- Add filtering capabilities (site_id, sites_filter)
- Implement result limiting and pagination
- Add comprehensive tests for search functionality

Deliverables:
- src/mcp/tools.rs with search_docs implementation
- Integration with LanceDB vector search
- Query processing and embedding generation
- Result ranking and relevance scoring
- Response formatting matching SPEC.md
- Filtering and limiting functionality
- Unit and integration tests for search operations

Testing:
- Test search functionality with various query types
- Test result ranking and relevance scoring accuracy
- Test filtering capabilities with different parameters
- Test result limiting and response formatting
- Test search performance with large datasets
- Test error handling for invalid search parameters
- Test integration with embedding generation pipeline
```

### Step 5.3: Complete MCP Server with All Tools

**Objective**: Complete the MCP server implementation with all required tools and functionality.

```
Finish the MCP server by implementing all remaining tools and adding production-ready features.

Requirements:
- Implement list_sites tool matching SPEC.md specification
- Add server startup and configuration management
- Implement the "docs-mcp serve" CLI command
- Add proper logging and monitoring for MCP operations
- Create health checking and status reporting
- Implement concurrent client handling
- Add comprehensive integration tests
- Ensure production readiness with proper error handling

Deliverables:
- Complete MCP server with all tools (search_docs, list_sites)
- "docs-mcp serve" CLI command implementation
- Server configuration and startup management
- Logging and monitoring for MCP operations
- Health checking and status reporting
- Integration tests for complete MCP functionality
- Production-ready error handling and resilience

Testing:
- Test all MCP tools with various parameter combinations
- Test server startup and configuration handling
- Test concurrent client connections and operations
- Test logging and monitoring functionality
- Test health checking and status reporting
- Test error handling and recovery scenarios
- Test integration with background indexing operations
```

## Phase 6: CLI Polish and Integration

### Step 6.1: Complete CLI Commands Implementation

**Objective**: Implement all remaining CLI commands with proper integration.

```
Complete all CLI commands with proper integration to the background systems and user-friendly interfaces.

Requirements:
- Implement "docs-mcp delete" command with proper cleanup
- Implement "docs-mcp update" command for re-indexing
- Add comprehensive status reporting for all operations
- Create user-friendly progress displays and formatting
- Implement proper error messages and help text
- Add command validation and parameter checking
- Create comprehensive tests for all CLI operations
- Ensure consistent behavior across all commands

Deliverables:
- Complete implementation of all CLI commands
- User-friendly progress displays and status reporting
- Comprehensive error handling and help messages
- Command validation and parameter checking
- Integration tests for all CLI operations
- Consistent behavior and formatting across commands

Testing:
- Test all CLI commands with various parameter combinations
- Test user interface elements and progress displays
- Test error handling and help message display
- Test command validation and parameter checking
- Test integration between CLI and background systems
- Test edge cases and error scenarios for all commands
```

### Step 6.2: JavaScript Rendering Support

**Objective**: Add headless browser support for JavaScript-rendered content.

```
Enhance the crawler to support JavaScript-rendered content using headless Chrome.

Requirements:
- Add headless_chrome dependency and browser management
- Implement JavaScript content rendering in crawler
- Create browser pool management for performance
- Add timeout and resource management for browser operations
- Integrate with existing content extraction pipeline
- Implement proper error handling for browser failures
- Add configuration options for browser behavior
- Create tests with JavaScript-heavy documentation sites

Deliverables:
- src/crawler/browser.rs with headless browser management
- JavaScript content rendering integration
- Browser pool management and resource optimization
- Timeout and error handling for browser operations
- Integration with content extraction pipeline
- Configuration options for browser behavior
- Tests with JavaScript-rendered content

Testing:
- Test JavaScript content rendering with various sites
- Test browser pool management and resource usage
- Test timeout handling and error recovery
- Test integration with existing extraction pipeline
- Test performance with JavaScript-heavy sites
- Test browser cleanup and resource management
```

### Step 6.3: Final Integration and Production Readiness

**Objective**: Complete final integration testing and prepare for production use.

```
Perform comprehensive integration testing and add production readiness features.

Requirements:
- Complete end-to-end integration testing
- Add comprehensive error handling and logging
- Implement performance optimizations
- Create user documentation and setup guides
- Add monitoring and observability features
- Implement backup and recovery procedures
- Create deployment and distribution packaging
- Perform security review and hardening

Deliverables:
- Complete end-to-end integration testing
- Production-ready error handling and logging
- Performance optimizations and monitoring
- User documentation and setup guides
- Backup and recovery procedures
- Security review and hardening
- Distribution packaging and deployment guides

Testing:
- Complete end-to-end testing with real documentation sites
- Performance testing under various load conditions
- Security testing and vulnerability assessment
- User acceptance testing with documentation workflows
- Backup and recovery testing
- Cross-platform compatibility testing
- Production deployment testing
```

## Testing Strategy Throughout Development

### Unit Testing Approach

- Each step includes comprehensive unit tests
- Mock external dependencies (HTTP, Ollama, file system)
- Test edge cases and error conditions
- Maintain >90% code coverage

### Integration Testing Approach

- Test component interactions at each phase
- Use test databases and mock services
- Test real workflows with sample data
- Validate data consistency across systems

### Performance Testing

- Benchmark each major component
- Test memory usage and resource management
- Validate rate limiting and throttling
- Measure search performance and accuracy

### Error Handling Testing

- Test all error paths and recovery scenarios
- Validate graceful degradation
- Test cleanup procedures
- Ensure proper error reporting

## Success Criteria for Each Step

Each step is considered complete when:

1. All functionality works as specified
2. Unit tests pass with good coverage
3. Integration tests demonstrate proper component interaction
4. Error handling is comprehensive and tested
5. Documentation is updated for implemented features
6. Performance meets acceptable benchmarks
7. Code review confirms quality and maintainability

This plan ensures steady progress with minimal risk, comprehensive testing, and early validation of critical functionality.
