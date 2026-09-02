// ----------
// Clipboard Monitoring Service
// Description: Background polling service monitoring the OS clipboard for text and image changes while gracefully handling empty clipboards and non-standard formats.
// ----------

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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

fn compute_image_hash(image: &arboard::ImageData) -> u64 {
    let mut hasher = DefaultHasher::new();
    image.width.hash(&mut hasher);
    image.height.hash(&mut hasher);
    image.bytes.len().hash(&mut hasher);
    let len = image.bytes.len();
    if len <= 2048 {
        image.bytes.hash(&mut hasher);
    } else {
        image.bytes[..512].hash(&mut hasher);
        image.bytes[len / 2..len / 2 + 512].hash(&mut hasher);
        image.bytes[len - 512..].hash(&mut hasher);
    }
    hasher.finish()
}

fn image_data_to_png_data_url(image: &arboard::ImageData) -> Result<String, String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use image::ImageEncoder;

    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            &image.bytes,
            image.width as u32,
            image.height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("failed to encode PNG: {e}"))?;

    let b64 = STANDARD.encode(&png_bytes);
    Ok(format!("data:image/png;base64,{b64}"))
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
    let mut previous_image_hash: Option<u64> = None;

    loop {
        let mut handled_image = false;

        // 1. Check for image content first
        match clipboard.get_image() {
            Ok(img) => {
                let hash = compute_image_hash(&img);
                if previous_image_hash != Some(hash) {
                    let metadata = source::current();
                    let is_self = metadata.source_process.as_deref().is_some_and(|proc| {
                        proc.eq_ignore_ascii_case("clipbox.exe")
                    });

                    if !is_self {
                        if let Ok(data_url) = image_data_to_png_data_url(&img) {
                            let dimensions = format!("{}x{}", img.width, img.height);
                            match store.add_image_entry(&data_url, &dimensions, &metadata) {
                                Ok(id) => {
                                    eprintln!("stored clipboard image entry {id}");
                                    let entry = crate::ClipboardEntry {
                                        id,
                                        content: format!("Image ({dimensions})"),
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
                                        image_data: Some(data_url),
                                        image_dimensions: Some(dimensions),
                                    };
                                    let _ = app.emit("clipboard://new-entry", entry);
                                }
                                Err(error) => eprintln!("could not store clipboard image: {error}"),
                            }
                        }
                    }

                    previous_image_hash = Some(hash);
                    previous_text = None;
                }
                handled_image = true;
            }
            Err(arboard::Error::ContentNotAvailable) => {}
            Err(_transient_error) => {}
        }

        // 2. If no image was found, check for text content
        if !handled_image {
            match clipboard.get_text() {
                Ok(text) => {
                    let cleaned = clipbox_core::strip_leading_empty_lines(&text);

                    let is_new_text = previous_text
                        .as_ref()
                        .is_some_and(|previous| previous != cleaned);

                    if is_new_text && !cleaned.is_empty() {
                        let metadata = source::current();
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
                                        entry_type: "text".into(),
                                        image_data: None,
                                        image_dimensions: None,
                                    };
                                    let _ = app.emit("clipboard://new-entry", entry);
                                }
                                Err(error) => eprintln!("could not store clipboard text: {error}"),
                            }
                        }
                    }

                    previous_text = Some(cleaned.to_string());
                    previous_image_hash = None;
                }
                Err(arboard::Error::ContentNotAvailable) => {}
                Err(_transient_error) => {}
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}
