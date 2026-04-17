use tracing_subscriber::{fmt, EnvFilter};

#[tauri::command]
fn ping() -> &'static str {
  "pong"
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
    .invoke_handler(tauri::generate_handler![ping])
    .run(tauri::generate_context!())
    .expect("error while running WinSearch application");
}
