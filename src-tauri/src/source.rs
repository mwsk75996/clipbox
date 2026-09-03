// ----------
// Application Source Metadata & Icon Extraction
// Description: Identifies active foreground window, owning process path, window title, and extracts the application's shell icon converted to a base64 BMP data URL.
// ----------

use clipbox_core::ClipboardMetadata;

/// Capture metadata about the application currently owning the foreground
/// window. Other platforms can add their native implementation here later.
pub fn current() -> ClipboardMetadata {
    #[cfg(windows)]
    {
        windows_source::current()
    }

    #[cfg(not(windows))]
    {
        ClipboardMetadata::default()
    }
}

#[cfg(windows)]
mod windows_source {
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::Mutex;

    use clipbox_core::ClipboardMetadata;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON};
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GetForegroundWindow, GetIconInfo, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, HICON, ICONINFO,
    };

    static ICON_CACHE: std::sync::LazyLock<Mutex<HashMap<String, Option<String>>>> =
        std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

    pub fn current() -> ClipboardMetadata {
        let window = unsafe { GetForegroundWindow() };
        if window.0.is_null() {
            return ClipboardMetadata::default();
        }

        let mut process_id = 0;
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut process_id));
        }

        let process_path = process_path(process_id);
        let source_process = process_path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .map(|name| name.to_string_lossy().into_owned());
        let source_app = process_path
            .as_deref()
            .and_then(|path| Path::new(path).file_stem())
            .map(|name| name.to_string_lossy().into_owned());

        let app_icon = process_path.as_deref().and_then(|path| {
            let mut cache = ICON_CACHE.lock().ok()?;
            if let Some(cached) = cache.get(path) {
                return cached.clone();
            }
            let icon = extract_icon_base64(path);
            cache.insert(path.to_string(), icon.clone());
            icon
        });

        let source_url = if crate::browser_url::is_browser_process(source_process.as_deref()) {
            crate::browser_url::get_browser_url_from_window(window)
        } else {
            None
        };

        ClipboardMetadata {
            source_app,
            source_process,
            window_title: window_title(window),
            app_icon,
            source_url,
        }
    }

    fn window_title(window: windows::Win32::Foundation::HWND) -> Option<String> {
        let length = unsafe { GetWindowTextLengthW(window) };
        if length <= 0 {
            return None;
        }

        let mut buffer = vec![0u16; length as usize + 1];
        let written = unsafe { GetWindowTextW(window, &mut buffer) };
        if written <= 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&buffer[..written as usize]))
    }

    fn process_path(process_id: u32) -> Option<String> {
        let process =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()? };
        let mut buffer = vec![0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        };
        unsafe {
            let _ = CloseHandle(process);
        }

        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..length as usize]))
    }

    fn extract_icon_base64(exe_path: &str) -> Option<String> {
        let mut wide_path: Vec<u16> = OsStr::new(exe_path).encode_wide().collect();
        wide_path.push(0);

        let mut shfi = SHFILEINFOW::default();
        let result = unsafe {
            SHGetFileInfoW(
                PCWSTR(wide_path.as_ptr()),
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut shfi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON,
            )
        };

        if result == 0 || shfi.hIcon.0.is_null() {
            return None;
        }

        let hicon = shfi.hIcon;
        let data_url = icon_to_bmp_base64(hicon);

        unsafe {
            let _ = DestroyIcon(hicon);
        }

        data_url
    }

    fn icon_to_bmp_base64(hicon: HICON) -> Option<String> {
        unsafe {
            let mut icon_info = ICONINFO::default();
            if GetIconInfo(hicon, &mut icon_info).is_err() {
                return None;
            }

            let hbm_color = icon_info.hbmColor;
            let hbm_mask = icon_info.hbmMask;

            // Ensure HBITMAP resources are deleted when dropped
            struct BitmapGuard(windows::Win32::Graphics::Gdi::HBITMAP);
            impl Drop for BitmapGuard {
                fn drop(&mut self) {
                    if !self.0 .0.is_null() {
                        unsafe {
                            let _ = DeleteObject(self.0.into());
                        }
                    }
                }
            }
            let _guard_color = BitmapGuard(hbm_color);
            let _guard_mask = BitmapGuard(hbm_mask);

            let bitmap_to_read = if !hbm_color.0.is_null() {
                hbm_color
            } else if !hbm_mask.0.is_null() {
                hbm_mask
            } else {
                return None;
            };

            let mut bm = BITMAP::default();
            let bytes_copied = GetObjectW(
                bitmap_to_read.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut _ as *mut _),
            );
            if bytes_copied == 0 {
                return None;
            }

            let width = bm.bmWidth;
            let height = if hbm_color.0.is_null() {
                bm.bmHeight / 2
            } else {
                bm.bmHeight
            };

            if width <= 0 || height <= 0 {
                return None;
            }

            let hdc = CreateCompatibleDC(None);
            if hdc.0.is_null() {
                return None;
            }

            struct DcGuard(HDC);
            impl Drop for DcGuard {
                fn drop(&mut self) {
                    if !self.0 .0.is_null() {
                        unsafe {
                            let _ = DeleteDC(self.0);
                        }
                    }
                }
            }
            let _guard_dc = DcGuard(hdc);

            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // negative for top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    biSizeImage: (width * height * 4) as u32,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [windows::Win32::Graphics::Gdi::RGBQUAD::default()],
            };

            let mut pixels = vec![0u8; (width * height * 4) as usize];
            let lines = GetDIBits(
                hdc,
                bitmap_to_read,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );

            if lines == 0 {
                return None;
            }

            // Fix alpha channel if all alphas are 0 (standard for 24-bit/32-bit GDI bitmaps)
            let has_any_alpha = pixels.chunks_exact(4).any(|p| p[3] > 0);
            if !has_any_alpha {
                for chunk in pixels.chunks_exact_mut(4) {
                    chunk[3] = 255;
                }
            }

            let bmp_bytes = encode_bmp(width as u32, height as u32, &pixels);
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bmp_bytes);
            Some(format!("data:image/bmp;base64,{encoded}"))
        }
    }

    fn encode_bmp(width: u32, height: u32, bgra_pixels: &[u8]) -> Vec<u8> {
        let file_header_size: u32 = 14;
        let info_header_size: u32 = 40;
        let image_size = bgra_pixels.len() as u32;
        let file_size = file_header_size + info_header_size + image_size;

        let mut bmp = Vec::with_capacity(file_size as usize);
        // BITMAPFILEHEADER
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size.to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]); // reserved
        bmp.extend_from_slice(&(file_header_size + info_header_size).to_le_bytes()); // offset to pixel data (54)

        // BITMAPINFOHEADER
        bmp.extend_from_slice(&info_header_size.to_le_bytes()); // biSize (40)
        bmp.extend_from_slice(&(width as i32).to_le_bytes());
        bmp.extend_from_slice(&(-(height as i32)).to_le_bytes()); // negative for top-down
        bmp.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        bmp.extend_from_slice(&32u16.to_le_bytes()); // biBitCount (32-bit BGRA)
        bmp.extend_from_slice(&0u32.to_le_bytes()); // biCompression (BI_RGB)
        bmp.extend_from_slice(&image_size.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes()); // biXPelsPerMeter
        bmp.extend_from_slice(&0u32.to_le_bytes()); // biYPelsPerMeter
        bmp.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
        bmp.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

        bmp.extend_from_slice(bgra_pixels);
        bmp
    }
}
