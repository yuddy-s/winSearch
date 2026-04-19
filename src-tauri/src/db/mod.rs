use rusqlite::{params, types::Type, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
  collections::HashSet,
  fs,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

const DB_FILE_NAME: &str = "winsearch.db";
const CURRENT_SCHEMA_VERSION: i64 = 3;
const MIGRATION_0001_INITIAL: &str = include_str!("../../migrations/0001_initial.sql");
const MIGRATION_0002_FILE_RECORDS: &str = include_str!("../../migrations/0002_file_records.sql");
const MIGRATION_0003_FILE_RECORDS_FTS5: &str =
  include_str!("../../migrations/0003_file_records_fts5.sql");
const SQLITE_LIKE_ESCAPE_CHAR: char = '\\';

#[derive(Clone)]
pub struct AppIndexStore {
  db_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
  pub db_path: String,
  pub schema_version: i64,
  pub app_count: i64,
  pub file_count: i64,
  pub source_version_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRecord {
  pub id: String,
  pub name: String,
  pub normalized_name: String,
  pub aliases: Vec<String>,
  pub source: String,
  pub launch_target: String,
  pub icon_key: Option<String>,
  pub merge_key: String,
  pub last_seen_at: i64,
  pub created_at: i64,
  pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRecordUpsert {
  pub name: String,
  #[serde(default)]
  pub aliases: Vec<String>,
  pub source: String,
  pub source_identifier: String,
  pub launch_target: String,
  pub icon_key: Option<String>,
  pub merge_key: String,
  pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
  pub id: String,
  pub name: String,
  pub extension: Option<String>,
  pub normalized_name: String,
  pub normalized_path: String,
  pub parent_path: String,
  pub size_bytes: i64,
  pub modified_at: i64,
  pub content_indexed: bool,
  pub last_seen_at: i64,
  pub created_at: i64,
  pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecordUpsert {
  pub path: String,
  pub size_bytes: i64,
  pub modified_at: i64,
  pub content_text: Option<String>,
  pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct FileRecordSnapshot {
  pub size_bytes: i64,
  pub modified_at: i64,
}

impl AppIndexStore {
  pub fn initialize(data_dir: &Path) -> Result<Self, String> {
    fs::create_dir_all(data_dir).map_err(|error| {
      format!(
        "Failed to create index data directory '{}': {error}",
        data_dir.display()
      )
    })?;

    let db_path = data_dir.join(DB_FILE_NAME);
    let connection = Connection::open(&db_path)
      .map_err(|error| format!("Failed to open SQLite database '{}': {error}", db_path.display()))?;

    connection
      .execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
        PRAGMA trusted_schema=OFF;
      "#,
      )
      .map_err(|error| format!("Failed to apply SQLite pragmas: {error}"))?;

    run_migrations(&connection)?;

    Ok(Self { db_path })
  }

  pub fn get_status(&self) -> Result<IndexStatus, String> {
    let connection = self.open_connection()?;

    let schema_version: i64 = connection
      .query_row("PRAGMA user_version;", [], |row| row.get(0))
      .map_err(|error| format!("Failed to read schema version: {error}"))?;

    let app_count: i64 = connection
      .query_row("SELECT COUNT(*) FROM app_records;", [], |row| row.get(0))
      .map_err(|error| format!("Failed to count app records: {error}"))?;

    let file_count: i64 = connection
      .query_row("SELECT COUNT(*) FROM file_records;", [], |row| row.get(0))
      .map_err(|error| format!("Failed to count file records: {error}"))?;

    let source_version_count: i64 = connection
      .query_row("SELECT COUNT(*) FROM source_versions;", [], |row| row.get(0))
      .map_err(|error| format!("Failed to count source versions: {error}"))?;

    Ok(IndexStatus {
      db_path: self.db_path.to_string_lossy().into_owned(),
      schema_version,
      app_count,
      file_count,
      source_version_count,
    })
  }

  pub fn upsert_app_record(&self, input: AppRecordUpsert) -> Result<AppRecord, String> {
    let mut connection = self.open_connection()?;
    let transaction = connection
      .transaction()
      .map_err(|error| format!("Failed to open SQLite transaction: {error}"))?;

    let now = current_timestamp_ms();
    let normalized_name = normalize_text(&input.name);
    let last_seen_at = input.last_seen_at.unwrap_or(now);
    let aliases_json = serde_json::to_string(&input.aliases)
      .map_err(|error| format!("Failed to serialize aliases: {error}"))?;

    let existing_id: Option<String> = transaction
      .query_row(
        "SELECT id FROM app_records WHERE merge_key = ?1;",
        [input.merge_key.as_str()],
        |row| row.get(0),
      )
      .optional()
      .map_err(|error| format!("Failed to look up existing app record: {error}"))?;

    let app_id = existing_id
      .clone()
      .unwrap_or_else(|| format!("app::{}", input.merge_key));

    if existing_id.is_some() {
      transaction
        .execute(
          r#"
          UPDATE app_records
          SET
            name = ?1,
            normalized_name = ?2,
            aliases_json = ?3,
            source = ?4,
            launch_target = ?5,
            icon_key = ?6,
            last_seen_at = ?7,
            updated_at = ?8
          WHERE id = ?9;
          "#,
          params![
            input.name,
            normalized_name,
            aliases_json,
            input.source,
            input.launch_target,
            input.icon_key,
            last_seen_at,
            now,
            app_id
          ],
        )
        .map_err(|error| format!("Failed to update app record: {error}"))?;
    } else {
      transaction
        .execute(
          r#"
          INSERT INTO app_records (
            id,
            name,
            normalized_name,
            aliases_json,
            source,
            launch_target,
            icon_key,
            merge_key,
            last_seen_at,
            created_at,
            updated_at
          )
          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11);
          "#,
          params![
            app_id,
            input.name,
            normalized_name,
            aliases_json,
            input.source,
            input.launch_target,
            input.icon_key,
            input.merge_key,
            last_seen_at,
            now,
            now
          ],
        )
        .map_err(|error| format!("Failed to insert app record: {error}"))?;
    }

    transaction
      .execute(
        r#"
        INSERT INTO app_record_sources (
          app_id,
          source,
          source_identifier,
          first_seen_at,
          last_seen_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(source, source_identifier)
        DO UPDATE SET
          app_id = excluded.app_id,
          last_seen_at = excluded.last_seen_at;
        "#,
        params![
          app_id,
          input.source,
          input.source_identifier,
          now,
          last_seen_at
        ],
      )
      .map_err(|error| format!("Failed to upsert app source attribution: {error}"))?;

    transaction
      .commit()
      .map_err(|error| format!("Failed to commit app upsert transaction: {error}"))?;

    self.get_app_record_by_id(&app_id)
  }

  pub fn list_apps(&self, limit: u32) -> Result<Vec<AppRecord>, String> {
    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT
          id,
          name,
          normalized_name,
          aliases_json,
          source,
          launch_target,
          icon_key,
          merge_key,
          last_seen_at,
          created_at,
          updated_at
        FROM app_records
        ORDER BY last_seen_at DESC, name ASC
        LIMIT ?1;
        "#,
      )
      .map_err(|error| format!("Failed to prepare app listing query: {error}"))?;

    let row_iter = statement
      .query_map([i64::from(limit)], map_app_record)
      .map_err(|error| format!("Failed to query app records: {error}"))?;

    row_iter
      .collect::<Result<Vec<_>, _>>()
      .map_err(|error| format!("Failed to parse app records: {error}"))
  }

  pub fn upsert_file_record(&self, input: FileRecordUpsert) -> Result<FileRecord, String> {
    let mut connection = self.open_connection()?;
    let transaction = connection
      .transaction()
      .map_err(|error| format!("Failed to open SQLite transaction: {error}"))?;

    let path = PathBuf::from(&input.path);
    let name = path
      .file_name()
      .and_then(|value| value.to_str())
      .map(str::trim)
      .filter(|value| !value.is_empty())
      .ok_or_else(|| format!("Invalid file name for path '{}'", input.path))?
      .to_string();

    let extension = path
      .extension()
      .and_then(|value| value.to_str())
      .map(|value| value.to_ascii_lowercase());

    let parent_path = path
      .parent()
      .map(|value| value.to_string_lossy().into_owned())
      .unwrap_or_default();

    let normalized_name = normalize_text(&name);
    let normalized_path = normalize_path_string(&input.path);
    let file_id = format!("file::{normalized_path}");
    let now = current_timestamp_ms();
    let last_seen_at = input.last_seen_at.unwrap_or(now);
    let normalized_content_text = input.content_text.map(|content| content.to_lowercase());
    let content_indexed = normalized_content_text
      .as_ref()
      .map(|content| !content.trim().is_empty())
      .unwrap_or(false);
    let content_indexed_as_int = if content_indexed { 1 } else { 0 };

    transaction
      .execute(
        r#"
        INSERT INTO file_records (
          id,
          name,
          extension,
          normalized_name,
          normalized_path,
          parent_path,
          size_bytes,
          modified_at,
          content_text,
          content_indexed,
          last_seen_at,
          created_at,
          updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(normalized_path)
        DO UPDATE SET
          name = excluded.name,
          extension = excluded.extension,
          normalized_name = excluded.normalized_name,
          parent_path = excluded.parent_path,
          size_bytes = excluded.size_bytes,
          modified_at = excluded.modified_at,
          content_text = excluded.content_text,
          content_indexed = excluded.content_indexed,
          last_seen_at = excluded.last_seen_at,
          updated_at = excluded.updated_at;
        "#,
        params![
          file_id,
          name,
          extension,
          normalized_name,
          normalized_path,
          parent_path,
          input.size_bytes,
          input.modified_at,
          normalized_content_text,
          content_indexed_as_int,
          last_seen_at,
          now,
          now
        ],
      )
      .map_err(|error| format!("Failed to upsert file record '{}': {error}", input.path))?;

    transaction
      .commit()
      .map_err(|error| format!("Failed to commit file upsert transaction: {error}"))?;

    self.get_file_record_by_path(&input.path)
  }

  pub fn list_files(&self, limit: u32) -> Result<Vec<FileRecord>, String> {
    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT
          id,
          name,
          extension,
          normalized_name,
          normalized_path,
          parent_path,
          size_bytes,
          modified_at,
          content_indexed,
          last_seen_at,
          created_at,
          updated_at
        FROM file_records
        ORDER BY last_seen_at DESC, name ASC
        LIMIT ?1;
        "#,
      )
      .map_err(|error| format!("Failed to prepare file listing query: {error}"))?;

    let row_iter = statement
      .query_map([i64::from(limit)], map_file_record)
      .map_err(|error| format!("Failed to query file records: {error}"))?;

    row_iter
      .collect::<Result<Vec<_>, _>>()
      .map_err(|error| format!("Failed to parse file records: {error}"))
  }

  pub fn get_file_record_snapshot(&self, path: &str) -> Result<Option<FileRecordSnapshot>, String> {
    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT size_bytes, modified_at
        FROM file_records
        WHERE normalized_path = ?1;
        "#,
      )
      .map_err(|error| format!("Failed to prepare file snapshot lookup query: {error}"))?;

    statement
      .query_row([normalize_path_string(path)], |row| {
        Ok(FileRecordSnapshot {
          size_bytes: row.get(0)?,
          modified_at: row.get(1)?,
        })
      })
      .optional()
      .map_err(|error| format!("Failed to load file snapshot '{}': {error}", path))
  }

  pub fn get_file_record_by_id(&self, file_id: &str) -> Result<Option<FileRecord>, String> {
    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT
          id,
          name,
          extension,
          normalized_name,
          normalized_path,
          parent_path,
          size_bytes,
          modified_at,
          content_indexed,
          last_seen_at,
          created_at,
          updated_at
        FROM file_records
        WHERE id = ?1;
        "#,
      )
      .map_err(|error| format!("Failed to prepare file-id lookup query: {error}"))?;

    statement
      .query_row([file_id], map_file_record)
      .optional()
      .map_err(|error| format!("Failed to load file record by id '{}': {error}", file_id))
  }

  pub fn search_files(&self, query: &str, limit: u32) -> Result<Vec<FileRecord>, String> {
    let normalized_query = normalize_text(query);
    if normalized_query.is_empty() {
      return Ok(Vec::new());
    }

    let escaped_query = escape_like_pattern(&normalized_query);
    let like_pattern = format!("%{escaped_query}%");
    let prefix_pattern = format!("{escaped_query}%");
    let connection = self.open_connection()?;
    let limit_i64 = i64::from(limit);

    if let Some(fts_query) = build_fts_prefix_query(&normalized_query) {
      let content_limit_i64 = i64::from(limit.saturating_mul(5).max(50));
      let mut statement = connection
        .prepare(
          r#"
          WITH
            name_matches AS (
              SELECT
                id AS file_id,
                CASE
                  WHEN normalized_name = ?1 THEN 0
                  WHEN normalized_name LIKE ?2 ESCAPE '\' THEN 1
                  WHEN normalized_name LIKE ?3 ESCAPE '\' THEN 2
                  ELSE 3
                END AS name_rank,
                0 AS content_hit,
                0.0 AS content_rank
              FROM file_records
              WHERE normalized_name LIKE ?3 ESCAPE '\'
            ),
            content_matches AS (
              SELECT
                file_id,
                4 AS name_rank,
                1 AS content_hit,
                bm25(file_records_fts) AS content_rank
              FROM file_records_fts
              WHERE file_records_fts MATCH ?4
              LIMIT ?5
            ),
            combined AS (
              SELECT * FROM name_matches
              UNION ALL
              SELECT * FROM content_matches
            ),
            dedup AS (
              SELECT
                file_id,
                MIN(name_rank) AS best_name_rank,
                MAX(content_hit) AS content_hit,
                MIN(content_rank) AS best_content_rank
              FROM combined
              GROUP BY file_id
            )
          SELECT
            fr.id,
            fr.name,
            fr.extension,
            fr.normalized_name,
            fr.normalized_path,
            fr.parent_path,
            fr.size_bytes,
            fr.modified_at,
            fr.content_indexed,
            fr.last_seen_at,
            fr.created_at,
            fr.updated_at
          FROM dedup d
          JOIN file_records fr ON fr.id = d.file_id
          ORDER BY
            d.best_name_rank ASC,
            d.content_hit DESC,
            CASE
              WHEN d.content_hit = 1 THEN d.best_content_rank
              ELSE 0
            END ASC,
            fr.last_seen_at DESC,
            fr.name ASC
          LIMIT ?6;
          "#,
        )
        .map_err(|error| format!("Failed to prepare FTS-backed file search query: {error}"))?;

      let row_iter = statement
        .query_map(
          params![
            normalized_query,
            prefix_pattern,
            like_pattern,
            fts_query,
            content_limit_i64,
            limit_i64
          ],
          map_file_record,
        )
        .map_err(|error| format!("Failed to run FTS-backed file search query: {error}"))?;

      return row_iter
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to parse FTS-backed file search results: {error}"));
    }

    let mut statement = connection
      .prepare(
        r#"
        SELECT
          id,
          name,
          extension,
          normalized_name,
          normalized_path,
          parent_path,
          size_bytes,
          modified_at,
          content_indexed,
          last_seen_at,
          created_at,
          updated_at
        FROM file_records
        WHERE normalized_name LIKE ?2 ESCAPE '\'
        ORDER BY
          CASE
            WHEN normalized_name = ?1 THEN 0
            WHEN normalized_name LIKE ?3 ESCAPE '\' THEN 1
            ELSE 2
          END,
          last_seen_at DESC,
          name ASC
        LIMIT ?4;
        "#,
      )
      .map_err(|error| format!("Failed to prepare name-only file search query: {error}"))?;

    let row_iter = statement
      .query_map(params![normalized_query, like_pattern, prefix_pattern, limit_i64], map_file_record)
      .map_err(|error| format!("Failed to run name-only file search query: {error}"))?;

    row_iter
      .collect::<Result<Vec<_>, _>>()
      .map_err(|error| format!("Failed to parse name-only file search results: {error}"))
  }

  pub fn list_indexed_file_paths_for_roots(&self, roots: &[String]) -> Result<Vec<String>, String> {
    if roots.is_empty() {
      return Ok(Vec::new());
    }

    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT normalized_path
        FROM file_records
        WHERE normalized_path = ?1 OR normalized_path LIKE ?2 ESCAPE '\';
        "#,
      )
      .map_err(|error| format!("Failed to prepare indexed path lookup query: {error}"))?;

    let mut dedup = HashSet::new();

    for root in roots {
      let normalized_root = normalize_path_string(root);
      if normalized_root.is_empty() {
        continue;
      }

      let normalized_root_prefix = normalized_root.trim_end_matches('\\');
      if normalized_root_prefix.is_empty() {
        continue;
      }

      let escaped_root_prefix = escape_like_pattern(normalized_root_prefix);
      let like_pattern = format!("{escaped_root_prefix}\\\\%");
      let path_iter = statement
        .query_map(params![normalized_root, like_pattern], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Failed to query indexed paths for root '{}': {error}", root))?;

      for path_result in path_iter {
        let path = path_result.map_err(|error| {
          format!("Failed to read indexed path row for root '{}': {error}", root)
        })?;
        dedup.insert(path);
      }
    }

    Ok(dedup.into_iter().collect())
  }

  pub fn delete_file_records_by_normalized_paths(&self, normalized_paths: &[String]) -> Result<u32, String> {
    if normalized_paths.is_empty() {
      return Ok(0);
    }

    let mut connection = self.open_connection()?;
    let transaction = connection
      .transaction()
      .map_err(|error| format!("Failed to open SQLite transaction for file pruning: {error}"))?;

    let mut statement = transaction
      .prepare("DELETE FROM file_records WHERE normalized_path = ?1;")
      .map_err(|error| format!("Failed to prepare file pruning statement: {error}"))?;

    let mut deleted_count: u32 = 0;

    for normalized_path in normalized_paths {
      let affected = statement
        .execute([normalized_path.as_str()])
        .map_err(|error| format!("Failed to prune file record '{}': {error}", normalized_path))?;
      deleted_count = deleted_count.saturating_add(affected as u32);
    }

    drop(statement);

    transaction
      .commit()
      .map_err(|error| format!("Failed to commit file pruning transaction: {error}"))?;

    Ok(deleted_count)
  }

  pub fn set_source_version(&self, source: &str, collector_version: &str) -> Result<(), String> {
    let connection = self.open_connection()?;
    let updated_at = current_timestamp_ms();

    connection
      .execute(
        r#"
        INSERT INTO source_versions (source, collector_version, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(source)
        DO UPDATE SET
          collector_version = excluded.collector_version,
          updated_at = excluded.updated_at;
        "#,
        params![source, collector_version, updated_at],
      )
      .map_err(|error| format!("Failed to set source version: {error}"))?;

    Ok(())
  }

  pub fn get_source_version(&self, source: &str) -> Result<Option<String>, String> {
    let connection = self.open_connection()?;

    connection
      .query_row(
        "SELECT collector_version FROM source_versions WHERE source = ?1;",
        [source],
        |row| row.get(0),
      )
      .optional()
      .map_err(|error| format!("Failed to read source version: {error}"))
  }

  fn get_app_record_by_id(&self, app_id: &str) -> Result<AppRecord, String> {
    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT
          id,
          name,
          normalized_name,
          aliases_json,
          source,
          launch_target,
          icon_key,
          merge_key,
          last_seen_at,
          created_at,
          updated_at
        FROM app_records
        WHERE id = ?1;
        "#,
      )
      .map_err(|error| format!("Failed to prepare app lookup query: {error}"))?;

    statement
      .query_row([app_id], map_app_record)
      .map_err(|error| format!("Failed to load app record '{app_id}': {error}"))
  }

  fn get_file_record_by_path(&self, path: &str) -> Result<FileRecord, String> {
    let connection = self.open_connection()?;
    let mut statement = connection
      .prepare(
        r#"
        SELECT
          id,
          name,
          extension,
          normalized_name,
          normalized_path,
          parent_path,
          size_bytes,
          modified_at,
          content_indexed,
          last_seen_at,
          created_at,
          updated_at
        FROM file_records
        WHERE normalized_path = ?1;
        "#,
      )
      .map_err(|error| format!("Failed to prepare file lookup query: {error}"))?;

    statement
      .query_row([normalize_path_string(path)], map_file_record)
      .map_err(|error| format!("Failed to load file record '{}': {error}", path))
  }

  fn open_connection(&self) -> Result<Connection, String> {
    let connection = Connection::open(&self.db_path).map_err(|error| {
      format!(
        "Failed to open SQLite database '{}': {error}",
        self.db_path.display()
      )
    })?;

    connection
      .execute_batch(
        r#"
        PRAGMA foreign_keys=ON;
        PRAGMA trusted_schema=OFF;
      "#,
      )
      .map_err(|error| format!("Failed to enable SQLite foreign keys: {error}"))?;

    Ok(connection)
  }
}

fn map_app_record(row: &Row<'_>) -> rusqlite::Result<AppRecord> {
  let aliases_json: String = row.get(3)?;
  let aliases = parse_aliases_json(&aliases_json, 3)?;

  Ok(AppRecord {
    id: row.get(0)?,
    name: row.get(1)?,
    normalized_name: row.get(2)?,
    aliases,
    source: row.get(4)?,
    launch_target: row.get(5)?,
    icon_key: row.get(6)?,
    merge_key: row.get(7)?,
    last_seen_at: row.get(8)?,
    created_at: row.get(9)?,
    updated_at: row.get(10)?,
  })
}

fn map_file_record(row: &Row<'_>) -> rusqlite::Result<FileRecord> {
  let content_indexed_as_int: i64 = row.get(8)?;

  Ok(FileRecord {
    id: row.get(0)?,
    name: row.get(1)?,
    extension: row.get(2)?,
    normalized_name: row.get(3)?,
    normalized_path: row.get(4)?,
    parent_path: row.get(5)?,
    size_bytes: row.get(6)?,
    modified_at: row.get(7)?,
    content_indexed: content_indexed_as_int != 0,
    last_seen_at: row.get(9)?,
    created_at: row.get(10)?,
    updated_at: row.get(11)?,
  })
}

fn parse_aliases_json(value: &str, column_index: usize) -> rusqlite::Result<Vec<String>> {
  serde_json::from_str(value).map_err(|error| {
    rusqlite::Error::FromSqlConversionFailure(column_index, Type::Text, Box::new(error))
  })
}

fn run_migrations(connection: &Connection) -> Result<(), String> {
  let user_version: i64 = connection
    .query_row("PRAGMA user_version;", [], |row| row.get(0))
    .map_err(|error| format!("Failed to read SQLite user_version: {error}"))?;

  if user_version >= CURRENT_SCHEMA_VERSION {
    return Ok(());
  }

  for version in (user_version + 1)..=CURRENT_SCHEMA_VERSION {
    match version {
      1 => connection
        .execute_batch(MIGRATION_0001_INITIAL)
        .map_err(|error| format!("Failed to apply migration v1: {error}"))?,
      2 => connection
        .execute_batch(MIGRATION_0002_FILE_RECORDS)
        .map_err(|error| format!("Failed to apply migration v2: {error}"))?,
      3 => connection
        .execute_batch(MIGRATION_0003_FILE_RECORDS_FTS5)
        .map_err(|error| format!("Failed to apply migration v3: {error}"))?,
      _ => {
        return Err(format!("Missing migration implementation for version {version}"));
      }
    }

    connection
      .pragma_update(None, "user_version", version)
      .map_err(|error| format!("Failed to bump SQLite user_version to {version}: {error}"))?;
  }

  Ok(())
}

fn normalize_text(value: &str) -> String {
  value
    .to_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

fn normalize_path_string(path: &str) -> String {
  path.trim().replace('/', "\\").to_lowercase()
}

fn escape_like_pattern(value: &str) -> String {
  let mut escaped = String::with_capacity(value.len());

  for ch in value.chars() {
    if ch == SQLITE_LIKE_ESCAPE_CHAR || ch == '%' || ch == '_' {
      escaped.push(SQLITE_LIKE_ESCAPE_CHAR);
    }
    escaped.push(ch);
  }

  escaped
}

fn build_fts_prefix_query(value: &str) -> Option<String> {
  let mut tokens: Vec<String> = Vec::new();

  for token in value.split_whitespace() {
    let cleaned = token.trim();
    if cleaned.is_empty() {
      continue;
    }

    let escaped = cleaned.replace('"', "\"\"");
    if escaped.is_empty() {
      continue;
    }

    tokens.push(format!("\"{escaped}\"*"));
  }

  if tokens.is_empty() {
    None
  } else {
    Some(tokens.join(" AND "))
  }
}

fn current_timestamp_ms() -> i64 {
  match SystemTime::now().duration_since(UNIX_EPOCH) {
    Ok(duration) => duration.as_millis() as i64,
    Err(_) => 0,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::{SystemTime, UNIX_EPOCH};

  #[test]
  fn list_indexed_paths_for_roots_escapes_like_wildcards() {
    let data_dir = unique_temp_dir("wildcard-root");
    let store = AppIndexStore::initialize(&data_dir).expect("store initialization should succeed");

    store
      .upsert_file_record(FileRecordUpsert {
        path: r"C:\safe%root\nested\inside.txt".to_string(),
        size_bytes: 11,
        modified_at: 1,
        content_text: None,
        last_seen_at: Some(1),
      })
      .expect("upsert for wildcard root path should succeed");
    store
      .upsert_file_record(FileRecordUpsert {
        path: r"C:\safeXroot\nested\outside.txt".to_string(),
        size_bytes: 12,
        modified_at: 2,
        content_text: None,
        last_seen_at: Some(2),
      })
      .expect("upsert for non-matching sibling path should succeed");

    let indexed = store
      .list_indexed_file_paths_for_roots(&[r"C:\safe%root".to_string()])
      .expect("indexed lookup should succeed");

    assert_eq!(indexed.len(), 1);
    assert!(indexed.iter().any(|value| value == r"c:\safe%root\nested\inside.txt"));

    let _ = fs::remove_dir_all(&data_dir);
  }

  #[test]
  fn search_files_uses_fts_content_matching() {
    let data_dir = unique_temp_dir("fts-content-match");
    let store = AppIndexStore::initialize(&data_dir).expect("store initialization should succeed");

    store
      .upsert_file_record(FileRecordUpsert {
        path: r"C:\docs\weekly-notes.txt".to_string(),
        size_bytes: 21,
        modified_at: 1,
        content_text: Some("release checklist hyperdrive beta".to_string()),
        last_seen_at: Some(10),
      })
      .expect("upsert for content-indexed file should succeed");

    let results = store
      .search_files("hyperdrive", 10)
      .expect("content-backed search should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].normalized_path, r"c:\docs\weekly-notes.txt");

    let _ = fs::remove_dir_all(&data_dir);
  }

  #[test]
  fn search_files_prefers_name_match_before_content_only_match() {
    let data_dir = unique_temp_dir("name-rank-priority");
    let store = AppIndexStore::initialize(&data_dir).expect("store initialization should succeed");

    store
      .upsert_file_record(FileRecordUpsert {
        path: r"C:\docs\secret-plan.txt".to_string(),
        size_bytes: 22,
        modified_at: 1,
        content_text: None,
        last_seen_at: Some(100),
      })
      .expect("upsert for name-match file should succeed");

    store
      .upsert_file_record(FileRecordUpsert {
        path: r"C:\docs\notes.txt".to_string(),
        size_bytes: 23,
        modified_at: 2,
        content_text: Some("contains secret architecture details".to_string()),
        last_seen_at: Some(200),
      })
      .expect("upsert for content-match file should succeed");

    let results = store
      .search_files("secret", 10)
      .expect("search should succeed");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].normalized_path, r"c:\docs\secret-plan.txt");
    assert_eq!(results[1].normalized_path, r"c:\docs\notes.txt");

    let _ = fs::remove_dir_all(&data_dir);
  }

  fn unique_temp_dir(prefix: &str) -> PathBuf {
    let unique_suffix = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("system time should be after unix epoch")
      .as_nanos();
    std::env::temp_dir().join(format!("winsearch-{prefix}-{unique_suffix}"))
  }
}
