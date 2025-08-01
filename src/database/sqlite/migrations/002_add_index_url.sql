-- Add index_url column and remove UNIQUE constraint from base_url
-- This migration handles existing data safely
-- Step 1: Add the index_url column if it doesn't exist
-- We can't use DEFAULT base_url directly, so we'll add it as nullable first
ALTER TABLE sites
ADD COLUMN index_url TEXT;

-- Step 2: Update existing rows to set index_url = base_url where index_url is NULL
UPDATE sites
SET
    index_url = base_url
WHERE
    index_url IS NULL;

-- Step 3: Create a new sites table with the updated schema
CREATE TABLE
    sites_new (
        id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
        index_url TEXT UNIQUE NOT NULL, -- New column with UNIQUE constraint
        base_url TEXT NOT NULL, -- Removed UNIQUE constraint
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        indexed_date DATETIME,
        status TEXT NOT NULL CHECK (
            status IN ('pending', 'indexing', 'completed', 'failed')
        ),
        progress_percent INTEGER NOT NULL DEFAULT 0 CHECK (
            progress_percent >= 0
            AND progress_percent <= 100
        ),
        total_pages INTEGER NOT NULL DEFAULT 0 CHECK (total_pages >= 0),
        indexed_pages INTEGER NOT NULL DEFAULT 0 CHECK (indexed_pages >= 0),
        error_message TEXT,
        created_date DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        last_heartbeat DATETIME,
        UNIQUE (name, version)
    );

-- Step 4: Copy data from old table to new table
INSERT INTO
    sites_new (
        id,
        index_url,
        base_url,
        name,
        version,
        indexed_date,
        status,
        progress_percent,
        total_pages,
        indexed_pages,
        error_message,
        created_date,
        last_heartbeat
    )
SELECT
    id,
    index_url,
    base_url,
    name,
    version,
    indexed_date,
    status,
    progress_percent,
    total_pages,
    indexed_pages,
    error_message,
    created_date,
    last_heartbeat
FROM
    sites;

-- Step 5: Drop the old table and rename the new one
DROP TABLE sites;

ALTER TABLE sites_new
RENAME TO sites;

-- Step 6: Recreate the indexes
CREATE INDEX IF NOT EXISTS idx_sites_status ON sites (status);

CREATE INDEX IF NOT EXISTS idx_sites_name_version ON sites (name, version);

CREATE INDEX IF NOT EXISTS idx_sites_index_url ON sites (index_url);