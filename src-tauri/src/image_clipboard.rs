// ----------
// Robust Native Clipboard Image Reader & Writer
// Description: Direct Win32 clipboard integration supporting CF_DIB (standard Windows bitmap), CF_DIBV5, and registered PNG formats with fallback to arboard for cross-platform support.
// ----------

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::ImageEncoder;

pub struct CapturedImage {
    pub data_url: String,
    pub dimensions: String,
    pub raw_bytes_sample_hash: u64,
    pub is_copied_image: bool,
}

#[cfg(windows)]
pub fn read_clipboard_image() -> Option<CapturedImage> {
    use windows::core::w;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }

        struct ClipboardGuard;
        impl Drop for ClipboardGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        let _guard = ClipboardGuard;

        // Browsers & rich apps place HTML format on the clipboard when a user
        // right-clicks and chooses "Copy Image". Pure screenshots (Snipping Tool,
        // PrintScreen, Win+Shift+S) do not place HTML format.
        let html_format = RegisterClipboardFormatW(w!("HTML Format"));
        let has_html = html_format != 0 && IsClipboardFormatAvailable(html_format).is_ok();

        // 1. Try registered "PNG" format first (used by many web browsers & modern apps)
        let png_format = RegisterClipboardFormatW(w!("PNG"));
        if png_format != 0 && IsClipboardFormatAvailable(png_format).is_ok() {
            if let Ok(handle) = GetClipboardData(png_format) {
                if !handle.is_invalid() {
                    let hglobal = HGLOBAL(handle.0 as *mut _);
                    let size = GlobalSize(hglobal);
                    if size > 0 {
                        let locked = GlobalLock(hglobal);
                        if !locked.is_null() {
                            let bytes = std::slice::from_raw_parts(locked as *const u8, size);
                            let cloned = bytes.to_vec();
                            let _ = GlobalUnlock(hglobal);

                            if let Ok(img) = image::load_from_memory(&cloned) {
                                let width = img.width();
                                let height = img.height();
                                let b64 = STANDARD.encode(&cloned);
                                let data_url = format!("data:image/png;base64,{b64}");
                                let dimensions = format!("{width}x{height}");
                                let raw_bytes_sample_hash = compute_bytes_hash(&cloned);
                                return Some(CapturedImage {
                                    data_url,
                                    dimensions,
                                    raw_bytes_sample_hash,
                                    is_copied_image: has_html,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2. Try CF_DIB (Format 8) or CF_DIBV5 (Format 17)
        // Windows OS automatically synthesizes CF_DIB for ANY image/screenshot placed on the clipboard!
        const CF_DIB: u32 = 8;
        const CF_DIBV5: u32 = 17;

        let dib_format = if IsClipboardFormatAvailable(CF_DIBV5).is_ok() {
            Some(CF_DIBV5)
        } else if IsClipboardFormatAvailable(CF_DIB).is_ok() {
            Some(CF_DIB)
        } else {
            None
        };

        if let Some(format_id) = dib_format {
            if let Ok(handle) = GetClipboardData(format_id) {
                if !handle.is_invalid() {
                    let hglobal = HGLOBAL(handle.0 as *mut _);
                    let size = GlobalSize(hglobal);
                    if size > 40 {
                        let locked = GlobalLock(hglobal);
                        if !locked.is_null() {
                            let dib_slice = std::slice::from_raw_parts(locked as *const u8, size);
                            let dynamic_img = dib_to_image(dib_slice);
                            let _ = GlobalUnlock(hglobal);

                            if let Ok(img) = dynamic_img {
                                let width = img.width();
                                let height = img.height();
                                if let Ok(data_url) = dynamic_image_to_png_data_url(&img) {
                                    let dimensions = format!("{width}x{height}");
                                    let raw_bytes_sample_hash = compute_bytes_hash(dib_slice);
                                    return Some(CapturedImage {
                                        data_url,
                                        dimensions,
                                        raw_bytes_sample_hash,
                                        is_copied_image: has_html,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(not(windows))]
pub fn read_clipboard_image() -> Option<CapturedImage> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let img = clipboard.get_image().ok()?;
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            &img.bytes,
            img.width as u32,
            img.height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;

    let b64 = STANDARD.encode(&png_bytes);
    let data_url = format!("data:image/png;base64,{b64}");
    let dimensions = format!("{}x{}", img.width, img.height);
    let raw_bytes_sample_hash = compute_bytes_hash(&img.bytes);
    Some(CapturedImage {
        data_url,
        dimensions,
        raw_bytes_sample_hash,
        is_copied_image: false,
    })
}

fn dib_to_image(dib: &[u8]) -> Result<image::DynamicImage, String> {
    if dib.len() < 40 {
        return Err("DIB buffer too small".into());
    }

    let header_size = u32::from_le_bytes(dib[0..4].try_into().unwrap()) as usize;
    let bit_count = if dib.len() >= 16 {
        u16::from_le_bytes(dib[14..16].try_into().unwrap())
    } else {
        0
    };
    let compression = if dib.len() >= 20 {
        u32::from_le_bytes(dib[16..20].try_into().unwrap())
    } else {
        0
    };
    let clr_used = if dib.len() >= 36 {
        u32::from_le_bytes(dib[32..36].try_into().unwrap())
    } else {
        0
    };

    let palette_colors = if clr_used > 0 {
        clr_used as usize
    } else if bit_count <= 8 && bit_count > 0 {
        1usize << bit_count
    } else {
        0
    };

    let masks_size = if header_size == 40 && (compression == 3 || compression == 6) {
        12 // 3 DWORD color masks (BI_BITFIELDS)
    } else {
        0
    };

    let palette_size = palette_colors * 4;
    let off_bits = 14 + header_size + masks_size + palette_size;
    let file_size = 14 + dib.len();

    let mut bmp_file = Vec::with_capacity(file_size);
    bmp_file.extend_from_slice(b"BM"); // bfType
    bmp_file.extend_from_slice(&(file_size as u32).to_le_bytes()); // bfSize
    bmp_file.extend_from_slice(&0u16.to_le_bytes()); // bfReserved1
    bmp_file.extend_from_slice(&0u16.to_le_bytes()); // bfReserved2
    bmp_file.extend_from_slice(&(off_bits as u32).to_le_bytes()); // bfOffBits
    bmp_file.extend_from_slice(dib);

    image::load_from_memory(&bmp_file).map_err(|e| format!("failed to decode DIB: {e}"))
}

fn dynamic_image_to_png_data_url(img: &image::DynamicImage) -> Result<String, String> {
    let rgba = img.to_rgba8();
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            &rgba,
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("failed to encode PNG: {e}"))?;

    let b64 = STANDARD.encode(&png_bytes);
    Ok(format!("data:image/png;base64,{b64}"))
}

pub fn compute_bytes_hash(bytes: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    let len = bytes.len();
    if len <= 2048 {
        bytes.hash(&mut hasher);
    } else {
        bytes[..512].hash(&mut hasher);
        bytes[len / 2..len / 2 + 512].hash(&mut hasher);
        bytes[len - 512..].hash(&mut hasher);
    }
    hasher.finish()
}

// ----------
// Native Multi-Format Image Clipboard Writer
// Description: Writes PNG bytes to the Windows clipboard in both standard CF_DIB (Format 8) for legacy applications (Paint, Office, Photoshop) and registered "PNG" format for modern applications (Discord, web browsers, Slack).
// ----------

#[cfg(windows)]
pub fn write_clipboard_image(png_bytes: &[u8]) -> Result<(), String> {
    use windows::core::w;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    // Decode PNG to get dimensions and RGBA pixels for CF_DIB
    let img = image::load_from_memory(png_bytes)
        .map_err(|e| format!("failed to decode PNG bytes: {e}"))?
        .to_rgba8();

    let width = img.width() as usize;
    let height = img.height() as usize;
    let raw_rgba = img.into_raw();

    // Standard Windows CF_DIB format:
    // BITMAPINFOHEADER (40 bytes) + bottom-up BGR / BGRA pixel array
    // Each row must be aligned to a 4-byte boundary.
    let row_stride = ((width * 32 + 31) / 32) * 4;
    let image_size = row_stride * height;
    let total_dib_size = 40 + image_size;

    let mut dib_bytes = vec![0u8; total_dib_size];
    // biSize = 40
    dib_bytes[0..4].copy_from_slice(&40u32.to_le_bytes());
    // biWidth
    dib_bytes[4..8].copy_from_slice(&(width as i32).to_le_bytes());
    // biHeight (positive for bottom-up)
    dib_bytes[8..12].copy_from_slice(&(height as i32).to_le_bytes());
    // biPlanes = 1
    dib_bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
    // biBitCount = 32
    dib_bytes[14..16].copy_from_slice(&32u16.to_le_bytes());
    // biCompression = BI_RGB (0)
    dib_bytes[16..20].copy_from_slice(&0u32.to_le_bytes());
    // biSizeImage
    dib_bytes[20..24].copy_from_slice(&(image_size as u32).to_le_bytes());

    // Write pixels bottom-up, converting RGBA to BGRA
    for y in 0..height {
        let src_row = height - 1 - y;
        let dst_row_offset = 40 + y * row_stride;
        for x in 0..width {
            let src_idx = (src_row * width + x) * 4;
            let dst_idx = dst_row_offset + x * 4;
            let r = raw_rgba[src_idx];
            let g = raw_rgba[src_idx + 1];
            let b = raw_rgba[src_idx + 2];
            let a = raw_rgba[src_idx + 3];
            dib_bytes[dst_idx] = b;
            dib_bytes[dst_idx + 1] = g;
            dib_bytes[dst_idx + 2] = r;
            dib_bytes[dst_idx + 3] = a;
        }
    }

    unsafe {
        // Retry OpenClipboard in case another application holds it momentarily
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
            return Err("could not open Windows clipboard to write image".into());
        }

        struct ClipboardGuard;
        impl Drop for ClipboardGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        let _guard = ClipboardGuard;

        let _ = EmptyClipboard();

        // 1. Set standard CF_DIB (Format 8) for Paint, Word, and native Windows viewers
        const CF_DIB: u32 = 8;
        if let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, dib_bytes.len()) {
            let locked = GlobalLock(hglobal);
            if !locked.is_null() {
                std::ptr::copy_nonoverlapping(dib_bytes.as_ptr(), locked as *mut u8, dib_bytes.len());
                let _ = GlobalUnlock(hglobal);
                let _ = SetClipboardData(CF_DIB, Some(HANDLE(hglobal.0)));
            }
        }

        // 2. Set registered "PNG" format (with alpha channel for Discord, browsers, Slack)
        let png_format = RegisterClipboardFormatW(w!("PNG"));
        if png_format != 0 {
            if let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, png_bytes.len()) {
                let locked = GlobalLock(hglobal);
                if !locked.is_null() {
                    std::ptr::copy_nonoverlapping(png_bytes.as_ptr(), locked as *mut u8, png_bytes.len());
                    let _ = GlobalUnlock(hglobal);
                    let _ = SetClipboardData(png_format, Some(HANDLE(hglobal.0)));
                }
            }
        }

        Ok(())
    }
}

#[cfg(not(windows))]
pub fn write_clipboard_image(png_bytes: &[u8]) -> Result<(), String> {
    let img = image::load_from_memory(png_bytes)
        .map_err(|e| format!("failed to decode PNG bytes: {e}"))?
        .to_rgba8();
    let width = img.width() as usize;
    let height = img.height() as usize;
    let bytes = img.into_raw();
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width,
            height,
            bytes: std::borrow::Cow::Owned(bytes),
        })
        .map_err(|e| e.to_string())
}
