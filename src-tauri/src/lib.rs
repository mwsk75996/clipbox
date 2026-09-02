#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use clipbox_core::{ClipboardEntry as CoreClipboardEntry, ClipboardStore};
use serde::Serialize;
use tauri::Manager;

mod autostart;
mod clipboard;
mod source;

struct AppState {
    database_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub id: i64,
    pub content: String,
    pub copied_at: i64,
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    pub window_title: Option<String>,
    pub app_icon: Option<String>,
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
    // Hide to system tray instead of terminating the app
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn hide_window(window: tauri::Window) -> Result<(), String> {
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn show_window(window: tauri::Window) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn start_dragging(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn is_window_maximized(window: tauri::Window) -> Result<bool, String> {
    window.is_maximized().map_err(|error| error.to_string())
}

// ----------
// Always on Top Window Commands
// Description: IPC commands to dynamically pin/unpin the Clipbox window above other applications and query the current pinning state.
// ----------

#[tauri::command]
fn set_always_on_top(window: tauri::Window, always_on_top: bool) -> Result<(), String> {
    window
        .set_always_on_top(always_on_top)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn is_always_on_top(window: tauri::Window) -> Result<bool, String> {
    window
        .is_always_on_top()
        .map_err(|error| error.to_string())
}

// ----------
// Autostart IPC Commands
// Description: Commands allowing the frontend to query and configure OS boot autostart registration.
// ----------

#[tauri::command]
fn is_autostart_enabled() -> Result<bool, String> {
    autostart::is_autostart_enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    autostart::set_autostart(enabled)
}

// ----------
// Start Minimized Setting Commands
// Description: Commands allowing the frontend to query and configure whether Clipbox starts minimized to the system tray.
// ----------

#[tauri::command]
fn is_start_minimized(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    Ok(store.get_setting("start_minimized").unwrap_or_default().as_deref() == Some("true"))
}

#[tauri::command]
fn set_start_minimized(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    let val = if enabled { "true" } else { "false" };
    store
        .set_setting("start_minimized", val)
        .map_err(|error| format!("could not save setting: {error}"))
}

// ----------
// Database Path & Explorer Reveal Commands
// Description: Returns the dynamic resolved filesystem path of the active SQLite database and opens it directly in Windows File Explorer.
// ----------

#[tauri::command]
fn get_database_path(state: tauri::State<'_, AppState>) -> Result<String, String> {
    Ok(state.database_path.to_string_lossy().to_string())
}

#[tauri::command]
fn open_database_directory(state: tauri::State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,\"{}\"", state.database_path.display()))
            .spawn()
            .map_err(|e| format!("could not open explorer: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if let Some(parent) = state.database_path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
        Ok(())
    }
}

// ----------
// Retention Limit Setting Commands
// Description: Queries and updates the history retention limit, immediately pruning existing surplus records if a smaller limit is selected.
// ----------

#[tauri::command]
fn get_retention_limit(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    Ok(store
        .get_setting("retention_limit")
        .unwrap_or_default()
        .unwrap_or_else(|| "500".into()))
}

#[tauri::command]
fn set_retention_limit(state: tauri::State<'_, AppState>, limit: String) -> Result<usize, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .set_setting("retention_limit", &limit)
        .map_err(|error| format!("could not save retention limit setting: {error}"))?;

    let pruned = if limit == "unlimited" {
        0
    } else if let Ok(parsed) = limit.parse::<usize>() {
        store.prune_entries(parsed).unwrap_or(0)
    } else {
        0
    };

    Ok(pruned)
}

// ----------
// Native Clipboard Copy Command
// Description: Writes text to the OS clipboard directly using arboard, ensuring copy actions from Clipbox cards succeed without browser focus limitations.
// ----------

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
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
            clipboard::start(database_path.clone(), app.handle().clone());

            let store = ClipboardStore::open(&database_path).ok();
            let is_minimized = std::env::args().any(|arg| arg == "--minimized" || arg == "--hidden")
                || store
                    .as_ref()
                    .and_then(|s| s.get_setting("start_minimized").ok().flatten())
                    .as_deref()
                    == Some("true");

            // Setup system tray menu and icon
            let show_item = tauri::menu::MenuItem::with_id(app, "show", "Show Clipbox", true, None::<&str>)?;
            let hide_item = tauri::menu::MenuItem::with_id(app, "hide", "Hide to Tray", true, None::<&str>)?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "Quit Clipbox", true, None::<&str>)?;
            let tray_menu = tauri::menu::Menu::with_items(app, &[&show_item, &hide_item, &separator, &quit_item])?;

            let mut tray_builder = tauri::tray::TrayIconBuilder::new()
                .tooltip("Clipbox")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _tray = tray_builder.build(app)?;

            // Initial window visibility based on start_minimized preference
            if let Some(window) = app.get_webview_window("main") {
                if !is_minimized {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_entries,
            clear_entries,
            minimize_window,
            toggle_maximize_window,
            close_window,
            hide_window,
            show_window,
            exit_app,
            copy_to_clipboard,
            start_dragging,
            is_window_maximized,
            set_always_on_top,
            is_always_on_top,
            is_autostart_enabled,
            set_autostart,
            is_start_minimized,
            set_start_minimized,
            get_database_path,
            open_database_directory,
            get_retention_limit,
            set_retention_limit
        ])
        .run(tauri::generate_context!())
        .expect("error while running Clipbox");
}
