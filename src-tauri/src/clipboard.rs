// ----------
// Clipboard Monitoring Service
// Description: Background polling service monitoring the OS clipboard for text changes while gracefully handling empty clipboards, non-text formats, and notifying the frontend instantly via events.
// ----------

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use clipbox_core::ClipboardStore;
use tauri::Emitter;

use crate::source;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

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
    let mut previous_text: Option<String> = None;

    loop {
        match clipboard.get_text() {
            Ok(text) => {
                let cleaned = clipbox_core::strip_leading_empty_lines(&text);

                let is_new_text = previous_text
                    .as_ref()
                    .is_some_and(|previous| previous != cleaned);

                if is_new_text && !cleaned.is_empty() {
                    let metadata = source::current();

                    // Do not duplicate copies that originated from Clipbox itself
                    let is_self = metadata.source_process.as_deref().is_some_and(|proc| {
                        proc.eq_ignore_ascii_case("clipbox.exe")
                    });

                    if !is_self {
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
                                };
                                let _ = app.emit("clipboard://new-entry", entry);
                            }
                            Err(error) => eprintln!("could not store clipboard text: {error}"),
                        }
                    }
                }

                previous_text = Some(cleaned.to_string());
            }
            Err(arboard::Error::ContentNotAvailable) => {
                // Normal clipboard state when empty or holding non-text formats (images, files, etc.).
                // Silently ignore to eliminate terminal log spam.
            }
            Err(_transient_error) => {
                // Transient OS errors (e.g. another application briefly locking the clipboard during copy).
                // Silently retry on next poll interval.
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}
