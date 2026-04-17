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
    match app.global_shortcut().on_shortcut(candidate, |app, _shortcut, event| {
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
    .invoke_handler(tauri::generate_handler![ping, hide_overlay, show_overlay, get_hotkey_status])
    .run(tauri::generate_context!())
    .expect("error while running WinSearch application");
}
