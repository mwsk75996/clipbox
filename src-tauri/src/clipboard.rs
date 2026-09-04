// ----------
// Clipboard Monitoring Service
// Description: Event-driven Windows clipboard monitor utilizing AddClipboardFormatListener (WM_CLIPBOARDUPDATE) with polling fallback, eliminating lock contention on Snipping Tool / PrintScreen while capturing files (CF_HDROP), text, and multi-format images.
// ----------

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use clipbox_core::ClipboardStore;
use tauri::Emitter;

use crate::file_clipboard;
use crate::image_clipboard;
use crate::source;

static LAST_INTERNAL_COPIED_TEXT: Mutex<Option<String>> = Mutex::new(None);
static LAST_INTERNAL_COPIED_IMAGE_HASH: AtomicU64 = AtomicU64::new(0);
static LAST_INTERNAL_COPIED_FILES_HASH: AtomicU64 = AtomicU64::new(0);

static MONITORING_PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static IGNORE_PASSWORD_MANAGERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static EXCLUDED_APPLICATIONS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DUPLICATE_HANDLING: Mutex<String> = Mutex::new(String::new());

// ----------
// Privacy Controls & Monitoring State
// Description: Manages runtime pause/resume state, application exclusions, password manager ignoring, and duplicate entry handling policies.
// ----------

pub fn is_monitoring_paused() -> bool {
    MONITORING_PAUSED.load(Ordering::SeqCst)
}

pub fn set_monitoring_paused(paused: bool) {
    MONITORING_PAUSED.store(paused, Ordering::SeqCst);
}

pub fn update_privacy_config(ignore_password_managers: bool, excluded_apps: Vec<String>) {
    IGNORE_PASSWORD_MANAGERS.store(ignore_password_managers, Ordering::SeqCst);
    if let Ok(mut guard) = EXCLUDED_APPLICATIONS.lock() {
        *guard = excluded_apps;
    }
}

pub fn update_duplicate_handling(mode: String) {
    if let Ok(mut guard) = DUPLICATE_HANDLING.lock() {
        *guard = mode;
    }
}

pub fn is_excluded_process(process_name: Option<&str>) -> bool {
    let Some(proc) = process_name else {
        return false;
    };

    let lower = proc.to_ascii_lowercase();

    // 1. Password managers exclusion
    if IGNORE_PASSWORD_MANAGERS.load(Ordering::SeqCst) {
        let known_pws = [
            "1password.exe",
            "bitwarden.exe",
            "keepass.exe",
            "keepassxc.exe",
            "lastpass.exe",
            "enpass.exe",
            "dashlane.exe",
            "roboform.exe",
            "nordpass.exe",
        ];
        if known_pws.iter().any(|&p| lower == p || lower.ends_with(&format!("\\{p}"))) {
            return true;
        }
    }

    // 2. User-configured excluded applications
    if let Ok(guard) = EXCLUDED_APPLICATIONS.lock() {
        for excluded in guard.iter() {
            let ex_lower = excluded.trim().to_ascii_lowercase();
            if !ex_lower.is_empty()
                && (lower == ex_lower
                    || lower.ends_with(&format!("\\{ex_lower}"))
                    || lower.starts_with(&ex_lower))
            {
                return true;
            }
        }
    }

    false
}

// ----------
// Sensitive Clipboard Content Check
// Description: Detects standard Windows clipboard privacy formats (ExcludeClipboardContentFromMonitorProcessing, Clipboard Viewer Ignore) used by security apps and password managers.
// ----------
#[cfg(windows)]
pub fn is_sensitive_clipboard_content() -> bool {
    use windows::core::w;
    use windows::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };

    unsafe {
        let fmt_exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if fmt_exclude != 0 && IsClipboardFormatAvailable(fmt_exclude).is_ok() {
            return true;
        }

        let fmt_ignore = RegisterClipboardFormatW(w!("Clipboard Viewer Ignore"));
        if fmt_ignore != 0 && IsClipboardFormatAvailable(fmt_ignore).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(not(windows))]
pub fn is_sensitive_clipboard_content() -> bool {
    false
}

// ----------
// Internal Clipboard Copy Tracking
// Description: Records content copied to the OS clipboard by Clipbox user actions (e.g. clicking the Copy button on an entry card) so the background monitor does not create duplicate entries in history, while ensuring external screenshots, files, or copies taken while Clipbox is focused are always captured.
// ----------

pub fn mark_internal_copy_text(text: &str) {
    let cleaned = clipbox_core::strip_leading_empty_lines(text).to_string();
    if let Ok(mut guard) = LAST_INTERNAL_COPIED_TEXT.lock() {
        *guard = Some(cleaned);
    }
}

pub fn mark_internal_copy_image(hash: u64) {
    LAST_INTERNAL_COPIED_IMAGE_HASH.store(hash, Ordering::SeqCst);
}

pub fn mark_internal_copy_files(hash: u64) {
    LAST_INTERNAL_COPIED_FILES_HASH.store(hash, Ordering::SeqCst);
}

/// Start the background clipboard monitor.
pub fn start(database_path: PathBuf, app: tauri::AppHandle) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("clipbox-clipboard-monitor".into())
        .spawn(move || monitor(database_path, app))
        .expect("failed to start clipboard monitor")
}

fn monitor(database_path: PathBuf, app: tauri::AppHandle) {
    // Load persisted privacy and reliability settings from database on startup
    if let Ok(store) = ClipboardStore::open(&database_path) {
        let paused = store.get_setting("monitoring_paused").ok().flatten().as_deref() == Some("true");
        set_monitoring_paused(paused);

        let ignore_pw = store.get_setting("ignore_password_managers").ok().flatten().as_deref() != Some("false");
        let excluded_json = store.get_setting("excluded_applications").ok().flatten().unwrap_or_else(|| "[]".into());
        let excluded_apps: Vec<String> = serde_json::from_str(&excluded_json).unwrap_or_default();
        update_privacy_config(ignore_pw, excluded_apps);

        let dup_mode = store.get_setting("duplicate_handling").ok().flatten().unwrap_or_else(|| "bump".into());
        update_duplicate_handling(dup_mode);
    }

    #[cfg(windows)]
    {
        monitor_windows(database_path, app);
    }

    #[cfg(not(windows))]
    {
        monitor_polling(database_path, app);
    }
}

// ----------
// Native Windows Event-Driven Clipboard Listener
// Description: Registers a message-only window with AddClipboardFormatListener to receive WM_CLIPBOARDUPDATE events. Eliminates polling, eliminates OpenClipboard lock contention with Snipping Tool, and reduces idle CPU usage to 0%.
// ----------

#[cfg(windows)]
fn monitor_windows(database_path: PathBuf, app: tauri::AppHandle) {
    use windows::core::w;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::{
        AddClipboardFormatListener, RemoveClipboardFormatListener,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostQuitMessage, RegisterClassW, TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE,
        WINDOW_STYLE, WM_CLIPBOARDUPDATE, WM_DESTROY, WNDCLASSW,
    };

    let store = match ClipboardStore::open(&database_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("could not open Clipbox database: {error}");
            return;
        }
    };

    let mut previous_files_hash: Option<u64> = file_clipboard::read_clipboard_files().map(|f| f.hash);
    let mut previous_text: Option<String> = None;
    let mut previous_image_hash: Option<u64> = None;

    // Read baseline so existing clipboard contents aren't recorded just because Clipbox started
    if previous_files_hash.is_none() {
        if let Some(img) = image_clipboard::read_clipboard_image() {
            previous_image_hash = Some(img.raw_bytes_sample_hash);
        } else if let Ok(mut cb) = Clipboard::new() {
            previous_text = cb
                .get_text()
                .ok()
                .map(|t| clipbox_core::strip_leading_empty_lines(&t).to_string());
        }
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_DESTROY => {
                let _ = RemoveClipboardFormatListener(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe {
        let hmodule = GetModuleHandleW(None).unwrap_or_default();
        let hinstance = HINSTANCE(hmodule.0);
        let class_name = w!("ClipboxClipboardListener");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };

        let _ = RegisterClassW(&wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("ClipboxClipboardListenerWindow"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("could not create clipboard listener window: {e}, falling back to polling");
                return monitor_polling(database_path, app);
            }
        };

        if let Err(e) = AddClipboardFormatListener(hwnd) {
            eprintln!("could not register clipboard format listener: {e}, falling back to polling");
            let _ = DestroyWindow(hwnd);
            return monitor_polling(database_path, app);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_CLIPBOARDUPDATE {
                // Sleep briefly (25ms) so the source application (e.g. Snipping Tool or File Explorer)
                // has fully completed writing its payload and closed the clipboard handle.
                thread::sleep(Duration::from_millis(25));
                check_clipboard(
                    &store,
                    &app,
                    &database_path,
                    &mut previous_files_hash,
                    &mut previous_text,
                    &mut previous_image_hash,
                );
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = RemoveClipboardFormatListener(hwnd);
        let _ = DestroyWindow(hwnd);
    }
}

// ----------
// Polling Clipboard Fallback Monitor
// Description: Used as fallback or on non-Windows platforms with a gentle 300ms polling interval.
// ----------

fn monitor_polling(database_path: PathBuf, app: tauri::AppHandle) {
    let store = match ClipboardStore::open(&database_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("could not open Clipbox database: {error}");
            return;
        }
    };

    let mut previous_files_hash: Option<u64> = file_clipboard::read_clipboard_files().map(|f| f.hash);
    let mut previous_text: Option<String> = None;
    let mut previous_image_hash: Option<u64> = None;

    if previous_files_hash.is_none() {
        if let Some(img) = image_clipboard::read_clipboard_image() {
            previous_image_hash = Some(img.raw_bytes_sample_hash);
        } else if let Ok(mut cb) = Clipboard::new() {
            previous_text = cb
                .get_text()
                .ok()
                .map(|t| clipbox_core::strip_leading_empty_lines(&t).to_string());
        }
    }

    loop {
        check_clipboard(
            &store,
            &app,
            &database_path,
            &mut previous_files_hash,
            &mut previous_text,
            &mut previous_image_hash,
        );
        thread::sleep(Duration::from_millis(300));
    }
}

// Helper with exponential backoff for transient clipboard locking
fn get_clipboard_text_with_retry() -> Result<String, arboard::Error> {
    let mut last_err = None;
    for attempt in 0..3 {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(25 * (1 << attempt)));
        }
        match Clipboard::new().and_then(|mut cb| cb.get_text()) {
            Ok(text) => return Ok(text),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or(arboard::Error::ClipboardOccupied))
}

// ----------
// Unified Clipboard Evaluation
// Description: Inspects the clipboard for new file descriptors (CF_HDROP), image bitmaps, or text entries, attributes source tags, prevents duplicate internal copies, honors privacy controls, and persists to SQLite.
// ----------

fn check_clipboard(
    store: &ClipboardStore,
    app: &tauri::AppHandle,
    database_path: &std::path::Path,
    previous_files_hash: &mut Option<u64>,
    previous_text: &mut Option<String>,
    previous_image_hash: &mut Option<u64>,
) {
    // Check if user paused monitoring
    if is_monitoring_paused() {
        return;
    }

    // Check if clipboard contains sensitive privacy flags (e.g. 1Password / Bitwarden / KeePass)
    if is_sensitive_clipboard_content() {
        return;
    }

    let duplicate_mode = {
        let guard = DUPLICATE_HANDLING.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_empty() {
            "bump".to_string()
        } else {
            guard.clone()
        }
    };

    // 1. Check for file list content (CF_HDROP) first
    if let Some(files) = file_clipboard::read_clipboard_files() {
        if *previous_files_hash != Some(files.hash) {
            let is_internal_copy =
                LAST_INTERNAL_COPIED_FILES_HASH.swap(0, Ordering::SeqCst) == files.hash;

            if !is_internal_copy {
                let metadata = source::current();
                if !is_excluded_process(metadata.source_process.as_deref()) {
                    if duplicate_mode == "bump" {
                        if let Ok(Some(existing_id)) = store.find_existing_file(&files.files_json) {
                            let _ = store.bump_entry(existing_id, &metadata);
                            if let Ok(Some(entry)) = store.get_entry(existing_id) {
                                let _ = app.emit("clipboard://entry-bumped", crate::ClipboardEntry::from(entry));
                                crate::refresh_tray_recent_clips(app);
                            }
                            *previous_files_hash = Some(files.hash);
                            *previous_image_hash = None;
                            *previous_text = None;
                            return;
                        }
                    } else if duplicate_mode == "ignore" {
                        if let Ok(Some(_)) = store.find_existing_file(&files.files_json) {
                            *previous_files_hash = Some(files.hash);
                            *previous_image_hash = None;
                            *previous_text = None;
                            return;
                        }
                    }

                    match store.add_file_entry(&files.display_summary, &files.files_json, &metadata) {
                        Ok(id) => {
                            eprintln!("stored clipboard file entry {id}");
                            let entry = crate::ClipboardEntry {
                                id,
                                content: files.display_summary,
                                copied_at: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64,
                                source_app: metadata.source_app,
                                source_process: metadata.source_process,
                                window_title: metadata.window_title,
                                app_icon: metadata.app_icon,
                                is_pinned: false,
                                entry_type: "file".into(),
                                image_data: None,
                                image_dimensions: None,
                                files_data: Some(files.files_json),
                                source_url: None,
                                ocr_text: None,
                            };
                            let _ = app.emit("clipboard://new-entry", entry);
                            crate::refresh_tray_recent_clips(app);
                        }
                        Err(error) => eprintln!("could not store clipboard file: {error}"),
                    }
                }
            }

            *previous_files_hash = Some(files.hash);
            *previous_image_hash = None;
            *previous_text = None;
        }
        return;
    }

    let mut handled_image = false;

    // 2. Check for image content second using robust native Windows Win32 / CF_DIB / PNG
    if let Some(img) = image_clipboard::read_clipboard_image() {
        if *previous_image_hash != Some(img.raw_bytes_sample_hash) {
            let is_internal_copy = LAST_INTERNAL_COPIED_IMAGE_HASH.swap(0, Ordering::SeqCst)
                == img.raw_bytes_sample_hash;

            if !is_internal_copy {
                let mut metadata = source::current();
                if !is_excluded_process(metadata.source_process.as_deref()) {
                    let is_known_screenshot_tool = metadata
                        .source_process
                        .as_deref()
                        .is_some_and(|proc| {
                            proc.eq_ignore_ascii_case("ScreenClippingHost.exe")
                                || proc.eq_ignore_ascii_case("SnippingTool.exe")
                                || proc.eq_ignore_ascii_case("ShellExperienceHost.exe")
                                || proc.eq_ignore_ascii_case("clipbox.exe")
                        });

                    let is_screen_capture = !img.is_copied_image || is_known_screenshot_tool;

                    if is_screen_capture {
                        metadata.source_app = Some("Screen Capture".into());
                        metadata.source_process = None;
                        metadata.window_title = None;
                        metadata.app_icon = None;
                        metadata.source_url = None;
                    } else {
                        if metadata.window_title.is_none() {
                            metadata.window_title = Some("Copied Image".into());
                        }
                        if metadata.source_url.is_none() {
                            metadata.source_url = crate::browser_url::read_clipboard_source_url();
                        }
                    }

                    if duplicate_mode == "bump" {
                        if let Ok(Some(existing_id)) = store.find_existing_image(&img.data_url) {
                            let _ = store.bump_entry(existing_id, &metadata);
                            if let Ok(Some(entry)) = store.get_entry(existing_id) {
                                let _ = app.emit("clipboard://entry-bumped", crate::ClipboardEntry::from(entry));
                                crate::refresh_tray_recent_clips(app);
                            }
                            *previous_image_hash = Some(img.raw_bytes_sample_hash);
                            *previous_files_hash = None;
                            *previous_text = None;
                            return;
                        }
                    } else if duplicate_mode == "ignore" {
                        if let Ok(Some(_)) = store.find_existing_image(&img.data_url) {
                            *previous_image_hash = Some(img.raw_bytes_sample_hash);
                            *previous_files_hash = None;
                            *previous_text = None;
                            return;
                        }
                    }

                    match store.add_image_entry(&img.data_url, &img.dimensions, &metadata) {
                        Ok(id) => {
                            eprintln!("stored clipboard image entry {id}");
                            crate::ocr::scan_entry_async(database_path.to_path_buf(), id);
                            let entry = crate::ClipboardEntry {
                                id,
                                content: format!("Image ({})", img.dimensions),
                                copied_at: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64,
                                source_app: metadata.source_app,
                                source_process: metadata.source_process,
                                window_title: metadata.window_title,
                                app_icon: metadata.app_icon,
                                is_pinned: false,
                                entry_type: "image".into(),
                                image_data: Some(img.data_url),
                                image_dimensions: Some(img.dimensions),
                                files_data: None,
                                source_url: metadata.source_url,
                                ocr_text: None,
                            };
                            let _ = app.emit("clipboard://new-entry", entry);
                            crate::refresh_tray_recent_clips(app);
                        }
                        Err(error) => eprintln!("could not store clipboard image: {error}"),
                    }
                }
            }

            *previous_image_hash = Some(img.raw_bytes_sample_hash);
            *previous_files_hash = None;
            *previous_text = None;
        }
        handled_image = true;
    }

    // 3. If no file or image was found on the clipboard, check for text content with retry
    if !handled_image {
        if let Ok(text) = get_clipboard_text_with_retry() {
            let cleaned = clipbox_core::strip_leading_empty_lines(&text);

            let is_internal_copy = {
                let mut guard = LAST_INTERNAL_COPIED_TEXT
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if guard.as_deref() == Some(cleaned) {
                    *guard = None;
                    true
                } else {
                    false
                }
            };

            if !is_internal_copy && !cleaned.is_empty() {
                let mut metadata = source::current();
                if let Some(clip_url) = crate::browser_url::read_clipboard_source_url() {
                    metadata.source_url = Some(clip_url);
                } else if metadata.source_url.is_none() {
                    let trimmed = cleaned.trim();
                    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
                        && !trimmed.contains('\n')
                        && !trimmed.contains(' ')
                        && trimmed.len() > 8
                    {
                        metadata.source_url = Some(trimmed.to_string());
                    }
                }
                if !is_excluded_process(metadata.source_process.as_deref()) {
                    if duplicate_mode == "bump" {
                        if let Ok(Some(existing_id)) = store.find_existing_text(cleaned) {
                            let _ = store.bump_entry(existing_id, &metadata);
                            if let Ok(Some(entry)) = store.get_entry(existing_id) {
                                let _ = app.emit("clipboard://entry-bumped", crate::ClipboardEntry::from(entry));
                                crate::refresh_tray_recent_clips(app);
                            }
                            *previous_text = Some(cleaned.to_string());
                            *previous_image_hash = None;
                            *previous_files_hash = None;
                            return;
                        }
                    } else if duplicate_mode == "ignore" {
                        if let Ok(Some(_)) = store.find_existing_text(cleaned) {
                            *previous_text = Some(cleaned.to_string());
                            *previous_image_hash = None;
                            *previous_files_hash = None;
                            return;
                        }
                    }

                    match store.add_entry(cleaned, &metadata) {
                        Ok(id) => {
                            eprintln!("stored clipboard entry {id}");
                            let entry = crate::ClipboardEntry {
                                id,
                                content: cleaned.to_string(),
                                copied_at: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64,
                                source_app: metadata.source_app,
                                source_process: metadata.source_process,
                                window_title: metadata.window_title,
                                app_icon: metadata.app_icon,
                                is_pinned: false,
                                entry_type: "text".into(),
                                image_data: None,
                                image_dimensions: None,
                                files_data: None,
                                source_url: metadata.source_url,
                                ocr_text: None,
                            };
                            let _ = app.emit("clipboard://new-entry", entry);
                            crate::refresh_tray_recent_clips(app);
                        }
                        Err(error) => eprintln!("could not store clipboard text: {error}"),
                    }
                }
            }

            *previous_text = Some(cleaned.to_string());
            *previous_image_hash = None;
            *previous_files_hash = None;
        }
    }
}
