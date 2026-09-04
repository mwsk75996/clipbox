// ----------
// Image OCR Text Extraction
// Description: Recognizes text in captured screenshots with the on-device Windows OCR engine, fully in background threads. Nothing is uploaded; records simply gain searchable text.
// ----------

use std::path::{Path, PathBuf};

use clipbox_core::ClipboardStore;

/// Scan one stored image for text without blocking the caller.
/// Missing records (deleted meanwhile), non-images, and engine failures all
/// resolve quietly; unscanned rows are picked up by a later sweep.
pub fn scan_entry_async(database_path: PathBuf, entry_id: i64) {
    std::thread::spawn(move || {
        if let Err(error) = scan_entry_blocking(&database_path, entry_id) {
            eprintln!("could not OCR image entry {entry_id}: {error}");
        }
    });
}

/// Scan stored images missing OCR text (oldest first, bounded per run).
pub fn backfill_missing(database_path: PathBuf) {
    std::thread::spawn(move || {
        let store = match ClipboardStore::open(&database_path) {
            Ok(store) => store,
            Err(error) => {
                eprintln!("could not open Clipbox database for OCR backfill: {error}");
                return;
            }
        };
        let ids = store.images_missing_ocr_data(50).unwrap_or_default();
        for id in ids {
            if let Err(error) = scan_entry_blocking(&database_path, id) {
                eprintln!("could not OCR image entry {id}: {error}");
            }
        }
    });
}

fn scan_entry_blocking(database_path: &Path, entry_id: i64) -> Result<(), String> {
    #[cfg(windows)]
    {
        // WinRT calls require an initialized COM apartment on this thread.
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    let store =
        ClipboardStore::open(database_path).map_err(|error| error.to_string())?;
    let data_url = match store
        .get_entry(entry_id)
        .map_err(|error| error.to_string())?
    {
        Some(entry) if entry.entry_type == "image" => entry.image_data,
        _ => return Ok(()),
    };
    let Some(data_url) = data_url else {
        return Ok(());
    };

    let (text, boxes) = recognize_data_url(&data_url, &preferred_language(&store))?;
    store
        .set_ocr_text(entry_id, &text)
        .map_err(|error| error.to_string())?;
    store
        .set_ocr_boxes(entry_id, &boxes)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Availability of the engine for an explicit language preference,
/// falling back through the automatic chain. Used by Settings so the
/// status pill describes the engine scans actually use.
pub fn engine_status_for(preferred: &str) -> (bool, String) {
    #[cfg(windows)]
    {
        match ocr_engine_preferred(preferred) {
            Ok((_, tag)) => (true, tag),
            Err(_) => (false, String::new()),
        }
    }

    #[cfg(not(windows))]
    {
        let _ = preferred;
        (false, String::new())
    }
}

/// Tags of installed OCR recognizer languages (e.g. "da-DK", "en-US").
#[cfg(windows)]
pub fn available_languages() -> Vec<String> {
    use windows::Media::Ocr::OcrEngine;

    let Ok(languages) = OcrEngine::AvailableRecognizerLanguages() else {
        return Vec::new();
    };
    let Ok(count) = languages.Size() else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for i in 0..count {
        if let Ok(language) = languages.GetAt(i) {
            if let Ok(tag) = language.LanguageTag() {
                tags.push(tag.to_string());
            }
        }
    }
    tags.sort();
    tags
}

#[cfg(not(windows))]
pub fn available_languages() -> Vec<String> {
    Vec::new()
}

/// Stored recognition language preference ("auto" = user profile, English fallback).
fn preferred_language(store: &ClipboardStore) -> String {
    store
        .get_setting("ocr_language")
        .ok()
        .flatten()
        .unwrap_or_else(|| "auto".into())
}

/// Engine for an explicit language preference, falling back to the
/// automatic chain when unset ("auto") or unavailable.
#[cfg(windows)]
pub(crate) fn ocr_engine_preferred(
    preferred: &str,
) -> Result<(windows::Media::Ocr::OcrEngine, String), String> {
    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine;

    if preferred != "auto" {
        if let Ok(language) = Language::CreateLanguage(&windows::core::HSTRING::from(preferred))
        {
            if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&language) {
                return Ok((engine, preferred.into()));
            }
        }
    }
    ocr_engine()
}

#[cfg(windows)]
fn ocr_engine() -> Result<(windows::Media::Ocr::OcrEngine, String), String> {    use windows::Globalization::Language;
    use windows::Media::Ocr::OcrEngine;

    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        let tag = engine
            .RecognizerLanguage()
            .and_then(|language| language.LanguageTag())
            .map(|tag| tag.to_string())
            .unwrap_or_default();
        return Ok((engine, tag));
    }

    let english = Language::CreateLanguage(&windows::core::HSTRING::from("en"))
        .map_err(|error| format!("no OCR recognizer available: {error}"))?;
    let engine = OcrEngine::TryCreateFromLanguage(&english)
        .map_err(|error| format!("no OCR recognizer available: {error}"))?;
    Ok((engine, "en".into()))
}

#[cfg(windows)]
fn recognize_data_url(data_url: &str, preferred: &str) -> Result<(String, String), String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};

    let base64_str = data_url.split(',').next_back().unwrap_or(data_url);
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|error| format!("failed to decode image data: {error}"))?;
    use base64::Engine as _;

    let mut dynamic =
        image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png)
            .map_err(|error| format!("failed to decode image data: {error}"))?;

    // Cap the longest side: huge screenshots OCR slowly with no accuracy gain.
    // The engine's own maximum is authoritative; fall back to 3200px.
    let max_side = windows::Media::Ocr::OcrEngine::MaxImageDimension().unwrap_or(3200);
    if dynamic.width().max(dynamic.height()) > max_side {
        dynamic = dynamic.thumbnail(max_side, max_side);
    }
    let rgba = dynamic.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let pixels = rgba.into_raw();

    // &[u8] cannot become an IBuffer directly; copy through a Buffer surface.
    use windows::Storage::Streams::Buffer;
    use windows::Win32::System::WinRT::IBufferByteAccess;
    use windows::core::Interface as _;
    let buffer = Buffer::Create(pixels.len() as u32)
        .map_err(|error| format!("failed to prepare image for OCR: {error}"))?;
    let access: IBufferByteAccess = buffer
        .cast()
        .map_err(|error| format!("failed to prepare image for OCR: {error}"))?;
    let data_ptr = unsafe {
        access
            .Buffer()
            .map_err(|error| format!("failed to prepare image for OCR: {error}"))?
    };
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), data_ptr, pixels.len());
    }
    buffer
        .SetLength(pixels.len() as u32)
        .map_err(|error| format!("failed to prepare image for OCR: {error}"))?;

    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Rgba8,
        width as i32,
        height as i32,
    )
    .map_err(|error| format!("failed to prepare image for OCR: {error}"))?;

    let (engine, _) = ocr_engine_preferred(preferred)?;
    let operation = engine
        .RecognizeAsync(&bitmap)
        .map_err(|error| format!("failed to start text recognition: {error}"))?;

    // WinRT async has no Rust future bridge here; poll the operation instead.
    use windows_future::AsyncStatus;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match operation
            .Status()
            .map_err(|error| format!("failed to poll text recognition: {error}"))?
        {
            AsyncStatus::Completed => break,
            AsyncStatus::Started => {
                if std::time::Instant::now() > deadline {
                    return Err("image text recognition timed out".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            other => {
                return Err(format!(
                    "image text recognition ended unexpectedly: {other:?}"
                ))
            }
        }
    }

    let outcome = operation
        .GetResults()
        .map_err(|error| format!("failed to read text recognition result: {error}"))?;

    let mut text = String::new();
    // Word boxes as fractions of the bitmap, so the frontend can overlay
    // them at any display size without knowing the scan resolution.
    let mut boxes = Vec::new();
    let lines = outcome
        .Lines()
        .map_err(|error| format!("failed to read text recognition result: {error}"))?;
    for i in 0..lines
        .Size()
        .map_err(|error| format!("failed to read text recognition result: {error}"))?
    {
        let line = lines
            .GetAt(i)
            .map_err(|error| format!("failed to read text recognition result: {error}"))?;
        let words = line
            .Words()
            .map_err(|error| format!("failed to read text recognition result: {error}"))?;
        let mut line_text = String::new();
        for j in 0..words
            .Size()
            .map_err(|error| format!("failed to read text recognition result: {error}"))?
        {
            let word = words
                .GetAt(j)
                .map_err(|error| format!("failed to read text recognition result: {error}"))?;
            let word_text = word
                .Text()
                .map_err(|error| format!("failed to read text recognition result: {error}"))?
                .to_string();
            if j > 0 {
                line_text.push(' ');
            }
            line_text.push_str(&word_text);
            if !word_text.trim().is_empty() {
                if let Ok(rect) = word.BoundingRect() {
                    boxes.push(serde_json::json!({
                        "t": word_text,
                        "x": rect.X / width as f32,
                        "y": rect.Y / height as f32,
                        "w": rect.Width / width as f32,
                        "h": rect.Height / height as f32,
                    }));
                }
            }
        }
        if !line_text.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line_text.trim());
        }
    }
    let boxes_json =
        serde_json::to_string(&boxes).map_err(|error| format!("failed to encode word boxes: {error}"))?;
    Ok((text, boxes_json))
}

#[cfg(not(windows))]
fn recognize_data_url(_data_url: &str, _preferred: &str) -> Result<(String, String), String> {
    Err("image text recognition is currently supported on Windows".into())
}
