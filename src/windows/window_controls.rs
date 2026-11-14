//! Commands behind the custom title bar.
//!
//! The window is created without a native frame (`no-frame: true` in
//! MainWindow.slint), because the title bar is part of the application design:
//! the "Eumgr" caption, the close and the minimize button are drawn by the UI.
//! What the UI cannot do by itself are the three things this module implements:
//! moving the window, minimizing it and closing it.
//!
//! Every command logs one line, so a log file shows whether the user closed the
//! window with the button or with the keyboard (the latter never reaches this
//! module).

use crate::logs;
use crate::windows::window_center::main_window_handle;
use std::ffi::c_void;

/// Log location shared by every line of this module.
const LOG_LOCATION: &str = "windows::window_controls";

/// `SW_MINIMIZE`: minimise the window, keeping its place in the z-order.
#[cfg(windows)]
const SW_MINIMIZE: i32 = 6;

/// `WM_NCLBUTTONDOWN`: "a non client area button went down". Windows answers it
/// by starting its own modal move loop, which is why dragging a window through
/// this message feels exactly like dragging a native title bar.
#[cfg(windows)]
const WM_NCLBUTTONDOWN: u32 = 0x00A1;

/// `HTCAPTION`: the hit test result "this point is the caption", i.e. the
/// handle the move loop grabs.
#[cfg(windows)]
const HTCAPTION: usize = 2;

/// `DWMWA_WINDOW_CORNER_PREFERENCE`: Windows 11 composition attribute that
/// decides how the window corners are drawn.
#[cfg(windows)]
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;

/// `DWMWCP_ROUND`: the normal Windows 11 corner radius (the default value a
/// window with a native frame gets).
#[cfg(windows)]
const DWMWCP_ROUND: u32 = 2;

/// `VK_LBUTTON`: the virtual key code of the left mouse button, used with
/// `GetAsyncKeyState` to find out whether the drag is still in progress.
#[cfg(windows)]
const VK_LBUTTON: i32 = 0x01;

/// How often the release watcher asks whether the left button is still held.
/// 16 ms is one frame at 60 Hz: fast enough to notice the end of a drag without
/// keeping the thread busy.
#[cfg(windows)]
const DRAG_WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// `POINT`: a pair of coordinates, in physical pixels.
#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    /// Shows (`SW_MINIMIZE`, ...) or hides (`SW_HIDE`) a window.
    fn ShowWindow(hwnd: *mut c_void, command: i32) -> i32;
    /// Releases the mouse capture of the calling thread; without it the move
    /// loop below would not receive the mouse movement.
    fn ReleaseCapture() -> i32;
    /// Queues a window message; `PostMessageW` is used instead of `SendMessageW`
    /// so the modal move loop starts only after the current UI event handler
    /// returned (calling it directly would re-enter the event loop from inside
    /// Slint's own callback).
    fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
    /// Reads the current cursor position in screen coordinates (physical
    /// pixels), which is what the move loop dragged the window by.
    fn GetCursorPos(point: *mut Point) -> i32;
    /// Converts screen coordinates into client coordinates of `hwnd`.
    fn ScreenToClient(hwnd: *mut c_void, point: *mut Point) -> i32;
    /// Reports the state of a virtual key. The move loop is over as soon as the
    /// left button is no longer held down.
    fn GetAsyncKeyState(key: i32) -> i16;
}

// Timer that watches for the end of the system move loop.
//
// Slint stops a timer when its handle is dropped, so the handle is kept here for
// as long as the drag lasts. A doc comment is not used on purpose: the macro
// would not attach it to anything and the compiler warns about it.
thread_local! {
    static DRAG_WATCHER: std::cell::RefCell<Option<slint::Timer>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
#[link(name = "dwmapi")]
unsafe extern "system" {
    /// Sets one composition attribute of a window; used here to round the
    /// corners of the frameless window.
    fn DwmSetWindowAttribute(
        hwnd: *mut c_void,
        attribute: u32,
        value: *const c_void,
        size: u32,
    ) -> i32;
}

/// Rounds the corners of the frameless window.
///
/// A window without a native frame has square corners, so the rounding is asked
/// for at the compositor instead of drawing it in the UI: drawing it would only
/// round the content while the square window edge stayed visible behind it.
///
/// **Why this is not done in Slint:** a Slint window paints an opaque rectangle,
/// so a rounded shape inside it would still leave the square corners visible.
/// The composition attribute also keeps the antialiasing and the shadow of the
/// system.
///
/// Windows 10 and older do not know the attribute; the call then fails and the
/// window simply stays square, which is why a failure is logged as `warn` and
/// not treated as an error.
#[cfg(windows)]
pub fn apply_rounded_corners() {
    let hwnd = main_window_handle();
    if hwnd.is_null() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Rounded corners skipped: the window handle is unknown.",
            "warn",
        );
        return;
    }

    let preference = DWMWCP_ROUND;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };

    if result == 0 {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Window corners rounded (DWMWCP_ROUND).",
            "info",
        );
    } else {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "The system refused to round the window corners (HRESULT 0x{:08X}); \
                 this is normal before Windows 11.",
                result
            ),
            "warn",
        );
    }
}

/// Minimizes the window (the green button of the title bar).
#[cfg(windows)]
pub fn minimize() {
    let hwnd = main_window_handle();
    if hwnd.is_null() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Minimize requested, but the window handle is unknown.",
            "err",
        );
        return;
    }

    logs::logs_output::logs_output(LOG_LOCATION, "Minimizing the main window.", "info");
    // The return value is 0 when the window was already minimised, which is not
    // an error, so it is only logged.
    let previous = unsafe { ShowWindow(hwnd, SW_MINIMIZE) };
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("ShowWindow(SW_MINIMIZE) answered {} (0 = was visible).", previous),
        "info",
    );
}

/// Starts the system move loop while the user holds the mouse (drag handle).
///
/// # Why the release has to be given back to the UI
///
/// `WM_NCLBUTTONDOWN` does not move the window by itself: Windows answers it by
/// running a *modal* loop that follows the real mouse until the button is
/// released, and that loop **consumes the mouse-up**. Slint therefore never sees
/// the release, keeps the title bar touch area grabbed and sends every later
/// pointer event to it, which looks like "the whole window can be dragged and no
/// button reacts any more".
///
/// The watcher below closes that gap: as soon as the left button is up (the
/// modal loop has ended), a synthetic `PointerReleased` is dispatched to the
/// window so the grabbed touch area finishes its gesture.
#[cfg(windows)]
pub fn begin_drag<C: slint::ComponentHandle + 'static>(app: &C) {
    let hwnd = main_window_handle();
    if hwnd.is_null() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Drag requested, but the window handle is unknown.",
            "err",
        );
        return;
    }

    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Entering the system move loop (title bar drag).",
        "info",
    );

    // A press that arrives while the previous drag is still being recovered
    // would post a second `WM_NCLBUTTONDOWN` and start a move loop with no
    // button held, so it is ignored instead of stacking two loops.
    if DRAG_WATCHER.with(|slot| slot.borrow().is_some()) {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "A drag is already being finished; this press is ignored.",
            "warn",
        );
        return;
    }

    unsafe {
        ReleaseCapture();
        PostMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION, 0);
    }
    watch_for_drag_end(app.as_weak(), hwnd as usize);
}

/// Dispatches a synthetic pointer release once the left mouse button is up.
///
/// The move loop runs its own message pump, so this timer cannot fire while the
/// user is still dragging: the first tick after the loop returned already sees a
/// released button and ends the gesture for Slint.
///
/// The component is held as a weak handle because the timer outlives the call
/// that started it; if the window is gone by then, nothing has to be released.
#[cfg(windows)]
fn watch_for_drag_end<C: slint::ComponentHandle + 'static>(app: slint::Weak<C>, hwnd: usize) {
    DRAG_WATCHER.with(|slot| {
        // A second drag before the previous watcher finished: drop the old one.
        if let Some(previous) = slot.borrow_mut().take() {
            previous.stop();
        }

        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            DRAG_WATCH_INTERVAL,
            move || {
                let button_down = unsafe { GetAsyncKeyState(VK_LBUTTON) } as u16 & 0x8000 != 0;
                if button_down {
                    return;
                }

                // The drag is over: report the release at the place where the
                // cursor ended up, in logical coordinates as Slint expects.
                let position = if let Some(app) = app.upgrade() {
                    let hwnd = hwnd as *mut c_void;
                    let mut point = Point::default();
                    let have_position = unsafe { GetCursorPos(&mut point) != 0 }
                        && unsafe { ScreenToClient(hwnd, &mut point) != 0 };
                    let scale = app.window().scale_factor();
                    if have_position {
                        slint::LogicalPosition::new(point.x as f32 / scale, point.y as f32 / scale)
                    } else {
                        slint::LogicalPosition::new(0.0, 0.0)
                    }
                } else {
                    slint::LogicalPosition::new(0.0, 0.0)
                };

                if let Some(app) = app.upgrade() {
                    app.window()
                        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                            position,
                            button: slint::platform::PointerEventButton::Left,
                        });
                }

                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    "System move loop finished; the pointer release was handed back to the UI.",
                    "info",
                );

                DRAG_WATCHER.with(|slot| {
                    if let Some(finished) = slot.borrow_mut().take() {
                        finished.stop();
                    }
                });
            },
        );
        *slot.borrow_mut() = Some(timer);
    });
}

/// Stops the event loop, which is how the window is closed.
///
/// `quit_event_loop` is used instead of hiding the window: the call returns from
/// `run_event_loop`, so the shutdown path (final configuration write, log
/// message) still runs.
#[cfg(windows)]
pub fn close() {
    logs::logs_output::logs_output(LOG_LOCATION, "Close button pressed, stopping the event loop.", "info");
    if let Err(error) = slint::quit_event_loop() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("The event loop could not be stopped: {}.", error),
            "err",
        );
    }
}

/// Non Windows fallback: the application is Windows only, but the crate has to
/// build everywhere it is checked.
#[cfg(not(windows))]
pub fn minimize() {
    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Minimizing is only implemented for Windows.",
        "warn",
    );
}

/// Non Windows fallback, see [`minimize`].
#[cfg(not(windows))]
pub fn begin_drag(_window: &slint::Window) {
    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Dragging is only implemented for Windows.",
        "warn",
    );
}

/// Non Windows fallback, see [`close`].
#[cfg(not(windows))]
pub fn close() {
    logs::logs_output::logs_output(LOG_LOCATION, "Stopping the event loop.", "info");
    if let Err(error) = slint::quit_event_loop() {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("The event loop could not be stopped: {}.", error),
            "err",
        );
    }
}

/// Non Windows fallback, see [`apply_rounded_corners`].
#[cfg(not(windows))]
pub fn apply_rounded_corners() {
    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Rounded corners are only implemented for Windows.",
        "warn",
    );
}
