use crate::db::{AppIndexStore, AppRecordUpsert};
use std::{
  env,
  fs,
  path::{Path, PathBuf},
};

use super::CollectionReport;

const SOURCE_NAME: &str = "start_menu";
const COLLECTOR_VERSION: &str = "1";

pub fn collect(store: &AppIndexStore) -> Result<CollectionReport, String> {
  let mut report = CollectionReport::new(SOURCE_NAME);
  let roots = get_start_menu_roots();
  let mut shortcut_paths = Vec::new();

  for root in roots {
    if !root.exists() {
      continue;
    }

    collect_shortcut_paths(&root, &mut shortcut_paths, &mut report);
  }

  report.scanned_entries = shortcut_paths.len() as u32;

  for shortcut_path in shortcut_paths {
    let Some(name) = shortcut_path
      .file_stem()
      .and_then(|value| value.to_str())
      .map(str::trim)
      .filter(|value| !value.is_empty())
      .map(ToString::to_string)
    else {
      report.skipped_entries += 1;
      continue;
    };

    let source_identifier = shortcut_path.to_string_lossy().into_owned();
    let merge_key = build_merge_key(&name);

    let record = AppRecordUpsert {
      name,
      aliases: Vec::new(),
      source: SOURCE_NAME.to_string(),
      source_identifier,
      launch_target: shortcut_path.to_string_lossy().into_owned(),
      icon_key: None,
      merge_key,
      last_seen_at: None,
    };

    match store.upsert_app_record(record) {
      Ok(_) => report.indexed_entries += 1,
      Err(error) => {
        report.skipped_entries += 1;
        report.errors.push(format!(
          "Failed to index start-menu shortcut '{}': {error}",
          shortcut_path.display()
        ));
      }
    }
  }

  if let Err(error) = store.set_source_version(SOURCE_NAME, COLLECTOR_VERSION) {
    report
      .errors
      .push(format!("Failed to record collector version for start-menu source: {error}"));
  }

  Ok(report)
}

fn get_start_menu_roots() -> Vec<PathBuf> {
  let mut roots = Vec::new();

  if let Ok(program_data) = env::var("ProgramData") {
    roots.push(PathBuf::from(program_data).join("Microsoft\\Windows\\Start Menu\\Programs"));
  }

  if let Ok(app_data) = env::var("APPDATA") {
    roots.push(PathBuf::from(app_data).join("Microsoft\\Windows\\Start Menu\\Programs"));
  }

  roots
}

fn collect_shortcut_paths(root: &Path, output: &mut Vec<PathBuf>, report: &mut CollectionReport) {
  let mut stack = vec![root.to_path_buf()];

  while let Some(current_dir) = stack.pop() {
    let read_dir_result = fs::read_dir(&current_dir);

    let entries = match read_dir_result {
      Ok(entries) => entries,
      Err(error) => {
        report.errors.push(format!(
          "Failed to read start-menu directory '{}': {error}",
          current_dir.display()
        ));
        continue;
      }
    };

    for entry_result in entries {
      let entry = match entry_result {
        Ok(entry) => entry,
        Err(error) => {
          report
            .errors
            .push(format!("Failed to read start-menu directory entry: {error}"));
          continue;
        }
      };

      let entry_path = entry.path();

      if entry_path.is_dir() {
        stack.push(entry_path);
        continue;
      }

      if has_shortcut_extension(&entry_path) {
        output.push(entry_path);
      }
    }
  }
}

fn has_shortcut_extension(path: &Path) -> bool {
  path
    .extension()
    .and_then(|value| value.to_str())
    .map(|value| value.eq_ignore_ascii_case("lnk"))
    .unwrap_or(false)
}

fn build_merge_key(name: &str) -> String {
  let normalized = normalize_for_key(name);

  if normalized.is_empty() {
    return "start_menu::unknown".to_string();
  }

  format!("start_menu::{normalized}")
}

fn normalize_for_key(value: &str) -> String {
  value
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() {
        character.to_ascii_lowercase()
      } else {
        '-'
      }
    })
    .collect::<String>()
    .split('-')
    .filter(|segment| !segment.is_empty())
    .collect::<Vec<_>>()
    .join("-")
}
