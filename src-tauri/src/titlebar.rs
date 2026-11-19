//! Native title bar in the launcher's colours.
//!
//! - Windows 11: caption, text and border colours set through DWM (the system still draws
//!   the bar and its buttons, so snap layouts and resizing keep working). Windows 10 doesn't
//!   support these attributes and keeps the dark title bar from `theme: "Dark"`.
//! - macOS: `titleBarStyle: "Overlay"` in tauri.conf.json; the app's top bar sits under it.
//! - Linux: the window manager draws it; `theme: "Dark"` asks for the dark variant.

/// Midnight surface `#090d16`, text `#f1f5f9`, as COLORREF (0x00BBGGRR).
#[cfg(windows)]
const CAPTION: u32 = 0x0016_0d09;
#[cfg(windows)]
const TEXT: u32 = 0x00f9_f5f1;

#[cfg(windows)]
pub fn apply(window: &tauri::WebviewWindow) {
    use std::ffi::c_void;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attr: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd: *mut c_void = hwnd.0;
    for (attr, color) in [
        (DWMWA_CAPTION_COLOR, CAPTION),
        (DWMWA_TEXT_COLOR, TEXT),
        (DWMWA_BORDER_COLOR, CAPTION),
    ] {
        // Fails harmlessly (E_INVALIDARG) before Windows 11; nothing else to do then.
        // SAFETY: valid window handle from Tauri; `color` outlives the call; size matches.
        unsafe {
            DwmSetWindowAttribute(hwnd, attr, (&color as *const u32).cast(), 4);
        }
    }
}

#[cfg(not(windows))]
pub fn apply(_window: &tauri::WebviewWindow) {}
