// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tracing_subscriber::{fmt, layer::SubscriberExt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
  tracing_subscriber::registry()
    .with(fmt::layer())
    .with(EnvFilter::from_default_env())
    .init();

  tauri::async_runtime::set(tokio::runtime::Handle::current());

  let specta_builder = repo_quest::specta_builder();
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_shell::init())
    .invoke_handler(specta_builder.invoke_handler())
    .setup(move |app| {
      #[cfg(debug_assertions)]
      {
        use tauri::Manager;
        let window = app.get_webview_window("main").unwrap();
        window.open_devtools();
      }

      specta_builder.mount_events(app);

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
