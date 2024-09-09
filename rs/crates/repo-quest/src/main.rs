#![allow(warnings)]
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, error::Error, path::PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tokio::runtime::Handle;

#[tokio::main]
async fn main() {
  tauri::async_runtime::set(Handle::current());

  let specta_builder = repo_quest::specta_builder();
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_shell::init())
    .invoke_handler(specta_builder.invoke_handler())
    .setup(move |app| {
      #[cfg(debug_assertions)]
      {
        let window = app.get_webview_window("main").unwrap();
        window.open_devtools();
      }

      specta_builder.mount_events(app);

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
