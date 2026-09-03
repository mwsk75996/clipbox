// ----------
// File Clipboard Interoperability (CF_HDROP)
// Description: Detects and reads copied files and folders from the Windows clipboard using CF_HDROP without duplicating file data on disk, extracts file metadata (name, path, extension, size, is_dir), and restores CF_HDROP descriptors for pasting into File Explorer.
// ----------

use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub name: String,
    pub path: String,
    pub extension: String,
    pub size: u64,
    pub is_directory: bool,
}

pub struct CapturedFiles {
    #[allow(dead_code)]
    pub files: Vec<FileItem>,
    pub files_json: String,
    pub display_summary: String,
    pub hash: u64,
}

#[cfg(windows)]
pub fn read_clipboard_files() -> Option<CapturedFiles> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_HDROP: u32 = 15;

    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_err() {
            return None;
        }

        // Retry OpenClipboard in case another application holds it momentarily
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
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

        let handle: HANDLE = match GetClipboardData(CF_HDROP) {
            Ok(h) => h,
            Err(_) => return None,
        };

        if handle.0.is_null() {
            return None;
        }

        let hdrop = HDROP(handle.0);
        let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
        if count == 0 {
            return None;
        }

        let mut files = Vec::with_capacity(count as usize);

        for i in 0..count {
            let len = DragQueryFileW(hdrop, i, None);
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; len as usize + 1];
            let copied = DragQueryFileW(hdrop, i, Some(&mut buf));
            let path_str = String::from_utf16_lossy(&buf[..copied as usize]);

            let path = Path::new(&path_str);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone());

            let extension = if path.is_dir() {
                "folder".to_string()
            } else {
                path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default()
            };

            let (size, is_directory) = match std::fs::metadata(&path_str) {
                Ok(meta) => (meta.len(), meta.is_dir()),
                Err(_) => (0, false),
            };

            files.push(FileItem {
                name,
                path: path_str,
                extension,
                size,
                is_directory,
            });
        }

        if files.is_empty() {
            return None;
        }

        // Compute hash for deduplication
        let hash = compute_files_hash(&files);

        // Generate clean display summary
        let display_summary = if files.len() == 1 {
            let f = &files[0];
            if f.is_directory {
                format!("Folder: {}", f.name)
            } else {
                format!("{} ({})", f.name, format_file_size(f.size))
            }
        } else {
            let names: Vec<&str> = files.iter().take(3).map(|f| f.name.as_str()).collect();
            let remainder = files.len().saturating_sub(3);
            if remainder > 0 {
                format!("{} files: {}, +{} more", files.len(), names.join(", "), remainder)
            } else {
                format!("{} files: {}", files.len(), names.join(", "))
            }
        };

        let files_json = serde_json::to_string(&files).unwrap_or_else(|_| "[]".into());

        Some(CapturedFiles {
            files,
            files_json,
            display_summary,
            hash,
        })
    }
}

#[cfg(not(windows))]
pub fn read_clipboard_files() -> Option<CapturedFiles> {
    None
}

// ----------
// Native CF_HDROP Clipboard Writer
// Description: Restores the file descriptor list (CF_HDROP) onto the Windows clipboard so files can be pasted directly into File Explorer or Desktop.
// ----------

#[cfg(windows)]
pub fn write_clipboard_files(paths: &[String]) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::Shell::DROPFILES;

    const CF_HDROP: u32 = 15;

    if paths.is_empty() {
        return Ok(());
    }

    // Build the DROPFILES structure and double-null terminated wide string buffer
    let dropfiles_header_size = std::mem::size_of::<DROPFILES>();

    // Encode all paths into UTF-16 with null terminators
    let mut paths_utf16: Vec<u16> = Vec::new();
    for path in paths {
        paths_utf16.extend(path.encode_utf16());
        paths_utf16.push(0); // null terminator for this string
    }
    paths_utf16.push(0); // double-null terminator at end of list

    let total_bytes = dropfiles_header_size + paths_utf16.len() * 2;

    let mut buffer = vec![0u8; total_bytes];

    // Initialize DROPFILES header
    let dropfiles = DROPFILES {
        pFiles: dropfiles_header_size as u32,
        fWide: true.into(),
        ..Default::default()
    };

    unsafe {
        std::ptr::copy_nonoverlapping(
            &dropfiles as *const DROPFILES as *const u8,
            buffer.as_mut_ptr(),
            dropfiles_header_size,
        );

        std::ptr::copy_nonoverlapping(
            paths_utf16.as_ptr() as *const u8,
            buffer.as_mut_ptr().add(dropfiles_header_size),
            paths_utf16.len() * 2,
        );

        // Open clipboard
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
            return Err("could not open clipboard to write files".into());
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

        let hglobal = GlobalAlloc(GMEM_MOVEABLE, buffer.len())
            .map_err(|e| format!("GlobalAlloc failed: {e}"))?;

        let locked = GlobalLock(hglobal);
        if !locked.is_null() {
            std::ptr::copy_nonoverlapping(buffer.as_ptr(), locked as *mut u8, buffer.len());
            let _ = GlobalUnlock(hglobal);
            let _ = SetClipboardData(CF_HDROP, Some(HANDLE(hglobal.0)));
        }

        Ok(())
    }
}

#[cfg(not(windows))]
pub fn write_clipboard_files(_paths: &[String]) -> Result<(), String> {
    Err("CF_HDROP file restore is supported on Windows".into())
}

pub fn compute_files_hash(files: &[FileItem]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    files.len().hash(&mut hasher);
    for f in files {
        f.path.hash(&mut hasher);
        f.size.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
