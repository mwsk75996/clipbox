// ----------
// Browser Source URL Detection
// Description: Extracts the SourceURL parameter from CF_HTML clipboard format when content is copied from web browsers (Chrome, Edge, Brave, Firefox).
// ----------

#[cfg(windows)]
pub fn read_clipboard_source_url() -> Option<String> {
    use windows::core::w;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    unsafe {
        let html_format = RegisterClipboardFormatW(w!("HTML Format"));
        if html_format == 0 || IsClipboardFormatAvailable(html_format).is_err() {
            return None;
        }

        // Retry OpenClipboard in case another process holds it momentarily
        let mut opened = false;
        for _ in 0..3 {
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

        if let Ok(handle) = GetClipboardData(html_format) {
            if !handle.is_invalid() {
                let hglobal = HGLOBAL(handle.0 as *mut _);
                let size = GlobalSize(hglobal);
                if size > 0 {
                    let locked = GlobalLock(hglobal);
                    if !locked.is_null() {
                        let bytes = std::slice::from_raw_parts(locked as *const u8, size);
                        let text = String::from_utf8_lossy(bytes);
                        let _ = GlobalUnlock(hglobal);

                        return extract_source_url(&text);
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(windows))]
pub fn read_clipboard_source_url() -> Option<String> {
    None
}

/// Parse the SourceURL parameter from CF_HTML header text.
pub fn extract_source_url(html_content: &str) -> Option<String> {
    for line in html_content.lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 10 && trimmed[..10].eq_ignore_ascii_case("sourceurl:") {
            let url = trimmed[10..].trim();
            // Validate that it begins with a supported web protocol and contains no null bytes
            if (url.starts_with("http://") || url.starts_with("https://"))
                && !url.contains('\0')
                && url.len() > 8
            {
                return Some(url.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_source_url_from_standard_cf_html_header() {
        let header = "Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000418\r\nStartFragment:0000000141\r\nEndFragment:0000000382\r\nSourceURL:https://github.com/mwsk75996/clipbox/issues/3\r\n<html><body>Test</body></html>";
        assert_eq!(
            extract_source_url(header),
            Some("https://github.com/mwsk75996/clipbox/issues/3".to_string())
        );
    }

    #[test]
    fn handles_case_insensitive_source_url() {
        let header = "Version:1.0\nsourceurl: https://developer.mozilla.org/en-US/\n<html>";
        assert_eq!(
            extract_source_url(header),
            Some("https://developer.mozilla.org/en-US/".to_string())
        );
    }

    #[test]
    fn returns_none_when_source_url_is_missing_or_malformed() {
        assert_eq!(extract_source_url("Version:0.9\nStartHTML:0001"), None);
        assert_eq!(extract_source_url("SourceURL:"), None);
        assert_eq!(extract_source_url("SourceURL:   "), None);
        assert_eq!(extract_source_url("SourceURL:javascript:alert(1)"), None);
        assert_eq!(extract_source_url(""), None);
    }
}
