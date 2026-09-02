// ----------
// Clipboard Monitoring Service
// Description: Background polling service monitoring the OS clipboard for text changes while gracefully handling empty clipboards and non-text formats without console spam.
// ----------

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use clipbox_core::ClipboardStore;

use crate::source;

const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Start the background clipboard monitor.
pub fn start(database_path: PathBuf) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("clipbox-clipboard-monitor".into())
        .spawn(move || monitor(database_path))
        .expect("failed to start clipboard monitor")
}

fn monitor(database_path: PathBuf) {
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
    let mut previous_text = None;

    loop {
        match clipboard.get_text() {
            Ok(text) => {
                let is_new_text = previous_text
                    .as_ref()
                    .is_some_and(|previous| previous != &text);

                if is_new_text && !text.is_empty() {
                    let cleaned = clipbox_core::strip_leading_empty_lines(&text);
                    let metadata = source::current();
                    match store.add_entry(cleaned, &metadata) {
                        Ok(id) => eprintln!("stored clipboard entry {id}"),
                        Err(error) => eprintln!("could not store clipboard text: {error}"),
                    }
                }

                previous_text = Some(text);
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
