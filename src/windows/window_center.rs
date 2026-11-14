//! Places the main window in the middle of the screen when it is first shown.
//!
//! Slint has no "center me" flag, so the position is computed here: find the
//! native window of this process, read its real outer size (including the
//! frame), read the work area of the monitor it is on (excluding the taskbar)
//! and put the window in the middle of that area.

use slint::Window;

/// Centers `window` inside the work area of the monitor it belongs to.
///
/// Returns `false` when the native window is not created/laid out yet (the
/// window only gets its size once the event loop runs), so that the caller can
/// retry. Returns `true` once the position was applied.
pub fn center_window(window: &Window) -> bool {
    let size = window.size();
    let client_width = size.width as i32;
    let client_height = size.height as i32;

    #[cfg(windows)]
    {
        let Some((work_area, outer)) = win32::main_window_rect() else {
            return false;
        };

        // Use the outer rect (frame included) but never less than the client
        // size, in case the frame is reported before the first layout.
        let width = (outer.right - outer.left).max(client_width);
        let height = (outer.bottom - outer.top).max(client_height);
        if width <= 0 || height <= 0 {
            return false;
        }

        // Integer division: an odd leftover of one pixel is rounded down, which
        // is what Windows itself does.
        let x = work_area.left + ((work_area.right - work_area.left) - width) / 2;
        let y = work_area.top + ((work_area.bottom - work_area.top) - height) / 2;

        crate::logs::logs_output::logs_output(
            "windows::window_center::center_window",
            &format!(
                "window {}x{} at ({},{}), centered at ({},{}).",
                width, height, outer.left, outer.top, x, y
            ),
            "info",
        );

        // `set_position` takes physical pixels, exactly like GetWindowRect and
        // the monitor rectangle, so no DPI conversion is needed here.
        window.set_position(slint::PhysicalPosition::new(x, y));
        true
    }

    #[cfg(not(windows))]
    {
        // No Win32 available: keep the platform default position.
        let _ = (client_width, client_height);
        true
    }
}

/// Handle of this process' main window, or null before the window exists.
///
/// Needed by everything that has to be modal to the main window: the native
/// file and folder pickers of the launchpad pass it as their owner, which is
/// what keeps them in front of Eumgr instead of behind it.
#[cfg(windows)]
pub fn main_window_handle() -> *mut std::ffi::c_void {
    win32::find_main_window()
}

/// Non Windows fallback: there is no handle to hand out.
#[cfg(not(windows))]
pub fn main_window_handle() -> *mut std::ffi::c_void {
    std::ptr::null_mut()
}

/// Minimal Win32 bindings, declared by hand to avoid new dependencies.
#[cfg(windows)]
mod win32 {
    use std::cell::Cell;
    use std::ffi::c_void;

    /// `RECT` from `windef.h`.
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    /// `MONITORINFO` from `winuser.h` (the tail is only used internally).
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct MonitorInfo {
        cb_size: u32,
        rc_monitor: Rect,
        rc_work: Rect,
        flags: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        /// Enumerates every top level window of the desktop.
        fn EnumWindows(callback: extern "system" fn(*mut c_void, isize) -> i32, lparam: isize) -> i32;
        /// Returns the process id that owns a window.
        fn GetWindowThreadProcessId(hwnd: *mut c_void, process_id: *mut u32) -> u32;
        /// Tells whether a window is currently visible.
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
        /// Outer rectangle of a window, frame included, in physical pixels.
        fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
        /// Monitor that is nearest to (or contains) the window.
        fn MonitorFromWindow(hwnd: *mut c_void, flags: u32) -> *mut c_void;
        /// Fills `MONITORINFO`, including the work area without the taskbar.
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    /// Windows smaller than this are helper/popup windows and never the main
    /// one (300'000px² is roughly a 548x548 box, far below the 1216x749 frame).
    const MIN_AREA: i64 = 300_000;

    thread_local! {
        /// Window found by the running `EnumWindows` pass: (hwnd, area, visible).
        static CANDIDATE: Cell<(*mut c_void, i64, bool)> =
            const { Cell::new((std::ptr::null_mut(), 0, false)) };
    }

    /// Callback of `EnumWindows`: remembers the best matching window of this
    /// process, preferring a visible window over a larger hidden one.
    extern "system" fn enum_callback(hwnd: *mut c_void, _lparam: isize) -> i32 {
        unsafe {
            let mut process_id = 0u32;
            GetWindowThreadProcessId(hwnd, &mut process_id);
            if process_id != GetCurrentProcessId() {
                return 1; // Not ours, keep enumerating.
            }

            let mut rect = Rect::default();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return 1;
            }

            let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
            if area <= MIN_AREA {
                return 1; // Tool window / message-only window.
            }

            let visible = IsWindowVisible(hwnd) != 0;
            CANDIDATE.with(|candidate| {
                let (_, current_area, current_visible) = candidate.get();
                let better = match (visible, current_visible) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => area > current_area,
                };
                if better {
                    candidate.set((hwnd, area, visible));
                }
            });

            1 // Continue enumeration.
        }
    }

    /// Returns the handle of the largest visible window of this process, or
    /// null when the window is not created yet.
    ///
    /// Used as the owner of modal dialogs; it deliberately shares the search of
    /// [`main_window_rect`] so both always talk about the same window.
    pub fn find_main_window() -> *mut c_void {
        unsafe {
            CANDIDATE.with(|candidate| candidate.set((std::ptr::null_mut(), 0, false)));
            EnumWindows(enum_callback, 0);
            CANDIDATE.with(|candidate| candidate.get().0)
        }
    }

    /// Returns the work area of the monitor holding this process' main window
    /// together with the current outer rect of that window.
    pub fn main_window_rect() -> Option<(Rect, Rect)> {
        unsafe {
            CANDIDATE.with(|candidate| candidate.set((std::ptr::null_mut(), 0, false)));
            EnumWindows(enum_callback, 0);

            let hwnd = CANDIDATE.with(|candidate| candidate.get().0);
            if hwnd.is_null() {
                return None; // Window not created yet.
            }

            let mut outer = Rect::default();
            if GetWindowRect(hwnd, &mut outer) == 0 {
                return None;
            }
            if outer.right - outer.left <= 0 || outer.bottom - outer.top <= 0 {
                return None; // Created but not laid out yet (0x0).
            }

            let work_area = monitor_work_area(hwnd)?;
            Some((work_area, outer))
        }
    }

    /// Work area of the monitor the window is on.
    fn monitor_work_area(hwnd: *mut c_void) -> Option<Rect> {
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor.is_null() {
                return None;
            }

            let mut info = MonitorInfo {
                cb_size: std::mem::size_of::<MonitorInfo>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return None;
            }

            Some(info.rc_work)
        }
    }
}
