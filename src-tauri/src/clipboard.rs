// ----------
// Clipboard Monitoring Service
// Description: Background polling service monitoring the OS clipboard for text and native images (CF_DIB, CF_DIBV5, PNG) with duplicate avoidance and real-time frontend emissions.
// ----------

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use clipbox_core::ClipboardStore;
use tauri::Emitter;

use crate::image_clipboard;
use crate::source;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

static LAST_INTERNAL_COPIED_TEXT: Mutex<Option<String>> = Mutex::new(None);
static LAST_INTERNAL_COPIED_IMAGE_HASH: AtomicU64 = AtomicU64::new(0);

// ----------
// Internal Clipboard Copy Tracking
// Description: Records content copied to the OS clipboard by Clipbox user actions (e.g. clicking the Copy button on an entry card) so the background monitor does not create duplicate entries in history, while ensuring external screenshots or copies taken while Clipbox is focused are always captured.
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

/// Start the background clipboard monitor.
pub fn start(database_path: PathBuf, app: tauri::AppHandle) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("clipbox-clipboard-monitor".into())
        .spawn(move || monitor(database_path, app))
        .expect("failed to start clipboard monitor")
}

fn monitor(database_path: PathBuf, app: tauri::AppHandle) {
    let store = match ClipboardStore::open(&database_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("could not open Clipbox database: {error}");
            return;
        }
    };

    let mut clipboard = match Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            eprintln!("could not access the system clipboard: {error}");
            return;
        }
    };

    // The current clipboard is the baseline. This prevents old clipboard
    // contents from being recorded just because Clipbox was started.
    let mut previous_text: Option<String> = clipboard
        .get_text()
        .ok()
        .map(|t| clipbox_core::strip_leading_empty_lines(&t).to_string());
    let mut previous_image_hash: Option<u64> =
        image_clipboard::read_clipboard_image().map(|img| img.raw_bytes_sample_hash);

    loop {
        let mut handled_image = false;

        // 1. Check for image content first using robust native Windows Win32 / CF_DIB / PNG
        if let Some(img) = image_clipboard::read_clipboard_image() {
            if previous_image_hash != Some(img.raw_bytes_sample_hash) {
                // Check if this image was copied internally by Clipbox's Copy button
                let is_internal_copy = LAST_INTERNAL_COPIED_IMAGE_HASH.swap(0, Ordering::SeqCst)
                    == img.raw_bytes_sample_hash;

                if !is_internal_copy {
                    let mut metadata = source::current();
                    // If Clipbox happened to be focused when taking a screenshot, attribute nicely
                    if metadata
                        .source_process
                        .as_deref()
                        .is_some_and(|proc| proc.eq_ignore_ascii_case("clipbox.exe"))
                    {
                        metadata.source_app = Some("Screenshot".into());
                        metadata.window_title = Some("Screen Capture".into());
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
                            };
                            let _ = app.emit("clipboard://new-entry", entry);
                        }
                        Err(error) => eprintln!("could not store clipboard image: {error}"),
                    }
                }

                previous_image_hash = Some(img.raw_bytes_sample_hash);
                previous_text = None;
            }
            handled_image = true;
        }

        // 2. If no image was found on the clipboard, check for text content
        if !handled_image {
            match clipboard.get_text() {
                Ok(text) => {
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
                                };
                                let _ = app.emit("clipboard://new-entry", entry);
                            }
                            Err(error) => eprintln!("could not store clipboard text: {error}"),
                        }
                    }

                    previous_text = Some(cleaned.to_string());
                    previous_image_hash = None;
                }
                Err(_transient_error) => {}
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}
