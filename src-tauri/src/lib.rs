mod collectors;
mod db;

use collectors::{filesystem, start_menu, CollectionReport};
use db::{AppIndexStore, AppRecord, FileRecord, IndexStatus};
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::{
  fs,
  path::Path,
  path::PathBuf,
  process::Command,
  sync::{mpsc, Arc, Mutex},
  thread,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing_subscriber::{fmt, EnvFilter};

const OVERLAY_OPENED_EVENT: &str = "winsearch://overlay-opened";
const OVERLAY_CLOSED_EVENT: &str = "winsearch://overlay-closed";
const INDEXING_STATUS_EVENT: &str = "winsearch://indexing-status";
const HOTKEY_CANDIDATES: [&str; 2] = ["Alt+Space", "Ctrl+Shift+Space"];
const FILESYSTEM_BASELINE_SOURCE: &str = "filesystem_baseline";
const FILESYSTEM_BASELINE_VERSION: &str = "1";
const MAX_FILE_SEARCH_QUERY_CHARS: usize = 256;
const MAX_FILE_ID_CHARS: usize = 1024;
const WATCHER_DEBOUNCE_MS: u64 = 1500;

type IndexingStateHandle = Arc<Mutex<IndexingState>>;

#[derive(Default)]
struct HotkeyState {
  active_shortcut: Option<String>,
  failed_shortcuts: Vec<String>,
  fallback_in_use: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HotkeyStatus {
  active_shortcut: Option<String>,
  failed_shortcuts: Vec<String>,
  fallback_in_use: bool,
}

#[derive(Default)]
struct IndexingState {
  paused: bool,
  watcher_enabled: bool,
  baseline_complete: bool,
  scan_in_progress: bool,
  default_roots: Vec<String>,
  last_scan: Option<IndexingScanSummary>,
  last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexingScanSummary {
  mode: String,
  reason: String,
  scanned_entries: u32,
  indexed_entries: u32,
  skipped_entries: u32,
  pruned_entries: u32,
  error_count: u32,
  completed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexingStatus {
  paused: bool,
  watcher_enabled: bool,
  baseline_complete: bool,
  scan_in_progress: bool,
  default_roots: Vec<String>,
  last_scan: Option<IndexingScanSummary>,
  last_error: Option<String>,
}

#[tauri::command]
fn ping() -> &'static str {
  "pong"
}

#[tauri::command]
fn hide_overlay(app: AppHandle) -> Result<(), String> {
  let window = app
    .get_webview_window("main")
    .ok_or_else(|| "Main window is unavailable".to_string())?;

  let visible = window
    .is_visible()
    .map_err(|error| format!("Failed to read window visibility: {error}"))?;

  if visible {
    window
      .hide()
      .map_err(|error| format!("Failed to hide overlay: {error}"))?;
    let _ = app.emit(OVERLAY_CLOSED_EVENT, ());
  }

  Ok(())
}

#[tauri::command]
fn show_overlay(app: AppHandle) -> Result<(), String> {
  toggle_overlay_visibility(&app, true)
}

#[tauri::command]
fn get_hotkey_status(state: State<Mutex<HotkeyState>>) -> Result<HotkeyStatus, String> {
  let guard = state
    .lock()
    .map_err(|_| "Hotkey state lock is poisoned".to_string())?;

  Ok(HotkeyStatus {
    active_shortcut: guard.active_shortcut.clone(),
    failed_shortcuts: guard.failed_shortcuts.clone(),
    fallback_in_use: guard.fallback_in_use,
  })
}

#[tauri::command]
fn get_index_status(store: State<AppIndexStore>) -> Result<IndexStatus, String> {
  store.get_status()
}

#[tauri::command]
fn list_index_records(store: State<AppIndexStore>, limit: Option<u32>) -> Result<Vec<AppRecord>, String> {
  let bounded_limit = limit.unwrap_or(25).clamp(1, 200);
  store.list_apps(bounded_limit)
}

#[tauri::command]
fn get_index_source_version(
  store: State<AppIndexStore>,
  source: String,
) -> Result<Option<String>, String> {
  store.get_source_version(&source)
}

#[tauri::command]
fn get_default_file_index_roots() -> Vec<String> {
  filesystem::default_user_roots()
}

#[tauri::command]
fn get_indexing_status(indexing_state: State<IndexingStateHandle>) -> Result<IndexingStatus, String> {
  let guard = indexing_state
    .lock()
    .map_err(|_| "Indexing state lock is poisoned".to_string())?;

  Ok(snapshot_indexing_status(&guard))
}

#[tauri::command]
fn set_indexing_paused(
  app: AppHandle,
  indexing_state: State<IndexingStateHandle>,
  paused: bool,
) -> Result<IndexingStatus, String> {
  {
    let mut guard = indexing_state
      .lock()
      .map_err(|_| "Indexing state lock is poisoned".to_string())?;
    guard.paused = paused;
  }

  emit_indexing_status(&app, indexing_state.inner());
  get_indexing_status(indexing_state)
}

#[tauri::command]
fn collect_default_user_folders(
  app: AppHandle,
  store: State<AppIndexStore>,
  indexing_state: State<IndexingStateHandle>,
  incremental: Option<bool>,
) -> Result<CollectionReport, String> {
  let mode = if incremental.unwrap_or(false) {
    filesystem::CollectionMode::Incremental
  } else {
    filesystem::CollectionMode::Full
  };

  run_filesystem_scan(
    &app,
    store.inner(),
    indexing_state.inner(),
    mode,
    if matches!(mode, filesystem::CollectionMode::Full) {
      "manual_full_refresh"
    } else {
      "manual_incremental_refresh"
    },
    true,
  )
}

#[tauri::command]
fn list_file_index_records(store: State<AppIndexStore>, limit: Option<u32>) -> Result<Vec<FileRecord>, String> {
  let bounded_limit = limit.unwrap_or(50).clamp(1, 500);
  store.list_files(bounded_limit)
}

#[tauri::command]
fn search_file_index(
  store: State<AppIndexStore>,
  query: String,
  limit: Option<u32>,
) -> Result<Vec<FileRecord>, String> {
  let normalized_query = query.trim();
  if normalized_query.is_empty() {
    return Ok(Vec::new());
  }

  let safe_query = normalized_query.chars().take(MAX_FILE_SEARCH_QUERY_CHARS).collect::<String>();
  let bounded_limit = limit.unwrap_or(50).clamp(1, 500);
  store.search_files(&safe_query, bounded_limit)
}

#[tauri::command]
fn open_file_index_record(store: State<AppIndexStore>, file_id: String) -> Result<(), String> {
  let file_path = resolve_indexed_file_path(store.inner(), &file_id)?;
  open_path_with_system_default(&file_path)
}

#[tauri::command]
fn reveal_file_index_record(store: State<AppIndexStore>, file_id: String) -> Result<(), String> {
  let file_path = resolve_indexed_file_path(store.inner(), &file_id)?;

  let mut command = Command::new("explorer.exe");
  command.arg("/select,");
  command.arg(&file_path);

  command
    .spawn()
    .map_err(|error| format!("Failed to reveal file in Explorer '{}': {error}", file_path.display()))?;

  Ok(())
}

fn resolve_indexed_file_path(store: &AppIndexStore, file_id: &str) -> Result<PathBuf, String> {
  let normalized_file_id = file_id.trim();
  if normalized_file_id.is_empty() {
    return Err("Missing file id".to_string());
  }
  if normalized_file_id.chars().count() > MAX_FILE_ID_CHARS {
    return Err("File id is too long".to_string());
  }

  let file_record = store
    .get_file_record_by_id(normalized_file_id)?
    .ok_or_else(|| "The selected file is no longer indexed".to_string())?;

  let file_path = PathBuf::from(&file_record.normalized_path);
  if !file_path.is_absolute() {
    return Err("Indexed file path is invalid".to_string());
  }

  let metadata = fs::metadata(&file_path)
    .map_err(|_| format!("File no longer exists: {}", file_path.display()))?;

  if !metadata.is_file() {
    return Err(format!("Indexed path is not a file: {}", file_path.display()));
  }

  Ok(file_path)
}

fn open_path_with_system_default(path: &Path) -> Result<(), String> {
  #[cfg(target_os = "windows")]
  {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{Foundation::HWND, UI::Shell::ShellExecuteW};

    const SW_SHOWNORMAL: i32 = 1;

    let operation: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    let result = unsafe {
      ShellExecuteW(
        0 as HWND,
        operation.as_ptr(),
        file.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
        SW_SHOWNORMAL,
      )
    } as isize;

    if result <= 32 {
      return Err(format!(
        "Windows failed to open file '{}' (shell code {result})",
        path.display()
      ));
    }

    Ok(())
  }

  #[cfg(not(target_os = "windows"))]
  {
    let _ = path;
    Err("Opening files is only supported on Windows builds".to_string())
  }
}

fn toggle_overlay_visibility(app: &AppHandle, force_visible: bool) -> Result<(), String> {
  let window = app
    .get_webview_window("main")
    .ok_or_else(|| "Main window is unavailable".to_string())?;

  let currently_visible = window
    .is_visible()
    .map_err(|error| format!("Failed to read window visibility: {error}"))?;

  let should_show = force_visible || !currently_visible;

  if should_show {
    window
      .show()
      .map_err(|error| format!("Failed to show overlay: {error}"))?;
    window
      .set_focus()
      .map_err(|error| format!("Failed to focus overlay: {error}"))?;
    let _ = app.emit(OVERLAY_OPENED_EVENT, ());
    return Ok(());
  }

  window
    .hide()
    .map_err(|error| format!("Failed to hide overlay: {error}"))?;
  let _ = app.emit(OVERLAY_CLOSED_EVENT, ());

  Ok(())
}

fn register_overlay_hotkey(app: &AppHandle, state: &State<Mutex<HotkeyState>>) -> Result<(), String> {
  let mut lock = state
    .lock()
    .map_err(|_| "Hotkey state lock is poisoned".to_string())?;

  for (index, candidate) in HOTKEY_CANDIDATES.iter().enumerate() {
    match app.global_shortcut().on_shortcut(*candidate, |app, _shortcut, event| {
      if event.state == ShortcutState::Pressed {
        if let Err(error) = toggle_overlay_visibility(app, false) {
          tracing::error!(%error, "Failed to toggle overlay visibility from hotkey");
        }
      }
    }) {
      Ok(()) => {
        lock.active_shortcut = Some((*candidate).to_string());
        lock.fallback_in_use = index > 0;

        if index > 0 {
          tracing::warn!(
            preferred = HOTKEY_CANDIDATES[0],
            fallback = *candidate,
            "Preferred hotkey was unavailable; using fallback"
          );
        }

        return Ok(());
      }
      Err(error) => {
        let message = format!("{candidate}: {error}");
        tracing::warn!(%message, "Hotkey registration failed");
        lock.failed_shortcuts.push(message);
      }
    }
  }

  Err("Unable to register a global hotkey. Try closing other launcher apps first.".to_string())
}

fn init_logging() {
  let default_filter = if cfg!(debug_assertions) {
    "winsearch=debug,tauri=info"
  } else {
    "winsearch=info,tauri=warn"
  };

  let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

  let _ = fmt()
    .with_target(false)
    .with_env_filter(env_filter)
    .compact()
    .try_init();
}

fn snapshot_indexing_status(state: &IndexingState) -> IndexingStatus {
  IndexingStatus {
    paused: state.paused,
    watcher_enabled: state.watcher_enabled,
    baseline_complete: state.baseline_complete,
    scan_in_progress: state.scan_in_progress,
    default_roots: state.default_roots.clone(),
    last_scan: state.last_scan.clone(),
    last_error: state.last_error.clone(),
  }
}

fn emit_indexing_status(app: &AppHandle, indexing_state: &IndexingStateHandle) {
  let status = match indexing_state.lock() {
    Ok(guard) => snapshot_indexing_status(&guard),
    Err(_) => return,
  };
  let _ = app.emit(INDEXING_STATUS_EVENT, status);
}

fn current_timestamp_ms() -> i64 {
  match SystemTime::now().duration_since(UNIX_EPOCH) {
    Ok(duration) => duration.as_millis() as i64,
    Err(_) => 0,
  }
}

fn run_filesystem_scan(
  app: &AppHandle,
  store: &AppIndexStore,
  indexing_state: &IndexingStateHandle,
  mode: filesystem::CollectionMode,
  reason: &str,
  allow_when_paused: bool,
) -> Result<CollectionReport, String> {
  let roots = {
    let mut guard = indexing_state
      .lock()
      .map_err(|_| "Indexing state lock is poisoned".to_string())?;

    if guard.scan_in_progress {
      return Err("Indexing scan is already in progress".to_string());
    }

    if guard.paused && !allow_when_paused {
      return Err("Indexing is paused".to_string());
    }

    guard.scan_in_progress = true;
    guard.last_error = None;
    guard.default_roots.clone()
  };

  emit_indexing_status(app, indexing_state);

  if roots.is_empty() {
    if let Ok(mut guard) = indexing_state.lock() {
      guard.scan_in_progress = false;
      guard.last_error = Some("No default user folders are available for indexing".to_string());
    }
    emit_indexing_status(app, indexing_state);
    return Err("No default user folders are available for indexing".to_string());
  }

  let result = filesystem::collect_paths_with_mode(store, &roots, mode);

  match result {
    Ok(report) => {
      if matches!(mode, filesystem::CollectionMode::Full) {
        store
          .set_source_version(FILESYSTEM_BASELINE_SOURCE, FILESYSTEM_BASELINE_VERSION)
          .map_err(|error| format!("Failed to set filesystem baseline marker: {error}"))?;
      }

      if let Ok(mut guard) = indexing_state.lock() {
        guard.scan_in_progress = false;
        guard.baseline_complete = guard.baseline_complete || matches!(mode, filesystem::CollectionMode::Full);
        guard.last_scan = Some(IndexingScanSummary {
          mode: mode.as_str().to_string(),
          reason: reason.to_string(),
          scanned_entries: report.scanned_entries,
          indexed_entries: report.indexed_entries,
          skipped_entries: report.skipped_entries,
          pruned_entries: report.pruned_entries,
          error_count: report.errors.len() as u32,
          completed_at: current_timestamp_ms(),
        });
        guard.last_error = report.errors.first().cloned();
      }

      emit_indexing_status(app, indexing_state);
      Ok(report)
    }
    Err(error) => {
      if let Ok(mut guard) = indexing_state.lock() {
        guard.scan_in_progress = false;
        guard.last_error = Some(error.clone());
      }
      emit_indexing_status(app, indexing_state);
      Err(error)
    }
  }
}

fn start_filesystem_watcher(app: AppHandle, store: AppIndexStore, indexing_state: IndexingStateHandle) {
  thread::spawn(move || {
    let roots = match indexing_state.lock() {
      Ok(guard) => guard.default_roots.clone(),
      Err(_) => return,
    };

    if roots.is_empty() {
      return;
    }

    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(
      move |result| {
        let _ = tx.send(result);
      },
      NotifyConfig::default(),
    ) {
      Ok(watcher) => watcher,
      Err(error) => {
        if let Ok(mut guard) = indexing_state.lock() {
          guard.last_error = Some(format!("Failed to initialize filesystem watcher: {error}"));
          guard.watcher_enabled = false;
        }
        emit_indexing_status(&app, &indexing_state);
        return;
      }
    };

    for root in &roots {
      let root_path = PathBuf::from(root);
      if let Err(error) = watcher.watch(root_path.as_path(), RecursiveMode::Recursive) {
        if let Ok(mut guard) = indexing_state.lock() {
          guard.last_error = Some(format!("Failed to watch root '{}': {error}", root));
          guard.watcher_enabled = false;
        }
        emit_indexing_status(&app, &indexing_state);
        return;
      }
    }

    if let Ok(mut guard) = indexing_state.lock() {
      guard.watcher_enabled = true;
    }
    emit_indexing_status(&app, &indexing_state);

    let debounce_window = Duration::from_millis(WATCHER_DEBOUNCE_MS);
    let mut pending_event = false;
    let mut last_event_seen_at = Instant::now();

    loop {
      match rx.recv_timeout(Duration::from_millis(400)) {
        Ok(Ok(_)) => {
          pending_event = true;
          last_event_seen_at = Instant::now();
        }
        Ok(Err(error)) => {
          if let Ok(mut guard) = indexing_state.lock() {
            guard.last_error = Some(format!("Filesystem watcher error: {error}"));
          }
          emit_indexing_status(&app, &indexing_state);
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Err(mpsc::RecvTimeoutError::Disconnected) => break,
      }

      if !pending_event || last_event_seen_at.elapsed() < debounce_window {
        continue;
      }

      pending_event = false;

      let paused = match indexing_state.lock() {
        Ok(guard) => guard.paused,
        Err(_) => true,
      };
      if paused {
        continue;
      }

      if let Err(error) = run_filesystem_scan(
        &app,
        &store,
        &indexing_state,
        filesystem::CollectionMode::Incremental,
        "watcher_event",
        false,
      ) {
        if let Ok(mut guard) = indexing_state.lock() {
          guard.last_error = Some(format!("Watcher-triggered incremental scan failed: {error}"));
        }
        emit_indexing_status(&app, &indexing_state);
      }
    }
  });
}

pub fn run() {
  init_logging();

  tauri::Builder::default()
    .manage(Mutex::new(HotkeyState::default()))
    .manage(Arc::new(Mutex::new(IndexingState::default())))
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
    .setup(|app| {
      let index_data_dir = app.path().app_local_data_dir().map_err(|error| {
        std::io::Error::new(
          std::io::ErrorKind::Other,
          format!("Failed to resolve app-local data directory: {error}"),
        )
      })?;

      let index_store = AppIndexStore::initialize(&index_data_dir)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;

      let index_status = index_store
        .get_status()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;

      let indexing_state = app.state::<IndexingStateHandle>();
      {
        let mut guard = indexing_state
          .lock()
          .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Indexing state lock is poisoned"))?;
        guard.default_roots = filesystem::default_user_roots();
        guard.baseline_complete = index_store
          .get_source_version(FILESYSTEM_BASELINE_SOURCE)
          .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?
          .is_some();
      }

      tracing::info!(
        db_path = %index_status.db_path,
        schema_version = index_status.schema_version,
        app_count = index_status.app_count,
        file_count = index_status.file_count,
        "SQLite app index initialized"
      );

      match start_menu::collect(&index_store) {
        Ok(report) => {
          tracing::info!(
            source = report.source.as_str(),
            scanned_entries = report.scanned_entries,
            indexed_entries = report.indexed_entries,
            skipped_entries = report.skipped_entries,
            error_count = report.errors.len(),
            "Start-menu collector finished initial index pass"
          );
        }
        Err(error) => {
          tracing::error!(%error, "Start-menu collector failed during initial index pass");
        }
      }

      let startup_mode = {
        let guard = indexing_state
          .lock()
          .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Indexing state lock is poisoned"))?;
        if guard.baseline_complete {
          filesystem::CollectionMode::Incremental
        } else {
          filesystem::CollectionMode::Full
        }
      };

      if let Err(error) = run_filesystem_scan(
        &app.handle().clone(),
        &index_store,
        indexing_state.inner(),
        startup_mode,
        "startup",
        false,
      ) {
        tracing::error!(%error, "Filesystem collector failed during startup indexing pass");
      }

      app.manage(index_store);
      start_filesystem_watcher(app.handle().clone(), app.state::<AppIndexStore>().inner().clone(), indexing_state.inner().clone());

      let hotkey_state = app.state::<Mutex<HotkeyState>>();
      let registration_result = register_overlay_hotkey(&app.handle().clone(), &hotkey_state);

      if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();

        if let Err(error) = registration_result {
          tracing::error!(%error, "Failed to register global hotkey; showing window for manual access");
          let _ = window.show();
          let _ = window.set_focus();
        }
      }

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      ping,
      hide_overlay,
      show_overlay,
      get_hotkey_status,
      get_index_status,
      list_index_records,
      get_index_source_version,
      get_default_file_index_roots,
      get_indexing_status,
      set_indexing_paused,
      collect_default_user_folders,
      list_file_index_records,
      search_file_index,
      open_file_index_record,
      reveal_file_index_record
    ])
    .run(tauri::generate_context!())
    .expect("error while running WinSearch application");
}
