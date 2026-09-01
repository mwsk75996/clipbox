use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use clipbox_core::ClipboardStore;

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
                    match store.add_text(&text) {
                        Ok(id) => eprintln!("stored clipboard entry {id}"),
                        Err(error) => eprintln!("could not store clipboard text: {error}"),
                    }
                }

                previous_text = Some(text);
            }
            Err(error) => eprintln!("could not read the system clipboard: {error}"),
        }

        thread::sleep(POLL_INTERVAL);
    }
}
