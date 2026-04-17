mod collectors;
mod db;

use collectors::{start_menu, CollectionReport};
use db::{AppIndexStore, AppRecord, AppRecordUpsert, IndexStatus};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing_subscriber::{fmt, EnvFilter};

const OVERLAY_OPENED_EVENT: &str = "winsearch://overlay-opened";
const OVERLAY_CLOSED_EVENT: &str = "winsearch://overlay-closed";
const HOTKEY_CANDIDATES: [&str; 2] = ["Alt+Space", "Ctrl+Shift+Space"];

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
fn upsert_index_record(
  store: State<AppIndexStore>,
  record: AppRecordUpsert,
) -> Result<AppRecord, String> {
  store.upsert_app_record(record)
}

#[tauri::command]
fn list_index_records(store: State<AppIndexStore>, limit: Option<u32>) -> Result<Vec<AppRecord>, String> {
  let bounded_limit = limit.unwrap_or(25).clamp(1, 200);
  store.list_apps(bounded_limit)
}

#[tauri::command]
fn set_index_source_version(
  store: State<AppIndexStore>,
  source: String,
  collector_version: String,
) -> Result<(), String> {
  store.set_source_version(&source, &collector_version)
}

#[tauri::command]
fn get_index_source_version(
  store: State<AppIndexStore>,
  source: String,
) -> Result<Option<String>, String> {
  store.get_source_version(&source)
}

#[tauri::command]
fn collect_start_menu_apps(store: State<AppIndexStore>) -> Result<CollectionReport, String> {
  start_menu::collect(store.inner())
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

pub fn run() {
  init_logging();

  tauri::Builder::default()
    .manage(Mutex::new(HotkeyState::default()))
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

      tracing::info!(
        db_path = %index_status.db_path,
        schema_version = index_status.schema_version,
        app_count = index_status.app_count,
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

      app.manage(index_store);

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
      upsert_index_record,
      list_index_records,
      set_index_source_version,
      get_index_source_version,
      collect_start_menu_apps
    ])
    .run(tauri::generate_context!())
    .expect("error while running WinSearch application");
}
