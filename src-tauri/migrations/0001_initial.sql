CREATE TABLE IF NOT EXISTS app_records (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  normalized_name TEXT NOT NULL,
  aliases_json TEXT NOT NULL DEFAULT '[]',
  source TEXT NOT NULL,
  launch_target TEXT NOT NULL,
  icon_key TEXT,
  merge_key TEXT NOT NULL UNIQUE,
  last_seen_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_app_records_normalized_name ON app_records(normalized_name);
CREATE INDEX IF NOT EXISTS idx_app_records_last_seen_at ON app_records(last_seen_at DESC);

CREATE TABLE IF NOT EXISTS app_record_sources (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  app_id TEXT NOT NULL,
  source TEXT NOT NULL,
  source_identifier TEXT NOT NULL,
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  FOREIGN KEY(app_id) REFERENCES app_records(id) ON DELETE CASCADE,
  UNIQUE(source, source_identifier)
);

CREATE TABLE IF NOT EXISTS source_versions (
  source TEXT PRIMARY KEY,
  collector_version TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
