use rust_browser::settings::Theme;
use winit::{
    platform::windows::{Color, CornerPreference, WindowExtWindows},
    window::Window,
};

fn colors(theme: Theme) -> ([u8; 3], [u8; 3]) {
    match theme {
        Theme::Dark => ([0x13, 0x15, 0x18], [0xE9, 0xEC, 0xEF]),
        Theme::Black => ([0x00, 0x00, 0x00], [0xE9, 0xEC, 0xEF]),
        Theme::Light => ([0xD9, 0xDE, 0xE3], [0x1B, 0x24, 0x30]),
        Theme::Slate => ([0x14, 0x1C, 0x23], [0xE9, 0xEC, 0xEF]),
        Theme::Plum => ([0x1B, 0x14, 0x20], [0xE9, 0xEC, 0xEF]),
    }
}

pub fn apply_theme(window: &Window, theme: Theme) {
    let (background, foreground) = colors(theme);
    let background = Color::from_rgb(background[0], background[1], background[2]);
    window.set_title_background_color(Some(background));
    window.set_border_color(Some(background));
    window.set_title_text_color(Color::from_rgb(foreground[0], foreground[1], foreground[2]));
    window.set_corner_preference(CornerPreference::Round);
}

/// Give a borderless window Windows' own resize borders on its left, right and bottom.
///
/// winit's borderless windows keep `WS_CAPTION | WS_SIZEBOX` but answer
/// `WM_NCCALCSIZE` by making the whole window the client area, so the page views cover
/// every edge and there is nothing to drag. While the window is restored, this subclass
/// lets Windows compute the standard frame instead (on Windows 10 and 11 its side and
/// bottom borders are invisible, just outside the drawn window, like other apps') and
/// pulls only the top back up to the window's edge, where the toolbar draws the
/// caption and its own strip resizes the top edge. Maximized windows are left to
/// winit; full-screen ones fill the monitor (see [`set_fullscreen_frame`]). Returns
/// whether the subclass was installed.
pub fn enable_edge_resize(window: &Window) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::{
            Shell::SetWindowSubclass,
            WindowsAndMessaging::{
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SetWindowPos,
            },
        },
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let hwnd = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => HWND(handle.hwnd.get() as *mut _),
        _ => return false,
    };
    // SAFETY: `hwnd` is this thread's own live window (winit creates it on the event
    // loop thread, which is the only caller); the subclass procedure is a plain
    // function with no data, removed by Windows when the window is destroyed.
    unsafe {
        if !SetWindowSubclass(hwnd, Some(edge_resize_proc), EDGE_RESIZE_ID, 0).as_bool() {
            return false;
        }
        // Recompute the frame now that the borders exist.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    true
}

const EDGE_RESIZE_ID: usize = 0x5242_4544; // "RBED"

/// Tell the edge-resize subclass whether the window is full screen, and recompute
/// its frame.
///
/// winit keeps a maximized window marked maximized while it is full screen and then
/// shrinks its client area to the monitor's work area, so the taskbar stays and the
/// page stops above it. While this is on, the subclass makes the whole window the
/// client area instead: a full-screen page covers the entire monitor. Call it with
/// `true` before entering full screen and with `false` before leaving it. Returns
/// whether the subclass (installed by [`enable_edge_resize`]) took the change.
pub fn set_fullscreen_frame(window: &Window, fullscreen: bool) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        UI::{
            Shell::SetWindowSubclass,
            WindowsAndMessaging::{
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SetWindowPos,
            },
        },
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let hwnd = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => HWND(handle.hwnd.get() as *mut _),
        _ => return false,
    };
    // SAFETY: this thread's own live window, as in `enable_edge_resize`. Calling
    // SetWindowSubclass again with the same procedure and ID only replaces its
    // reference data (the full-screen flag).
    unsafe {
        if !SetWindowSubclass(
            hwnd,
            Some(edge_resize_proc),
            EDGE_RESIZE_ID,
            usize::from(fullscreen),
        )
        .as_bool()
        {
            return false;
        }
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    true
}

/// The subclass behind [`enable_edge_resize`].
///
/// # Safety
/// Called by Windows with a valid window and, for `WM_NCCALCSIZE` with a non-zero
/// `wparam`, an `lparam` pointing at a live `NCCALCSIZE_PARAMS`.
unsafe extern "system" fn edge_resize_proc(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _id: usize,
    fullscreen: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::{
        Shell::DefSubclassProc,
        WindowsAndMessaging::{
            DefWindowProcW, GWL_STYLE, GetWindowLongW, IsIconic, IsZoomed, NCCALCSIZE_PARAMS,
            WM_NCCALCSIZE, WS_CAPTION, WS_SIZEBOX,
        },
    };
    // SAFETY: see the function's contract; every call here takes that window.
    unsafe {
        // Full screen: the whole window is the client area, whatever winit would
        // trim from a maximized window (see `set_fullscreen_frame`).
        if message == WM_NCCALCSIZE && wparam.0 != 0 && fullscreen != 0 {
            return windows::Win32::Foundation::LRESULT(0);
        }
        if message == WM_NCCALCSIZE
            && wparam.0 != 0
            && !IsZoomed(hwnd).as_bool()
            && !IsIconic(hwnd).as_bool()
        {
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            // Full screen drops the caption and sizing styles: leave it to winit.
            if style & WS_SIZEBOX.0 != 0 && style & WS_CAPTION.0 == WS_CAPTION.0 {
                let params = lparam.0 as *mut NCCALCSIZE_PARAMS;
                let top = (*params).rgrc[0].top;
                let result = DefWindowProcW(hwnd, message, wparam, lparam);
                (*params).rgrc[0].top = top;
                return result;
            }
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance([red, green, blue]: [u8; 3]) -> f64 {
        let linear = [red, green, blue].map(|channel| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    #[test]
    fn caption_palette_matches_each_toolbar_theme() {
        assert_eq!(
            colors(Theme::Dark),
            ([0x13, 0x15, 0x18], [0xE9, 0xEC, 0xEF])
        );
        assert_eq!(
            colors(Theme::Light),
            ([0xD9, 0xDE, 0xE3], [0x1B, 0x24, 0x30])
        );
        assert_eq!(
            colors(Theme::Slate),
            ([0x14, 0x1C, 0x23], [0xE9, 0xEC, 0xEF])
        );
        assert_eq!(
            colors(Theme::Plum),
            ([0x1B, 0x14, 0x20], [0xE9, 0xEC, 0xEF])
        );
        assert_eq!(
            colors(Theme::Black),
            ([0x00, 0x00, 0x00], [0xE9, 0xEC, 0xEF])
        );
    }

    #[test]
    fn caption_text_meets_normal_text_contrast_in_each_theme() {
        for theme in [
            Theme::Dark,
            Theme::Black,
            Theme::Light,
            Theme::Slate,
            Theme::Plum,
        ] {
            let (background, foreground) = colors(theme);
            let background = luminance(background);
            let foreground = luminance(foreground);
            let ratio = (background.max(foreground) + 0.05) / (background.min(foreground) + 0.05);
            assert!(
                ratio >= 4.5,
                "{theme:?} caption text contrast is {ratio:.2}:1"
            );
        }
    }
}
