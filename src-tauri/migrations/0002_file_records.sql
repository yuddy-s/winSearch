CREATE TABLE IF NOT EXISTS file_records (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  extension TEXT,
  normalized_name TEXT NOT NULL,
  normalized_path TEXT NOT NULL UNIQUE,
  parent_path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  modified_at INTEGER NOT NULL,
  content_text TEXT,
  content_indexed INTEGER NOT NULL DEFAULT 0,
  last_seen_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_records_normalized_name ON file_records(normalized_name);
CREATE INDEX IF NOT EXISTS idx_file_records_last_seen_at ON file_records(last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_file_records_modified_at ON file_records(modified_at DESC);
