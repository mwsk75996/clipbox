use std::path::PathBuf;

use clipbox_core::{
    deleted_retention_lifetime_seconds, ClipboardEntry as CoreClipboardEntry, ClipboardStore,
    DeletedEntry as CoreDeletedEntry,
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

mod autostart;
mod browser_url;
mod clipboard;
mod file_clipboard;
mod image_clipboard;
mod ocr;
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
    pub ocr_text: Option<String>,
    pub ocr_boxes: Option<String>,
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
            ocr_text: entry.ocr_text,
            ocr_boxes: entry.ocr_boxes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedClipboardEntry {
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
    pub deleted_at: i64,
}
impl From<CoreDeletedEntry> for DeletedClipboardEntry {
    fn from(entry: CoreDeletedEntry) -> Self {
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
            deleted_at: entry.deleted_at,
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
fn clear_entries(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let archived = store
        .clear_entries()
        .map_err(|error| format!("could not clear Clipbox entries: {error}"))?;
    refresh_tray_recent_clips(&app);
    Ok(archived)
}

// ----------
// Delete Single Entry Command
// Description: IPC command moving an individual clipboard record into the Recently Deleted archive by its unique ID. With the "immediately" retention the record is hard-deleted instead. Returns "archived", "deleted", or "missing" so the frontend can confirm accordingly.
// ----------

#[tauri::command]
fn delete_entry(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let outcome = if deleted_retention_setting(&store) == "immediately" {
        store
            .delete_entry(id)
            .map(|deleted| {
                if deleted {
                    "deleted".to_string()
                } else {
                    "missing".to_string()
                }
            })
            .map_err(|error| format!("could not delete Clipbox entry: {error}"))?
    } else {
        store
            .soft_delete_entry(id)
            .map(|archived| {
                if archived {
                    "archived".to_string()
                } else {
                    "missing".to_string()
                }
            })
            .map_err(|error| format!("could not archive Clipbox entry: {error}"))?
    };
    refresh_tray_recent_clips(&app);
    Ok(outcome)
}

// ----------
// Recently Deleted Archive Commands
// Description: IPC commands listing, restoring, permanently deleting, and purging archived clipboard records, plus the deleted-retention setting.
// ----------

/// Deleted-retention setting value, defaulting to a 7-day safety net.
fn deleted_retention_setting(store: &ClipboardStore) -> String {
    store
        .get_setting("deleted_retention")
        .unwrap_or_default()
        .unwrap_or_else(|| "7days".into())
}

/// Purge archived records older than the configured retention timespan.
/// Returns the number of purged rows (0 for "immediately", which never archives).
fn purge_expired_deleted_entries(store: &ClipboardStore) -> usize {
    let setting = deleted_retention_setting(store);
    let Some(lifetime) = deleted_retention_lifetime_seconds(&setting) else {
        return 0;
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    store
        .purge_deleted_entries_older_than(now.saturating_sub(lifetime as i64))
        .unwrap_or(0)
}

#[tauri::command]
fn list_deleted_entries(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DeletedClipboardEntry>, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .recent_deleted_entries(100)
        .map(|entries| {
            entries
                .into_iter()
                .map(DeletedClipboardEntry::from)
                .collect()
        })
        .map_err(|error| format!("could not read deleted Clipbox entries: {error}"))
}

#[tauri::command]
fn restore_deleted_entry(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Option<i64>, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let restored = store
        .restore_deleted_entry(id)
        .map_err(|error| format!("could not restore deleted Clipbox entry: {error}"))?;
    refresh_tray_recent_clips(&app);
    Ok(restored)
}

#[tauri::command]
fn delete_deleted_entry(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<bool, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let deleted = store
        .hard_delete_entry(id)
        .map_err(|error| format!("could not permanently delete Clipbox entry: {error}"))?;
    refresh_tray_recent_clips(&app);
    Ok(deleted)
}

#[tauri::command]
fn get_deleted_retention(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    Ok(deleted_retention_setting(&store))
}

#[tauri::command]
fn set_deleted_retention(
    state: tauri::State<'_, AppState>,
    retention: String,
) -> Result<usize, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    store
        .set_setting("deleted_retention", &retention)
        .map_err(|error| format!("could not save deleted retention setting: {error}"))?;

    // Switching to "immediately" purges the whole archive at once; shorter
    // timespans purge whatever is already past them.
    if retention == "immediately" {
        return store
            .purge_deleted_entries_older_than(i64::MAX)
            .map_err(|error| format!("could not purge deleted Clipbox entries: {error}"));
    }

    Ok(purge_expired_deleted_entries(&store))
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
fn start_dragging(window: tauri::Window) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN};

        if let Ok(raw_hwnd) = window.hwnd() {
            unsafe {
                let _ = ReleaseCapture();
                let hwnd = HWND(raw_hwnd.0 as _);
                SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
                return Ok(());
            }
        }
    }

    window.start_dragging().map_err(|error| error.to_string())
}

// ----------
// Window Visibility Lifecycle
// Description: Single state-aware show/restore/focus and hide-to-tray policy
// shared by the tray menu, tray click activation, and frontend commands.
// ----------

/// Restore a minimized window before making it visible, then focus it.
/// Unminimize runs first so a hidden+minimized window never flashes a
/// minimized intermediate state, then show, then focus.
fn restore_and_focus(window: &tauri::WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Hide only when the window is already visible, focused, and not minimized.
/// In every other state (hidden, minimized, or merely obscured) a tray
/// activation restores and focuses instead of hiding.
fn hides_on_tray_activation(window: &tauri::WebviewWindow) -> bool {
    window.is_visible().unwrap_or(false)
        && window.is_focused().unwrap_or(false)
        && !window.is_minimized().unwrap_or(false)
}

fn toggle_window_from_tray(window: &tauri::WebviewWindow) {
    if hides_on_tray_activation(window) {
        let _ = window.hide();
    } else {
        restore_and_focus(window);
    }
}

// ----------
// Tray Recent Clips
// Description: Up to five latest history entries surfaced directly in the tray
// menu for one-click copy-back. Fixed menu-item slots are refreshed in place;
// entry ids are tracked alongside so clicks resolve even after resequencing.
// ----------

/// Number of history entries quick-pasteable from the tray (menu height).
const TRAY_RECENT_COUNT: usize = 5;

struct TrayRecentClips {
    items: Vec<tauri::menu::MenuItem<tauri::Wry>>,
    entry_ids: std::sync::Mutex<Vec<i64>>,
}

fn tray_clip_label(content: &str) -> String {
    const MAX_CHARS: usize = 40;
    let first_line = content.lines().next().unwrap_or("").trim();
    let truncated: String = first_line.chars().take(MAX_CHARS).collect();
    if truncated.is_empty() {
        "(empty clip)".to_string()
    } else if first_line.chars().count() > MAX_CHARS {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Re-read the newest history entries into the fixed tray slots.
/// Called on startup, on every capture, and after history mutations.
pub(crate) fn refresh_tray_recent_clips(app: &tauri::AppHandle) {
    let db_path = match app.try_state::<AppState>() {
        Some(state) => state.database_path.clone(),
        None => return,
    };
    let entries = ClipboardStore::open(&db_path)
        .map(|store| {
            store
                .recent_entries(TRAY_RECENT_COUNT as u32)
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let Some(tray) = app.try_state::<TrayRecentClips>() else {
        return;
    };
    let mut ids = match tray.entry_ids.lock() {
        Ok(ids) => ids,
        Err(_) => return,
    };
    ids.clear();
    for (slot, item) in tray.items.iter().enumerate() {
        match entries.get(slot) {
            Some(entry) => {
                ids.push(entry.id);
                let _ = item.set_text(tray_clip_label(&entry.content));
                let _ = item.set_enabled(true);
            }
            None => {
                let _ = item.set_text("—");
                let _ = item.set_enabled(false);
            }
        }
    }
}

fn paste_text_to_clipboard(content: &str) -> Result<(), String> {
    clipboard::mark_internal_copy_text(content);
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_text(content.to_owned())
        .map_err(|e| e.to_string())
}

fn decode_image_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let base64_str = if let Some(idx) = data_url.find(',') {
        &data_url[idx + 1..]
    } else {
        data_url
    };

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD
        .decode(base64_str)
        .map_err(|e| format!("failed to decode base64 image data: {e}"))
}

fn paste_image_data_url(data_url: &str) -> Result<(), String> {
    let png_bytes = decode_image_data_url(data_url)?;
    let sample_hash = image_clipboard::compute_bytes_hash(&png_bytes);
    clipboard::mark_internal_copy_image(sample_hash);
    image_clipboard::write_clipboard_image(&png_bytes)
}

fn copy_files_by_paths(paths: &[String]) -> Result<(), String> {
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

    file_clipboard::write_clipboard_files(paths)
}

fn paste_files_json(files_json: &str) -> Result<(), String> {
    let paths: Vec<String> = serde_json::from_str::<serde_json::Value>(files_json)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.get("path")?.as_str().map(str::to_string))
        .collect();
    if paths.is_empty() {
        return Err("no file paths in archived record".into());
    }
    copy_files_by_paths(&paths)
}

/// Copy a tray slot's entry back to the OS clipboard. Marks the write as
/// internal first so the monitor bumps the entry instead of duplicating it.
/// A vanished entry refreshes the menu instead of failing loudly.
fn paste_tray_clip(app: &tauri::AppHandle, slot: usize) {
    let entry_id = {
        let tray = match app.try_state::<TrayRecentClips>() {
            Some(tray) => tray,
            None => return,
        };
        let ids = match tray.entry_ids.lock() {
            Ok(ids) => ids,
            Err(_) => return,
        };
        ids.get(slot).copied()
    };
    let Some(entry_id) = entry_id else {
        return;
    };

    let db_path = match app.try_state::<AppState>() {
        Some(state) => state.database_path.clone(),
        None => return,
    };
    let entry = match ClipboardStore::open(&db_path)
        .ok()
        .and_then(|store| store.get_entry(entry_id).ok().flatten())
    {
        Some(entry) => entry,
        None => {
            refresh_tray_recent_clips(app);
            return;
        }
    };

    let result = if entry.entry_type == "image" {
        match entry.image_data.as_deref() {
            Some(data_url) => paste_image_data_url(data_url),
            None => return,
        }
    } else if entry.entry_type == "file" {
        match entry.files_data.as_deref() {
            Some(files_json) => paste_files_json(files_json),
            None => return,
        }
    } else {
        paste_text_to_clipboard(&entry.content)
    };

    if result.is_err() {
        refresh_tray_recent_clips(app);
    }
}

#[tauri::command]
fn hide_window(window: tauri::Window) -> Result<(), String> {
    // Hide to system tray instead of terminating the app
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn show_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        restore_and_focus(&window);
    }
    Ok(())
}

// ----------
// Close Behavior Setting Commands
// Description: Queries and updates what the titlebar close button and native close requests do: ask every time, hide to tray, or quit the app.
// ----------

/// Stored close-button policy, defaulting to asking every time.
fn close_behavior_setting(store: &ClipboardStore) -> String {
    store
        .get_setting("close_behavior")
        .unwrap_or_default()
        .unwrap_or_else(|| "ask".into())
}

#[tauri::command]
fn get_close_behavior(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    Ok(close_behavior_setting(&store))
}

#[tauri::command]
fn set_close_behavior(state: tauri::State<'_, AppState>, behavior: String) -> Result<(), String> {
    if !matches!(behavior.as_str(), "ask" | "hide" | "quit") {
        return Err(format!("invalid close behavior: {behavior}"));
    }
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    store
        .set_setting("close_behavior", &behavior)
        .map_err(|error| format!("could not save close behavior setting: {error}"))
}

// ----------
// Entries Per Page Setting Commands
// Description: Queries and updates how many history entries render per feed page.
// ----------

const VALID_ENTRIES_PER_PAGE: [&str; 4] = ["10", "25", "50", "100"];

#[tauri::command]
fn get_entries_per_page(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    Ok(store
        .get_setting("entries_per_page")
        .unwrap_or_default()
        .unwrap_or_else(|| "25".into()))
}

#[tauri::command]
fn set_entries_per_page(state: tauri::State<'_, AppState>, per_page: String) -> Result<(), String> {
    if !VALID_ENTRIES_PER_PAGE.contains(&per_page.as_str()) {
        return Err(format!("invalid entries per page: {per_page}"));
    }
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    store
        .set_setting("entries_per_page", &per_page)
        .map_err(|error| format!("could not save entries per page setting: {error}"))
}

// ----------
// Image OCR Status Commands
// Description: Reports on-device screenshot text-recognition availability and opens Windows language settings to install recognizer support.
// ----------

#[derive(serde::Serialize)]
struct OcrStatus {
    available: bool,
    language: String,
    languages: Vec<String>,
    selected: String,
}

#[tauri::command]
fn get_ocr_status(state: tauri::State<'_, AppState>) -> Result<OcrStatus, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    let stored = store
        .get_setting("ocr_language")
        .unwrap_or_default()
        .unwrap_or_else(|| "auto".into());
    // Normalize unknown values so the pill never describes a phantom engine.
    let languages = ocr::available_languages();
    let selected = if stored == "auto" || languages.iter().any(|tag| tag == &stored) {
        stored
    } else {
        "auto".into()
    };
    let (available, language) = ocr::engine_status_for(&selected);
    Ok(OcrStatus {
        available,
        language,
        languages,
        selected,
    })
}

#[tauri::command]
fn set_ocr_language(state: tauri::State<'_, AppState>, language: String) -> Result<(), String> {
    if language.trim().is_empty() {
        return Err("empty OCR language".into());
    }
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;
    store
        .set_setting("ocr_language", &language)
        .map_err(|error| format!("could not save OCR language setting: {error}"))
}

/// English OCR capability for DISM Features on Demand.
#[cfg(windows)]
const ENGLISH_OCR_CAPABILITY: &str = "Language.OCR~~~en-US~0.0.1.0";

#[tauri::command]
async fn install_english_ocr() -> Result<bool, String> {
    #[cfg(windows)]
    {
        if ocr::available_languages()
            .iter()
            .any(|tag| tag.starts_with("en"))
        {
            return Ok(true);
        }

        // Elevated DISM install (shows a UAC prompt); declining surfaces as a
        // launch failure below, and everything blocking runs off-executor.
        tauri::async_runtime::spawn_blocking(|| {
            use std::os::windows::process::CommandExt;
            let installed = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!(
                        "Start-Process -FilePath \"$env:SystemRoot\\System32\\Dism.exe\" -ArgumentList @('/Online', '/NoRestart', '/Add-Capability', '/CapabilityName:{ENGLISH_OCR_CAPABILITY}') -Verb RunAs -Wait"
                    ),
                ])
                .creation_flags(0x08000000)
                .output()
                .map_err(|error| format!("could not launch capability installer: {error}"))?;
            if !installed.status.success() {
                let detail = String::from_utf8_lossy(&installed.stderr);
                return Err(format!("capability installer failed: {}", detail.trim()));
            }
            // Trust re-detection, not exit codes: poll for the recognizer.
            for _ in 0..60 {
                if ocr::available_languages()
                    .iter()
                    .any(|tag| tag.starts_with("en"))
                {
                    return Ok(true);
                }
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
            Ok(false)
        })
        .await
        .map_err(|error| format!("installer task failed: {error}"))?
    }

    #[cfg(not(windows))]
    {
        Err("OCR language install is currently supported on Windows".into())
    }
}

#[tauri::command]
fn open_language_settings() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::core::w;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

        let result =
            unsafe { ShellExecuteW(None, w!("open"), w!("ms-settings:regionlanguage"), None, None, SW_SHOW) };
        if result.0 as usize > 32 {
            Ok(())
        } else {
            Err("could not open Windows language settings".into())
        }
    }

    #[cfg(not(windows))]
    {
        Err("language settings shortcut is currently supported on Windows".into())
    }
}

// ----------
// Self-Update Commands
// Description: Version query plus GitHub Releases download/install flow: stream the setup asset to temp with progress events, then silent-install and relaunch.
// ----------

#[tauri::command]
fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
async fn download_update(
    app: tauri::AppHandle,
    url: String,
    filename: String,
) -> Result<String, String> {
    const TRUSTED_PREFIXES: [&str; 2] = [
        "https://github.com/mwsk75996/clipbox/releases/download/",
        "https://objects.githubusercontent.com/",
    ];
    if !TRUSTED_PREFIXES
        .iter()
        .any(|prefix| url.starts_with(prefix))
    {
        return Err("refusing to download from an untrusted url".into());
    }

    let safe_name: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    if safe_name.is_empty() || !safe_name.to_lowercase().ends_with(".exe") {
        return Err("refusing to download a non-installer file".into());
    }
    let dest = std::env::temp_dir().join(safe_name);

    let mut response = reqwest::get(&url)
        .await
        .map_err(|error| format!("update download failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("update download failed: {error}"))?;
    let total = response.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&dest)
        .map_err(|error| format!("could not stage update: {error}"))?;
    let mut downloaded: u64 = 0;
    loop {
        match response
            .chunk()
            .await
            .map_err(|error| format!("update download failed: {error}"))?
        {
            Some(bytes) => {
                use std::io::Write;
                file.write_all(&bytes)
                    .map_err(|error| format!("could not stage update: {error}"))?;
                downloaded += bytes.len() as u64;
                let _ = app.emit(
                    "update://download-progress",
                    serde_json::json!({ "downloaded": downloaded, "total": total }),
                );
            }
            None => break,
        }
    }

    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn install_update(app: tauri::AppHandle, path: String) -> Result<(), String> {
    // Only ever run what download_update staged: an .exe directly in temp.
    let candidate = std::path::PathBuf::from(&path);
    if candidate.extension().and_then(|ext| ext.to_str()) != Some("exe")
        || candidate.parent() != Some(std::env::temp_dir().as_path())
    {
        return Err("refusing to run updater outside the temp directory".into());
    }

    #[cfg(windows)]
    {
        // Relaunch target is the running exe (the installed location in production).
        let current = std::env::current_exe()
            .map_err(|error| format!("could not locate running executable: {error}"))?;
        // Silent-install, then reopen the fresh build. Our own exit lands
        // first; the installer call blocks, so no timeout guessing is needed.
        // Hidden console: no window may flash for this background handoff.
        // The script travels base64-encoded: unlike cmd argv strings, the
        // encoding alphabet has no spaces or quotes for any parser to mangle.
        let script = updater_powershell_script(
            &path,
            &current.display().to_string(),
            std::process::id(),
        );
        let encoded: String = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(
                script
                    .encode_utf16()
                    .flat_map(|unit| unit.to_le_bytes())
                    .collect::<Vec<u8>>(),
            )
        };
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-EncodedCommand",
                &encoded,
            ])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|error| format!("could not launch updater: {error}"))?;
        app.exit(0);
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (app, path);
        Err("self-update is currently supported on Windows".into())
    }
}

/// Self-update handoff script: wait until no other Clipbox process holds
/// files (our own exit lands first), silent-install, reopen the fresh build,
/// remove the staged installer. Every step logs to clipbox-update.log so a
/// failed handoff is diagnosable instead of silent. Single quotes delimit
/// paths (PowerShell-safe); the caller transports this base64-encoded so no
/// shell layer ever parses it.
fn updater_powershell_script(installer: &str, target: &str, own_pid: u32) -> String {
    let quote = |path: &str| path.replace('\'', "''");
    format!(
        "$log = Join-Path $env:TEMP 'clipbox-update.log'; \
         'handoff start' | Out-File $log; \
         $tries = 0; \
         while ((Get-Process -Name clipbox -ErrorAction SilentlyContinue | Where-Object {{ $_.Id -ne {own_pid} }}) -and ($tries -lt 60)) {{ Start-Sleep 1; $tries++ }}; \
         'waited for other instances' | Out-File $log -Append; \
         & '{installer}' /S; \
         'installer exit code: ' + $LASTEXITCODE | Out-File $log -Append; \
         Start-Process '{target}'; \
         'relaunched' | Out-File $log -Append; \
         Remove-Item '{installer}' -ErrorAction SilentlyContinue",
        installer = quote(installer),
        target = quote(target),
        own_pid = own_pid
    )
}

/// UTF-16LE base64 encoding PowerShell requires for -EncodedCommand.
fn encode_powershell_script(script: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect::<Vec<u8>>(),
    )
}

#[cfg(test)]
mod updater_script_tests {
    use super::updater_powershell_script;

    #[test]
    fn script_quotes_paths_without_shell_metachars() {
        let script = updater_powershell_script(
            r"C:\Temp\Clipbox_1.1.1_x64-setup.exe",
            r"C:\Users\matti\AppData\Local\Clipbox\clipbox.exe",
            1234,
        );
        assert!(script.contains(r"'C:\Temp\Clipbox_1.1.1_x64-setup.exe' /S"));
        assert!(script.contains("1234"));
        assert!(script.contains("clipbox-update.log"));
    }

    #[test]
    fn script_escapes_embedded_quotes() {
        let script = updater_powershell_script(
            r"C:\Temp\O'Brien\setup.exe",
            r"C:\Temp\app.exe",
            1234,
        );
        assert!(script.contains(r"'C:\Temp\O''Brien\setup.exe'"));
        assert!(!script.contains('\"'));
    }
    #[test]
    #[cfg(windows)]
    fn encoded_command_survives_process_spawn() {
        use super::encode_powershell_script;

        // Space-containing path: exactly what broke the cmd/argv handoff.
        let marker = std::env::temp_dir().join("clipbox updater transport marker.txt");
        let _ = std::fs::remove_file(&marker);
        let script = format!(
            "New-Item '{}' -ItemType File -Force",
            marker.display().to_string().replace('\'', "''")
        );
        let encoded = encode_powershell_script(&script);

        use std::os::windows::process::CommandExt;
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-EncodedCommand",
                &encoded,
            ])
            .creation_flags(0x08000000)
            .status()
            .expect("powershell should launch");
        assert!(status.success());
        assert!(
            marker.is_file(),
            "encoded script with a space-containing path must execute"
        );
        let _ = std::fs::remove_file(&marker);
    }
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
fn set_retention_limit(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    limit: String,
) -> Result<usize, String> {
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

    refresh_tray_recent_clips(&app);
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
    // Serde-defaulted (not merely Default-impl'd) so stored settings from
    // before this field existed still deserialize instead of resetting all
    // rebinds to defaults.
    #[serde(default = "default_toggle_window")]
    pub toggle_window: KeyBinding,
}

fn default_toggle_window() -> KeyBinding {
    // Unbound by default: the user opts into a global hotkey explicitly.
    KeyBinding {
        key: "".into(),
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
        label: "Not bound".into(),
    }
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
            toggle_window: default_toggle_window(),
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
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: ShortcutSettings,
) -> Result<(), String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("could not serialize shortcut settings: {e}"))?;

    store
        .set_setting("keyboard_shortcuts", &json)
        .map_err(|e| format!("could not save keyboard shortcuts: {e}"))?;

    // Rebinds (including the global toggle) apply without a restart.
    refresh_global_toggle_shortcut(&app, &state.database_path);
    Ok(())
}

#[tauri::command]
fn reset_shortcut_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ShortcutSettings, String> {
    let store = ClipboardStore::open(&state.database_path)
        .map_err(|error| format!("could not open Clipbox database: {error}"))?;

    let defaults = ShortcutSettings::default();
    let json = serde_json::to_string(&defaults)
        .map_err(|e| format!("could not serialize default shortcuts: {e}"))?;

    store
        .set_setting("keyboard_shortcuts", &json)
        .map_err(|e| format!("could not reset keyboard shortcuts: {e}"))?;

    refresh_global_toggle_shortcut(&app, &state.database_path);
    Ok(defaults)
}

// ----------
// Global Window Toggle Hotkey
// Description: System-wide hotkey showing/hiding the main window, driven by the stored toggle_window binding and handled through the tray toggle lifecycle.
// ----------

/// Build a global-shortcut accelerator string (e.g. "Alt+Shift+V") from a
/// stored key binding. Returns None for empty keys, bare keys without any
/// modifier (registering those would swallow normal typing system-wide),
/// and unparseable input.
fn toggle_window_accelerator(binding: &serde_json::Value) -> Option<String> {
    let key = binding.get("key")?.as_str()?;
    if key.is_empty() {
        return None;
    }

    let flag = |name: &str| binding.get(name).and_then(|v| v.as_bool()).unwrap_or(false);
    let mut parts = Vec::new();
    if flag("ctrl") {
        parts.push("CommandOrControl".to_string());
    }
    if flag("alt") {
        parts.push("Alt".to_string());
    }
    if flag("shift") {
        parts.push("Shift".to_string());
    }
    if flag("meta") {
        parts.push("Super".to_string());
    }
    if parts.is_empty() {
        return None;
    }

    let main = match key {
        " " => "Space".to_string(),
        "ArrowDown" => "Down".to_string(),
        "ArrowUp" => "Up".to_string(),
        "ArrowLeft" => "Left".to_string(),
        "ArrowRight" => "Right".to_string(),
        single if single.chars().count() == 1 => single.to_uppercase(),
        named => named.to_string(),
    };
    parts.push(main);
    Some(parts.join("+"))
}

/// (Re)register the global window-toggle hotkey from stored settings.
/// Only this shortcut is ever registered, so resetting first is correct.
/// A taken binding (another app owns it) warns and keeps running.
fn refresh_global_toggle_shortcut(app: &tauri::AppHandle, database_path: &std::path::Path) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let _ = app.global_shortcut().unregister_all();

    let binding = ClipboardStore::open(database_path)
        .ok()
        .and_then(|store| store.get_setting("keyboard_shortcuts").ok().flatten())
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|settings| settings.get("toggle_window").cloned());
    let Some(binding) = binding else {
        return;
    };
    if binding
        .get("key")
        .and_then(|key| key.as_str())
        .unwrap_or("")
        .is_empty()
    {
        // Unbound by choice: stay silent.
        return;
    }

    match toggle_window_accelerator(&binding) {
        Some(accelerator) => {
            if let Err(error) = app.global_shortcut().register(accelerator.as_str()) {
                eprintln!("could not register global window toggle (is another app using it?): {error}");
            }
        }
        None => eprintln!("no usable global window toggle binding configured"),
    }
}

#[cfg(test)]
mod global_shortcut_tests {
    use super::toggle_window_accelerator;
    use serde_json::json;

    #[test]
    fn builds_alt_shift_v_accelerator() {
        let binding = json!({ "key": "v", "ctrl": false, "shift": true, "alt": true, "meta": false });
        assert_eq!(
            toggle_window_accelerator(&binding).as_deref(),
            Some("Alt+Shift+V")
        );
    }

    #[test]
    fn uppercases_single_keys_and_maps_space() {
        let binding = json!({ "key": "g", "ctrl": true, "shift": false, "alt": false, "meta": false });
        assert_eq!(
            toggle_window_accelerator(&binding).as_deref(),
            Some("CommandOrControl+G")
        );
        let binding = json!({ "key": " ", "ctrl": true, "shift": false, "alt": false, "meta": false });
        assert_eq!(
            toggle_window_accelerator(&binding).as_deref(),
            Some("CommandOrControl+Space")
        );
    }

    #[test]
    fn refuses_bare_keys_and_empty_bindings() {
        let binding = json!({ "key": "v", "ctrl": false, "shift": false, "alt": false, "meta": false });
        assert_eq!(toggle_window_accelerator(&binding), None);
        let binding = json!({ "key": "", "ctrl": true, "shift": false, "alt": false, "meta": false });
        assert_eq!(toggle_window_accelerator(&binding), None);
    }

    #[test]
    fn unbound_by_default() {
        assert!(super::default_toggle_window().key.is_empty());
    }
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
    paste_image_data_url(&data_url)
}

// ----------
// Native File Clipboard Copy Command
// Description: Restores CF_HDROP file descriptors to the Windows clipboard so files can be pasted directly into File Explorer or Desktop.
// ----------

#[tauri::command]
fn copy_files_to_clipboard(paths: Vec<String>) -> Result<(), String> {
    copy_files_by_paths(&paths)
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
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state == ShortcutState::Pressed {
                        if let Some(window) = app.get_webview_window("main") {
                            toggle_window_from_tray(&window);
                        }
                    }
                })
                .build(),
        )
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

            // Fixed quick-paste slots, populated below and refreshed in place.
            let mut paste_items = Vec::with_capacity(TRAY_RECENT_COUNT);
            for slot in 0..TRAY_RECENT_COUNT {
                paste_items.push(tauri::menu::MenuItem::with_id(
                    app,
                    format!("paste-{slot}"),
                    "—",
                    false,
                    None::<&str>,
                )?);
            }
            app.manage(TrayRecentClips {
                items: paste_items.clone(),
                entry_ids: std::sync::Mutex::new(Vec::new()),
            });
            let paste_separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let tray_menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &paste_items[0],
                    &paste_items[1],
                    &paste_items[2],
                    &paste_items[3],
                    &paste_items[4],
                    &paste_separator,
                    &show_item,
                    &hide_item,
                    &pause_item,
                    &separator,
                    &quit_item,
                ],
            )?;
            let app_handle = app.handle();
            refresh_tray_recent_clips(app_handle);

            let mut tray_builder = tauri::tray::TrayIconBuilder::new()
                .tooltip("Clipbox")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            restore_and_focus(&window);
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
                    id if id.starts_with("paste-") => {
                        if let Ok(slot) = id["paste-".len()..].parse::<usize>() {
                            paste_tray_clip(app, slot);
                        }
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
                            toggle_window_from_tray(&window);
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

            // Purge expired Recently Deleted records on startup, then hourly
            // while the app keeps running from the tray.
            if let Ok(store) = ClipboardStore::open(&database_path) {
                let _ = purge_expired_deleted_entries(&store);
            }
            {
                let database_path = database_path.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(3_600));
                    if let Ok(store) = ClipboardStore::open(&database_path) {
                        let _ = purge_expired_deleted_entries(&store);
                    }
                });
            }

            // Register the global window-toggle hotkey from stored settings.
            let app_handle = app.handle();
            refresh_global_toggle_shortcut(app_handle, &database_path);

            // Scan pre-existing unscanned screenshots in the background.
            ocr::backfill_missing(database_path.clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Close-button policy also governs native close requests (Alt+F4):
                // "hide" keeps the established hide-to-tray behavior, "quit"
                // lets the request through so the process terminates, and "ask"
                // defers to the frontend prompt. Unreadable settings fail safe
                // to hide, never to quit.
                let app = window.app_handle();
                let behavior = ClipboardStore::open(&app.state::<AppState>().database_path)
                    .map(|store| close_behavior_setting(&store))
                    .unwrap_or_else(|_| "hide".into());

                if behavior == "quit" {
                    return;
                }
                api.prevent_close();
                if behavior == "ask" {
                    let _ = app.emit("window://close-requested", ());
                } else {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_entries,
            clear_entries,
            delete_entry,
            list_deleted_entries,
            restore_deleted_entry,
            delete_deleted_entry,
            get_deleted_retention,
            set_deleted_retention,
            toggle_pinned,
            minimize_window,
            start_dragging,
            hide_window,
            show_window,
            get_close_behavior,
            set_close_behavior,
            get_entries_per_page,
            set_entries_per_page,
            get_ocr_status,
            set_ocr_language,
            install_english_ocr,
            open_language_settings,
            get_app_version,
            download_update,
            install_update,
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
