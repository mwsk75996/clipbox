// ----------
// Application Autostart Manager
// Description: Manages OS boot launch registration on Windows via the CurrentVersion\Run registry key.
// ----------

pub fn is_autostart_enabled() -> Result<bool, String> {
    #[cfg(windows)]
    {
        windows_autostart::is_enabled()
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows_autostart::set_enabled(enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[cfg(windows)]
mod windows_autostart {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    const RUN_KEY_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const APP_VALUE_NAME: &str = "Clipbox";

    fn to_wide(s: &str) -> Vec<u16> {
        let mut v: Vec<u16> = OsStr::new(s).encode_wide().collect();
        v.push(0);
        v
    }

    struct KeyGuard(HKEY);
    impl Drop for KeyGuard {
        fn drop(&mut self) {
            if !self.0 .0.is_null() {
                unsafe {
                    let _ = RegCloseKey(self.0);
                }
            }
        }
    }

    pub fn is_enabled() -> Result<bool, String> {
        let subkey = to_wide(RUN_KEY_PATH);
        let value_name = to_wide(APP_VALUE_NAME);

        unsafe {
            let mut hkey = HKEY::default();
            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                KEY_QUERY_VALUE,
                &mut hkey,
            );

            if status != ERROR_SUCCESS {
                return Ok(false);
            }

            let _guard = KeyGuard(hkey);

            let status = RegQueryValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                None,
                None,
                None,
                None,
            );

            if status == ERROR_SUCCESS {
                Ok(true)
            } else if status == ERROR_FILE_NOT_FOUND {
                Ok(false)
            } else {
                Err(format!("registry query error: {status:?}"))
            }
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let subkey = to_wide(RUN_KEY_PATH);
        let value_name = to_wide(APP_VALUE_NAME);

        if enabled {
            let exe_path = std::env::current_exe()
                .map_err(|e| format!("could not determine current executable path: {e}"))?;
            let cmd = format!("\"{}\"", exe_path.to_string_lossy());
            let cmd_wide = to_wide(&cmd);
            let cmd_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    cmd_wide.as_ptr() as *const u8,
                    cmd_wide.len() * std::mem::size_of::<u16>(),
                )
            };

            unsafe {
                let mut hkey = HKEY::default();
                let status = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(subkey.as_ptr()),
                    None,
                    KEY_SET_VALUE,
                    &mut hkey,
                );

                if status != ERROR_SUCCESS {
                    return Err(format!("could not open Run registry key: {status:?}"));
                }

                let _guard = KeyGuard(hkey);

                let status = RegSetValueExW(
                    hkey,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(cmd_bytes),
                );

                if status != ERROR_SUCCESS {
                    return Err(format!("could not set Run registry value: {status:?}"));
                }

                Ok(())
            }
        } else {
            unsafe {
                let mut hkey = HKEY::default();
                let status = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(subkey.as_ptr()),
                    None,
                    KEY_SET_VALUE,
                    &mut hkey,
                );

                if status != ERROR_SUCCESS {
                    return Ok(());
                }

                let _guard = KeyGuard(hkey);

                let status = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
                if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                    return Err(format!("could not delete Run registry value: {status:?}"));
                }

                Ok(())
            }
        }
    }
}
