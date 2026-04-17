use rusqlite::{params, types::Type, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
  fs,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

const DB_FILE_NAME: &str = "winsearch.db";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const MIGRATION_0001_INITIAL: &str = include_str!("../../migrations/0001_initial.sql");

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

    let source_version_count: i64 = connection
      .query_row("SELECT COUNT(*) FROM source_versions;", [], |row| row.get(0))
      .map_err(|error| format!("Failed to count source versions: {error}"))?;

    Ok(IndexStatus {
      db_path: self.db_path.to_string_lossy().into_owned(),
      schema_version,
      app_count,
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

  fn open_connection(&self) -> Result<Connection, String> {
    let connection = Connection::open(&self.db_path).map_err(|error| {
      format!(
        "Failed to open SQLite database '{}': {error}",
        self.db_path.display()
      )
    })?;

    connection
      .execute_batch("PRAGMA foreign_keys=ON;")
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

fn current_timestamp_ms() -> i64 {
  match SystemTime::now().duration_since(UNIX_EPOCH) {
    Ok(duration) => duration.as_millis() as i64,
    Err(_) => 0,
  }
}
