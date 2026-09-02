#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use clipbox_core::{ClipboardEntry as CoreClipboardEntry, ClipboardStore};
use serde::Serialize;
use tauri::Manager;

mod clipboard;
mod source;

struct AppState {
    database_path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardEntry {
    id: i64,
    content: String,
    copied_at: i64,
    source_app: Option<String>,
    source_process: Option<String>,
    window_title: Option<String>,
    app_icon: Option<String>,
}

impl From<CoreClipboardEntry> for ClipboardEntry {
    fn from(entry: CoreClipboardEntry) -> Self {
        Self {
            id: entry.id,
            content: entry.content,
            copied_at: entry.copied_at,
            source_app: entry.source_app,
            source_process: entry.source_process,
            window_title: entry.window_title,
            app_icon: entry.app_icon,
        }
    }
}

#[tauri::command]
fn list_entries(state: tauri::State<'_, AppState>) -> Result<Vec<ClipboardEntry>, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .recent_entries(100)
        .map(|entries| entries.into_iter().map(ClipboardEntry::from).collect())
        .map_err(|error| format!("could not read Clipbox entries: {error}"))
}

// ----------
// Clear History Command
// Description: IPC command allowing the frontend to permanently clear all stored clipboard records from the database.
// ----------

#[tauri::command]
fn clear_entries(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .clear_entries()
        .map_err(|error| format!("could not clear Clipbox entries: {error}"))
}

// ----------
// Window Control Commands
// Description: IPC commands for custom titlebar actions including minimize, toggle maximize, close, and query maximized state.
// ----------

#[tauri::command]
fn minimize_window(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_maximize_window(window: tauri::Window) -> Result<(), String> {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|error| error.to_string())
    } else {
        window.maximize().map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn close_window(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|error| error.to_string())
}

#[tauri::command]
fn start_dragging(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn is_window_maximized(window: tauri::Window) -> Result<bool, String> {
    window.is_maximized().map_err(|error| error.to_string())
}

/// Run the Clipbox desktop application.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database_directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&database_directory)?;

            let database_path = database_directory.join("clipbox.sqlite3");
            app.manage(AppState {
                database_path: database_path.clone(),
            });
            clipboard::start(database_path);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_entries,
            clear_entries,
            minimize_window,
            toggle_maximize_window,
            close_window,
            start_dragging,
            is_window_maximized
        ])
        .run(tauri::generate_context!())
        .expect("error while running Clipbox");
}
