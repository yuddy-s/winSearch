use crate::db::{AppIndexStore, FileRecordSnapshot, FileRecordUpsert};
use std::{
  collections::HashSet,
  env,
  fs,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

use super::CollectionReport;

const SOURCE_NAME: &str = "filesystem";
const COLLECTOR_VERSION: &str = "1";
const MAX_CONTENT_BYTES: u64 = 256 * 1024;
const MAX_CONTENT_CHARS: usize = 200_000;
const MAX_ERROR_MESSAGES: usize = 150;
const MAX_INDEX_FILES_PER_RUN: usize = 200_000;
const MAX_WALK_DEPTH: usize = 64;

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;

const TEXT_CONTENT_EXTENSIONS: [&str; 18] = [
  "txt", "md", "markdown", "log", "json", "csv", "yaml", "yml", "toml", "xml", "html", "css", "js",
  "ts", "tsx", "jsx", "rs", "py",
];

const SKIP_DIR_NAMES: [&str; 8] = [
  ".git",
  ".idea",
  ".vscode",
  "node_modules",
  "target",
  "dist",
  "coverage",
  ".next",
];

#[derive(Debug, Clone, Copy)]
pub enum CollectionMode {
  Full,
  Incremental,
}

impl CollectionMode {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Full => "full",
      Self::Incremental => "incremental",
    }
  }
}

pub fn default_user_roots() -> Vec<String> {
  let mut roots = Vec::new();
  let mut seen = HashSet::new();

  let user_profile = match env::var("USERPROFILE") {
    Ok(value) => PathBuf::from(value),
    Err(_) => return roots,
  };

  for relative in ["Desktop", "Documents", "Downloads", "Pictures", "Music", "Videos"] {
    let absolute = user_profile.join(relative);
    if !absolute.exists() || !absolute.is_dir() {
      continue;
    }

    let value = absolute.to_string_lossy().into_owned();
    let key = value.to_ascii_lowercase();
    if seen.insert(key) {
      roots.push(value);
    }
  }

  roots
}

pub fn collect_paths_with_mode(
  store: &AppIndexStore,
  roots: &[String],
  mode: CollectionMode,
) -> Result<CollectionReport, String> {
  if roots.is_empty() {
    return Err("No folder paths provided for filesystem collection".to_string());
  }

  let mut report = CollectionReport::new(SOURCE_NAME);
  report.mode = Some(mode.as_str().to_string());
  let mut file_paths = Vec::new();
  let mut did_hit_file_cap = false;

  for root in roots {
    let root_path = PathBuf::from(root);

    if !root_path.exists() {
      push_error(
        &mut report,
        format!("Filesystem root does not exist and was skipped: '{}'", root_path.display()),
      );
      report.skipped_entries += 1;
      continue;
    }

    if root_path.is_file() {
      file_paths.push(root_path);
      continue;
    }

    let limit_reached = collect_files_from_root(&root_path, &mut file_paths, &mut report);
    if limit_reached {
      did_hit_file_cap = true;
      break;
    }
  }

  report.scanned_entries = file_paths.len() as u32;

  for file_path in &file_paths {
    if !file_path.is_file() {
      report.skipped_entries += 1;
      continue;
    }

    let metadata = match fs::metadata(&file_path) {
      Ok(metadata) => metadata,
      Err(error) => {
        push_error(
          &mut report,
          format!("Failed to read file metadata '{}': {error}", file_path.display()),
        );
        report.skipped_entries += 1;
        continue;
      }
    };

    let modified_at = metadata
      .modified()
      .ok()
      .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
      .map(|value| value.as_millis() as i64)
      .unwrap_or_else(current_timestamp_ms);

    let path_string = file_path.to_string_lossy().into_owned();
    let snapshot = if matches!(mode, CollectionMode::Incremental) {
      match store.get_file_record_snapshot(&path_string) {
        Ok(snapshot) => snapshot,
        Err(error) => {
          push_error(
            &mut report,
            format!("Failed to check existing file snapshot '{}': {error}", file_path.display()),
          );
          report.skipped_entries += 1;
          continue;
        }
      }
    } else {
      None
    };

    if is_unchanged(snapshot.as_ref(), metadata.len() as i64, modified_at) {
      report.skipped_entries += 1;
      continue;
    }

    let content_text = read_file_content_if_supported(&file_path, metadata.len());

    let upsert = FileRecordUpsert {
      path: path_string,
      size_bytes: metadata.len() as i64,
      modified_at,
      content_text,
      last_seen_at: None,
    };

    match store.upsert_file_record(upsert) {
      Ok(_) => report.indexed_entries += 1,
      Err(error) => {
        report.skipped_entries += 1;
        push_error(
          &mut report,
          format!("Failed to upsert file record '{}': {error}", file_path.display()),
        );
      }
    }
  }

  if matches!(mode, CollectionMode::Incremental) && !did_hit_file_cap {
    let mut scanned_normalized_paths = HashSet::new();
    for file_path in &file_paths {
      scanned_normalized_paths.insert(normalize_path_for_lookup(file_path));
    }

    let indexed_paths = store.list_indexed_file_paths_for_roots(roots)?;
    let paths_to_prune = indexed_paths
      .into_iter()
      .filter(|path| !scanned_normalized_paths.contains(path))
      .collect::<Vec<_>>();

    report.pruned_entries = store.delete_file_records_by_normalized_paths(&paths_to_prune)?;
  }

  if let Err(error) = store.set_source_version(SOURCE_NAME, COLLECTOR_VERSION) {
    push_error(
      &mut report,
      format!("Failed to record collector version for filesystem source: {error}"),
    );
  }

  Ok(report)
}

fn is_unchanged(snapshot: Option<&FileRecordSnapshot>, size_bytes: i64, modified_at: i64) -> bool {
  let Some(snapshot) = snapshot else {
    return false;
  };

  snapshot.size_bytes == size_bytes && snapshot.modified_at == modified_at
}

fn collect_files_from_root(root: &Path, output: &mut Vec<PathBuf>, report: &mut CollectionReport) -> bool {
  let mut stack = vec![(root.to_path_buf(), 0usize)];
  let mut visited_dirs: HashSet<String> = HashSet::new();

  while let Some((current_dir, depth)) = stack.pop() {
    if depth > MAX_WALK_DEPTH {
      push_error(
        report,
        format!(
          "Skipping deep directory traversal at '{}' due to max depth limit",
          current_dir.display()
        ),
      );
      continue;
    }

    let canonical_key = fs::canonicalize(&current_dir)
      .unwrap_or_else(|_| current_dir.clone())
      .to_string_lossy()
      .to_ascii_lowercase();

    if !visited_dirs.insert(canonical_key) {
      continue;
    }

    let entries = match fs::read_dir(&current_dir) {
      Ok(entries) => entries,
      Err(error) => {
        push_error(
          report,
          format!(
            "Failed to read filesystem directory '{}': {error}",
            current_dir.display()
          ),
        );
        continue;
      }
    };

    for entry_result in entries {
      let entry = match entry_result {
        Ok(entry) => entry,
        Err(error) => {
          push_error(report, format!("Failed to read filesystem directory entry: {error}"));
          continue;
        }
      };

      let entry_path = entry.path();
      let entry_metadata = match fs::symlink_metadata(&entry_path) {
        Ok(metadata) => metadata,
        Err(error) => {
          push_error(
            report,
            format!("Failed to read filesystem entry metadata '{}': {error}", entry_path.display()),
          );
          continue;
        }
      };

      if is_symlink_or_reparse(&entry_metadata) {
        continue;
      }

      if entry_path.is_dir() {
        if should_skip_dir(&entry_path) {
          continue;
        }

        stack.push((entry_path, depth + 1));
        continue;
      }

      if output.len() >= MAX_INDEX_FILES_PER_RUN {
        push_error(
          report,
          format!(
            "Reached max files-per-run limit ({MAX_INDEX_FILES_PER_RUN}); remaining files skipped"
          ),
        );
        return true;
      }

      output.push(entry_path);
    }
  }

  false
}

fn should_skip_dir(path: &Path) -> bool {
  path
    .file_name()
    .and_then(|value| value.to_str())
    .map(|value| {
      let lowercase = value.to_ascii_lowercase();
      SKIP_DIR_NAMES.contains(&lowercase.as_str())
    })
    .unwrap_or(false)
}

fn read_file_content_if_supported(path: &Path, size_bytes: u64) -> Option<String> {
  if size_bytes == 0 || size_bytes > MAX_CONTENT_BYTES {
    return None;
  }

  if !has_supported_text_extension(path) {
    return None;
  }

  let bytes = fs::read(path).ok()?;
  let mut content = String::from_utf8_lossy(&bytes).into_owned();
  content.truncate(MAX_CONTENT_CHARS);

  let trimmed = content.trim();
  if trimmed.is_empty() {
    return None;
  }

  Some(trimmed.to_string())
}

fn has_supported_text_extension(path: &Path) -> bool {
  let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
    return false;
  };

  let lowercase_extension = extension.to_ascii_lowercase();
  TEXT_CONTENT_EXTENSIONS.contains(&lowercase_extension.as_str())
}

fn push_error(report: &mut CollectionReport, message: String) {
  if report.errors.len() < MAX_ERROR_MESSAGES {
    report.errors.push(message);
  }
}

fn normalize_path_for_lookup(path: &Path) -> String {
  path.to_string_lossy().trim().replace('/', "\\").to_lowercase()
}

fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
  if metadata.file_type().is_symlink() {
    return true;
  }

  #[cfg(windows)]
  {
    use std::os::windows::fs::MetadataExt;
    return (metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
  }

  #[cfg(not(windows))]
  {
    false
  }
}

fn current_timestamp_ms() -> i64 {
  match SystemTime::now().duration_since(UNIX_EPOCH) {
    Ok(duration) => duration.as_millis() as i64,
    Err(_) => 0,
  }
}
