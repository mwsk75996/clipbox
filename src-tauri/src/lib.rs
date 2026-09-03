#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use clipbox_core::{ClipboardEntry as CoreClipboardEntry, ClipboardStore};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

mod autostart;
mod browser_url;
mod clipboard;
mod file_clipboard;
mod image_clipboard;
mod source;

struct AppState {
    database_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub id: i64,
    pub content: String,
    pub copied_at: i64,
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    pub window_title: Option<String>,
    pub app_icon: Option<String>,
    pub is_pinned: bool,
    pub entry_type: String,
    pub image_data: Option<String>,
    pub image_dimensions: Option<String>,
    pub files_data: Option<String>,
    pub source_url: Option<String>,
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
            is_pinned: entry.is_pinned,
            entry_type: entry.entry_type,
            image_data: entry.image_data,
            image_dimensions: entry.image_dimensions,
            files_data: entry.files_data,
            source_url: entry.source_url,
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
// Delete Single Entry Command
// Description: IPC command allowing the frontend to permanently remove an individual clipboard record by its unique ID.
// ----------

#[tauri::command]
fn delete_entry(state: tauri::State<'_, AppState>, id: i64) -> Result<bool, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .delete_entry(id)
        .map_err(|error| format!("could not delete Clipbox entry: {error}"))
}

// ----------
// Pin Entry Command
// Description: IPC command allowing the frontend to toggle the pinned status of a clipboard entry.
// ----------

#[tauri::command]
fn toggle_pinned(state: tauri::State<'_, AppState>, id: i64) -> Result<bool, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .toggle_pinned(id)
        .map_err(|error| format!("could not toggle pin status: {error}"))
}

// ----------
// Window Control Commands
// Description: IPC commands retained for the startup fallback page and hide-to-tray lifecycle.
// ----------

#[tauri::command]
fn minimize_window(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_maximize_window(window: tauri::Window) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            IsZoomed, PostMessageW, SC_MAXIMIZE, SC_RESTORE, WM_SYSCOMMAND,
        };

        let raw_hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let hwnd = HWND(raw_hwnd.0 as _);
        let command = if unsafe { IsZoomed(hwnd).as_bool() } {
            SC_RESTORE
        } else {
            SC_MAXIMIZE
        };

        unsafe {
            PostMessageW(Some(hwnd), WM_SYSCOMMAND, WPARAM(command as usize), LPARAM(0))
                .map_err(|error| error.to_string())
        }
    }

    #[cfg(not(windows))]
    {
        if window.is_maximized().unwrap_or(false) {
            window.unmaximize().map_err(|error| error.to_string())
        } else {
            window.maximize().map_err(|error| error.to_string())
        }
    }
}

#[tauri::command]
fn begin_window_drag(window: tauri::Window) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
        use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetCursorPos, PostMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
        };

        let raw_hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let hwnd = HWND(raw_hwnd.0 as _);
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }.map_err(|error| error.to_string())?;
        let coordinates =
            ((cursor.y as u16 as u32) << 16) | cursor.x as u16 as u32;

        unsafe {
            let _ = ReleaseCapture();
            PostMessageW(
                Some(hwnd),
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(coordinates as isize),
            )
            .map_err(|error| error.to_string())
        }
    }

    #[cfg(not(windows))]
    {
        window.start_dragging().map_err(|error| error.to_string())
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
        use std::os::windows::process::CommandExt;
        let clean_path = state.database_path.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{clean_path}\""))
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
// Open in File Explorer Command
// Description: Opens native Windows File Explorer to reveal and select a file, or directly opens a directory.
// ----------

#[tauri::command]
fn open_in_explorer(path: String) -> Result<(), String> {
    let clean_path = path.replace('/', "\\");
    let p = std::path::PathBuf::from(&clean_path);
    if !p.exists() {
        return Err("File or folder does not exist on disk".into());
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if p.is_dir() {
            std::process::Command::new("explorer")
                .raw_arg(format!("\"{clean_path}\""))
                .spawn()
                .map_err(|e| format!("could not open explorer: {e}"))?;
        } else {
            std::process::Command::new("explorer")
                .raw_arg(format!("/select,\"{clean_path}\""))
                .spawn()
                .map_err(|e| format!("could not open explorer: {e}"))?;
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let target = if p.is_dir() {
            p
        } else {
            p.parent().map(|parent| parent.to_path_buf()).unwrap_or(p)
        };
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
        Ok(())
    }
}

// ----------
// Open URL in Default Browser
// Description: Opens a validated HTTP/HTTPS web address in the user's default browser.
// ----------

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("rundll32")
                .raw_arg(format!("url.dll,FileProtocolHandler \"{trimmed}\""))
                .spawn()
                .map_err(|e| format!("could not open url: {e}"))?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("xdg-open")
                .arg(trimmed)
                .spawn()
                .map_err(|e| format!("could not open url: {e}"))?;
            Ok(())
        }
    } else {
        Err("Invalid or unsupported URL protocol".into())
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
// Privacy Settings
// Description: Settings for clipboard capture pause/resume, password manager filtering, excluded applications, and duplicate handling.
// ----------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PrivacySettings {
    pub monitoring_paused: bool,
    pub ignore_password_managers: bool,
    pub excluded_applications: Vec<String>,
    pub duplicate_handling: String, // "bump", "ignore", "create_new"
}

// ----------
// Keybind Customization
// Description: User-defined keybindings for app shortcuts with modifier keys, key codes, and human-readable labels.
// ----------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct KeyBinding {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    pub label: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ShortcutSettings {
    pub focus_search: KeyBinding,
    pub nav_down: KeyBinding,
    pub nav_up: KeyBinding,
    pub copy_entry: KeyBinding,
    pub expand_preview: KeyBinding,
    pub toggle_pin: KeyBinding,
    pub delete_entry: KeyBinding,
    pub clear_escape: KeyBinding,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            focus_search: KeyBinding {
                key: "f".into(),
                ctrl: true,
                shift: false,
                alt: false,
                meta: false,
                label: "Ctrl + F".into(),
            },
            nav_down: KeyBinding {
                key: "ArrowDown".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "↓ (Arrow Down)".into(),
            },
            nav_up: KeyBinding {
                key: "ArrowUp".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "↑ (Arrow Up)".into(),
            },
            copy_entry: KeyBinding {
                key: "Enter".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "Enter".into(),
            },
            expand_preview: KeyBinding {
                key: " ".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "Space".into(),
            },
            toggle_pin: KeyBinding {
                key: "p".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "P".into(),
            },
            delete_entry: KeyBinding {
                key: "Delete".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "Delete".into(),
            },
            clear_escape: KeyBinding {
                key: "Escape".into(),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
                label: "Escape".into(),
            },
        }
    }
}

// ----------
// Privacy & Shortcut Tauri Commands
// Description: Query and mutate privacy controls, monitoring pause state, application exclusions, and keyboard shortcuts.
// ----------

#[tauri::command]
fn is_monitoring_paused() -> Result<bool, String> {
    Ok(clipboard::is_monitoring_paused())
}

#[tauri::command]
fn set_monitoring_paused(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    paused: bool,
) -> Result<(), String> {
    clipboard::set_monitoring_paused(paused);
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    store
        .set_setting("monitoring_paused", if paused { "true" } else { "false" })
        .map_err(|error| format!("could not save setting: {error}"))?;
    let _ = app.emit("clipboard://monitoring-paused-changed", paused);
    Ok(())
}

#[tauri::command]
fn get_privacy_settings(state: tauri::State<'_, AppState>) -> Result<PrivacySettings, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let monitoring_paused = clipboard::is_monitoring_paused();
    let ignore_password_managers = store
        .get_setting("ignore_password_managers")
        .ok()
        .flatten()
        .as_deref()
        != Some("false");
    let excluded_json = store
        .get_setting("excluded_applications")
        .ok()
        .flatten()
        .unwrap_or_else(|| "[]".into());
    let excluded_applications: Vec<String> =
        serde_json::from_str(&excluded_json).unwrap_or_default();
    let duplicate_handling = store
        .get_setting("duplicate_handling")
        .ok()
        .flatten()
        .unwrap_or_else(|| "bump".into());

    Ok(PrivacySettings {
        monitoring_paused,
        ignore_password_managers,
        excluded_applications,
        duplicate_handling,
    })
}

#[tauri::command]
fn set_privacy_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: PrivacySettings,
) -> Result<(), String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    clipboard::set_monitoring_paused(settings.monitoring_paused);
    clipboard::update_privacy_config(
        settings.ignore_password_managers,
        settings.excluded_applications.clone(),
    );
    clipboard::update_duplicate_handling(settings.duplicate_handling.clone());

    store
        .set_setting(
            "monitoring_paused",
            if settings.monitoring_paused {
                "true"
            } else {
                "false"
            },
        )
        .map_err(|e| format!("could not save monitoring_paused: {e}"))?;

    store
        .set_setting(
            "ignore_password_managers",
            if settings.ignore_password_managers {
                "true"
            } else {
                "false"
            },
        )
        .map_err(|e| format!("could not save ignore_password_managers: {e}"))?;

    let excluded_json = serde_json::to_string(&settings.excluded_applications)
        .map_err(|e| format!("could not serialize excluded applications: {e}"))?;
    store
        .set_setting("excluded_applications", &excluded_json)
        .map_err(|e| format!("could not save excluded_applications: {e}"))?;

    store
        .set_setting("duplicate_handling", &settings.duplicate_handling)
        .map_err(|e| format!("could not save duplicate_handling: {e}"))?;

    let _ = app.emit("clipboard://monitoring-paused-changed", settings.monitoring_paused);

    Ok(())
}

#[tauri::command]
fn get_shortcut_settings(state: tauri::State<'_, AppState>) -> Result<ShortcutSettings, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    if let Ok(Some(json)) = store.get_setting("keyboard_shortcuts") {
        if let Ok(settings) = serde_json::from_str::<ShortcutSettings>(&json) {
            return Ok(settings);
        }
    }

    Ok(ShortcutSettings::default())
}

#[tauri::command]
fn set_shortcut_settings(
    state: tauri::State<'_, AppState>,
    settings: ShortcutSettings,
) -> Result<(), String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("could not serialize shortcut settings: {e}"))?;

    store
        .set_setting("keyboard_shortcuts", &json)
        .map_err(|e| format!("could not save keyboard shortcuts: {e}"))
}

#[tauri::command]
fn reset_shortcut_settings(state: tauri::State<'_, AppState>) -> Result<ShortcutSettings, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let defaults = ShortcutSettings::default();
    let json = serde_json::to_string(&defaults)
        .map_err(|e| format!("could not serialize default shortcuts: {e}"))?;

    store
        .set_setting("keyboard_shortcuts", &json)
        .map_err(|e| format!("could not reset keyboard shortcuts: {e}"))?;

    Ok(defaults)
}

// ----------
// Native Clipboard Copy Command
// Description: Writes text to the OS clipboard directly using arboard, ensuring copy actions from Clipbox cards succeed without browser focus limitations.
// ----------

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    clipboard::mark_internal_copy_text(&text);
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

// ----------
// Native Image Clipboard Copy Command
// Description: Decodes a PNG image data URL and writes standard Windows CF_DIB (Format 8) and registered "PNG" format directly to the OS clipboard, enabling pasting into Paint, Discord, web browsers, and chat apps.
// ----------

#[tauri::command]
fn copy_image_to_clipboard(data_url: String) -> Result<(), String> {
    let base64_str = if let Some(idx) = data_url.find(',') {
        &data_url[idx + 1..]
    } else {
        &data_url
    };

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let png_bytes = STANDARD
        .decode(base64_str)
        .map_err(|e| format!("failed to decode base64 image data: {e}"))?;

    let sample_hash = image_clipboard::compute_bytes_hash(&png_bytes);
    clipboard::mark_internal_copy_image(sample_hash);

    image_clipboard::write_clipboard_image(&png_bytes)
}

// ----------
// Native File Clipboard Copy Command
// Description: Restores CF_HDROP file descriptors to the Windows clipboard so files can be pasted directly into File Explorer or Desktop.
// ----------

#[tauri::command]
fn copy_files_to_clipboard(paths: Vec<String>) -> Result<(), String> {
    if !paths.is_empty() {
        let items: Vec<file_clipboard::FileItem> = paths
            .iter()
            .map(|p| {
                let (size, is_directory) = match std::fs::metadata(p) {
                    Ok(m) => (m.len(), m.is_dir()),
                    Err(_) => (0, false),
                };
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.clone());
                file_clipboard::FileItem {
                    name,
                    path: p.clone(),
                    extension: String::new(),
                    size,
                    is_directory,
                }
            })
            .collect();
        let hash = file_clipboard::compute_files_hash(&items);
        clipboard::mark_internal_copy_files(hash);
    }

    file_clipboard::write_clipboard_files(&paths)
}

// ----------
// Native Image Save Dialog Command
// Description: Decodes PNG data URL and displays standard Windows Save As dialog with .png filter, writing image bytes directly to the chosen local disk path.
// ----------

#[tauri::command]
fn save_image_to_file(
    window: tauri::Window,
    data_url: String,
    default_filename: String,
) -> Result<Option<String>, String> {
    let base64_str = if let Some(idx) = data_url.find(',') {
        &data_url[idx + 1..]
    } else {
        &data_url
    };

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let png_bytes = STANDARD
        .decode(base64_str)
        .map_err(|e| format!("failed to decode base64 image data: {e}"))?;

    #[cfg(windows)]
    {
        use windows::core::{w, PWSTR};
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Controls::Dialogs::{
            GetSaveFileNameW, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
        };

        let hwnd = match window.hwnd() {
            Ok(h) => HWND(h.0 as _),
            Err(_) => HWND(std::ptr::null_mut()),
        };

        let mut file_buf = [0u16; 1024];
        let default_name_utf16: Vec<u16> = default_filename.encode_utf16().collect();
        let copy_len = default_name_utf16.len().min(file_buf.len() - 1);
        file_buf[..copy_len].copy_from_slice(&default_name_utf16[..copy_len]);

        let filter = w!("PNG Image (*.png)\0*.png\0All Files (*.*)\0*.*\0\0");

        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd,
            lpstrFilter: filter,
            lpstrFile: PWSTR(file_buf.as_mut_ptr()),
            nMaxFile: file_buf.len() as u32,
            lpstrDefExt: w!("png"),
            Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
            ..Default::default()
        };

        let success = unsafe { GetSaveFileNameW(&mut ofn) }.as_bool();
        if !success {
            return Ok(None);
        }

        let end = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
        let selected_path = String::from_utf16_lossy(&file_buf[..end]);

        std::fs::write(&selected_path, &png_bytes)
            .map_err(|e| format!("failed to save image to {selected_path}: {e}"))?;

        Ok(Some(selected_path))
    }

    #[cfg(not(windows))]
    {
        Err("Save dialog is currently supported on Windows".into())
    }
}

// ----------
// Save Edited Image Entry Command
// Description: Persists an annotated or cropped image as a new entry in Clipbox SQLite storage, inheriting origin metadata from the source image, and emits clipboard://new-entry to update the feed.
// ----------

#[tauri::command]
fn save_edited_image_entry(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    data_url: String,
    dimensions: String,
    source_entry_id: Option<i64>,
) -> Result<ClipboardEntry, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|e| format!("could not open database: {e}"))?;

    let metadata = if let Some(source_id) = source_entry_id {
        if let Ok(Some(source)) = store.get_entry(source_id) {
            clipbox_core::ClipboardMetadata {
                source_app: source.source_app.or(Some("Clipbox Editor".into())),
                source_process: source.source_process,
                window_title: Some("Annotated Image".into()),
                app_icon: source.app_icon,
                source_url: source.source_url,
            }
        } else {
            clipbox_core::ClipboardMetadata {
                source_app: Some("Clipbox Editor".into()),
                source_process: None,
                window_title: Some("Annotated Image".into()),
                app_icon: None,
                source_url: None,
            }
        }
    } else {
        clipbox_core::ClipboardMetadata {
            source_app: Some("Clipbox Editor".into()),
            source_process: None,
            window_title: Some("Annotated Image".into()),
            app_icon: None,
            source_url: None,
        }
    };

    let id = store
        .add_image_entry(&data_url, &dimensions, &metadata)
        .map_err(|e| format!("could not save image entry: {e}"))?;

    let entry = store
        .get_entry(id)
        .map_err(|e| format!("could not get saved entry: {e}"))?
        .ok_or_else(|| "entry not found after insert".to_string())?;

    let tauri_entry = ClipboardEntry::from(entry);
    let _ = app.emit("clipboard://new-entry", tauri_entry.clone());
    Ok(tauri_entry)
}

// ----------
// Application Restart Command
// Description: Restarts the Clipbox application process cleanly via Tauri AppHandle.
// ----------
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    app.restart();
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
            let pause_item = tauri::menu::MenuItem::with_id(app, "toggle_pause", "Pause / Resume Monitoring", true, None::<&str>)?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit_item = tauri::menu::MenuItem::with_id(app, "quit", "Quit Clipbox", true, None::<&str>)?;
            let tray_menu = tauri::menu::Menu::with_items(app, &[&show_item, &hide_item, &pause_item, &separator, &quit_item])?;

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
                    "toggle_pause" => {
                        let new_state = !clipboard::is_monitoring_paused();
                        clipboard::set_monitoring_paused(new_state);
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(store) = ClipboardStore::open(&state.database_path) {
                                let _ = store.set_setting("monitoring_paused", if new_state { "true" } else { "false" });
                            }
                        }
                        let _ = app.emit("clipboard://monitoring-paused-changed", new_state);
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

            // Initial window visibility and development connection error handling
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_min_size(Some(tauri::LogicalSize::new(900.0, 700.0)));
                if let Ok(size) = window.inner_size() {
                    if let Ok(scale_factor) = window.scale_factor() {
                        let logical_width = size.width as f64 / scale_factor;
                        let logical_height = size.height as f64 / scale_factor;
                        if logical_width < 900.0 || logical_height < 700.0 {
                            let new_w = logical_width.max(900.0);
                            let new_h = logical_height.max(700.0);
                            let _ = window.set_size(tauri::LogicalSize::new(new_w, new_h));
                        }
                    }
                }
                #[cfg(debug_assertions)]
                {
                    use std::net::{SocketAddr, TcpStream};
                    use std::time::Duration;

                    let dev_addr: SocketAddr = "127.0.0.1:1420".parse().unwrap();
                    let is_dev_running = TcpStream::connect_timeout(&dev_addr, Duration::from_millis(350)).is_ok();
                    if !is_dev_running {
                        use base64::engine::general_purpose::STANDARD;
                        use base64::Engine;
                        let html = include_str!("fallback_error.html");
                        let b64 = STANDARD.encode(html.as_bytes());
                        let data_url = format!("data:text/html;base64,{b64}");
                        if let Ok(url) = data_url.parse() {
                            let _ = window.navigate(url);
                        }
                    }
                }

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
            delete_entry,
            toggle_pinned,
            minimize_window,
            toggle_maximize_window,
            begin_window_drag,
            close_window,
            hide_window,
            show_window,
            exit_app,
            copy_to_clipboard,
            copy_image_to_clipboard,
            set_always_on_top,
            is_always_on_top,
            is_autostart_enabled,
            set_autostart,
            is_start_minimized,
            set_start_minimized,
            get_database_path,
            open_database_directory,
            get_retention_limit,
            set_retention_limit,
            restart_app,
            save_image_to_file,
            save_edited_image_entry,
            copy_files_to_clipboard,
            open_in_explorer,
            open_url,
            is_monitoring_paused,
            set_monitoring_paused,
            get_privacy_settings,
            set_privacy_settings,
            get_shortcut_settings,
            set_shortcut_settings,
            reset_shortcut_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Clipbox");
}
