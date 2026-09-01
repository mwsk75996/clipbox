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
    use std::path::Path;

    use clipbox_core::ClipboardMetadata;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

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

        ClipboardMetadata {
            source_app,
            source_process,
            window_title: window_title(window),
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
}
