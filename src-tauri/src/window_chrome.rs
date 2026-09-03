// ----------
// Native Windows Titlebar Hit Testing
// Description: Marks the custom HTML titlebar as a native caption area so Windows handles dragging and double-click maximize directly, without a JavaScript or IPC round trip.
// ----------

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::ScreenToClient;
#[cfg(windows)]
use windows::Win32::UI::HiDpi::GetDpiForWindow;
#[cfg(windows)]
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsZoomed, HTCAPTION, WM_NCDESTROY, WM_NCHITTEST,
};

#[cfg(windows)]
const SUBCLASS_ID: usize = 0x434C_4950;
#[cfg(windows)]
const TITLEBAR_HEIGHT_CSS_PX: i32 = 36;
#[cfg(windows)]
const WINDOW_CONTROLS_WIDTH_CSS_PX: i32 = 132;
#[cfg(windows)]
const RESIZE_BORDER_CSS_PX: i32 = 5;

#[cfg(windows)]
fn scale_css_pixels(hwnd: HWND, value: i32) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i32;
    (value * dpi + 95) / 96
}

#[cfg(windows)]
unsafe extern "system" fn titlebar_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    _reference_data: usize,
) -> LRESULT {
    if message == WM_NCHITTEST {
        let mut point = POINT {
            x: lparam.0 as i16 as i32,
            y: (lparam.0 >> 16) as i16 as i32,
        };
        let mut client_rect = RECT::default();

        if ScreenToClient(hwnd, &mut point).as_bool() && GetClientRect(hwnd, &mut client_rect).is_ok() {
            let titlebar_height = scale_css_pixels(hwnd, TITLEBAR_HEIGHT_CSS_PX);
            let controls_width = scale_css_pixels(hwnd, WINDOW_CONTROLS_WIDTH_CSS_PX);
            let resize_border = if IsZoomed(hwnd).as_bool() {
                0
            } else {
                scale_css_pixels(hwnd, RESIZE_BORDER_CSS_PX)
            };

            if point.y >= resize_border
                && point.y < titlebar_height
                && point.x >= resize_border
                && point.x < client_rect.right - controls_width
            {
                return LRESULT(HTCAPTION as isize);
            }
        }
    } else if message == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(titlebar_subclass_proc), subclass_id);
    }

    DefSubclassProc(hwnd, message, wparam, lparam)
}

#[cfg(windows)]
pub fn install(window: &tauri::WebviewWindow) -> Result<(), String> {
    let raw_hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let hwnd = HWND(raw_hwnd.0 as _);
    let installed = unsafe {
        SetWindowSubclass(hwnd, Some(titlebar_subclass_proc), SUBCLASS_ID, 0).as_bool()
    };

    if installed {
        Ok(())
    } else {
        Err(windows::core::Error::from_win32().to_string())
    }
}

#[cfg(not(windows))]
pub fn install(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}
