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
            &mut previous_files_hash,
            &mut previous_text,
            &mut previous_image_hash,
        );
        thread::sleep(Duration::from_millis(300));
    }
}

// ----------
// Unified Clipboard Evaluation
// Description: Inspects the clipboard for new file descriptors (CF_HDROP), image bitmaps, or text entries, attributes source tags, prevents duplicate internal copies, and persists to SQLite.
// ----------

fn check_clipboard(
    store: &ClipboardStore,
    app: &tauri::AppHandle,
    previous_files_hash: &mut Option<u64>,
    previous_text: &mut Option<String>,
    previous_image_hash: &mut Option<u64>,
) {
    // 1. Check for file list content (CF_HDROP) first
    if let Some(files) = file_clipboard::read_clipboard_files() {
        if *previous_files_hash != Some(files.hash) {
            let is_internal_copy =
                LAST_INTERNAL_COPIED_FILES_HASH.swap(0, Ordering::SeqCst) == files.hash;

            if !is_internal_copy {
                let metadata = source::current();
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
                        };
                        let _ = app.emit("clipboard://new-entry", entry);
                    }
                    Err(error) => eprintln!("could not store clipboard file: {error}"),
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
            // Check if this image was copied internally by Clipbox's Copy button
            let is_internal_copy = LAST_INTERNAL_COPIED_IMAGE_HASH.swap(0, Ordering::SeqCst)
                == img.raw_bytes_sample_hash;

            if !is_internal_copy {
                let mut metadata = source::current();

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
                } else {
                    // User explicitly right-clicked "Copy Image" in an application (Brave, Discord, etc.)
                    metadata.window_title = Some("Copied Image".into());
                }

                match store.add_image_entry(&img.data_url, &img.dimensions, &metadata) {
                    Ok(id) => {
                        eprintln!("stored clipboard image entry {id}");
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
                        };
                        let _ = app.emit("clipboard://new-entry", entry);
                    }
                    Err(error) => eprintln!("could not store clipboard image: {error}"),
                }
            }

            *previous_image_hash = Some(img.raw_bytes_sample_hash);
            *previous_files_hash = None;
            *previous_text = None;
        }
        handled_image = true;
    }

    // 3. If no file or image was found on the clipboard, check for text content
    if !handled_image {
        if let Ok(mut clipboard) = Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
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

                let is_new_text = previous_text
                    .as_ref()
                    .is_some_and(|previous| previous != cleaned);

                if !is_internal_copy && is_new_text && !cleaned.is_empty() {
                    let metadata = source::current();
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
                            };
                            let _ = app.emit("clipboard://new-entry", entry);
                        }
                        Err(error) => eprintln!("could not store clipboard text: {error}"),
                    }
                }

                *previous_text = Some(cleaned.to_string());
                *previous_image_hash = None;
                *previous_files_hash = None;
            }
        }
    }
}
