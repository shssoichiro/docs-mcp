-- Initial database schema for docs-mcp
-- This creates all the core tables as specified in SPEC.md

-- Sites table: stores information about indexed documentation sites
CREATE TABLE IF NOT EXISTS sites (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    base_url TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    indexed_date DATETIME,
    status TEXT NOT NULL CHECK (status IN ('pending', 'indexing', 'completed', 'failed')),
    progress_percent INTEGER NOT NULL DEFAULT 0 CHECK (progress_percent >= 0 AND progress_percent <= 100),
    total_pages INTEGER NOT NULL DEFAULT 0 CHECK (total_pages >= 0),
    indexed_pages INTEGER NOT NULL DEFAULT 0 CHECK (indexed_pages >= 0),
    error_message TEXT,
    created_date DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_heartbeat DATETIME,
    UNIQUE(name, version)
);

-- Crawl queue table: manages URLs to be crawled for each site
CREATE TABLE IF NOT EXISTS crawl_queue (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL,
    url TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    error_message TEXT,
    created_date DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (site_id) REFERENCES sites (id) ON DELETE CASCADE,
    UNIQUE(site_id, url)
);

-- Indexed chunks table: stores processed content chunks with metadata
CREATE TABLE IF NOT EXISTS indexed_chunks (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    site_id INTEGER NOT NULL,
    url TEXT NOT NULL,
    page_title TEXT,
    heading_path TEXT,
    chunk_content TEXT NOT NULL,
    chunk_index INTEGER NOT NULL CHECK (chunk_index >= 0),
    vector_id TEXT NOT NULL UNIQUE,
    indexed_date DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (site_id) REFERENCES sites (id) ON DELETE CASCADE
);

-- Indexer heartbeat table: tracks background process status
CREATE TABLE IF NOT EXISTS indexer_heartbeat (
    id INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    last_heartbeat DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    process_id TEXT,
    status TEXT NOT NULL DEFAULT 'idle' CHECK (status IN ('idle', 'indexing', 'failed'))
);

-- Insert initial heartbeat row
INSERT OR IGNORE INTO indexer_heartbeat (id, last_heartbeat, status) VALUES (1, 0, 'idle');

-- Indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_sites_status ON sites(status);
CREATE INDEX IF NOT EXISTS idx_sites_name_version ON sites(name, version);
CREATE INDEX IF NOT EXISTS idx_crawl_queue_site_status ON crawl_queue(site_id, status);
CREATE INDEX IF NOT EXISTS idx_crawl_queue_status ON crawl_queue(status);
CREATE INDEX IF NOT EXISTS idx_indexed_chunks_site_id ON indexed_chunks(site_id);
CREATE INDEX IF NOT EXISTS idx_indexed_chunks_vector_id ON indexed_chunks(vector_id);
CREATE INDEX IF NOT EXISTS idx_indexed_chunks_url ON indexed_chunks(url);