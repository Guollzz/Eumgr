//! Starts a launchpad entry.
//!
//! `ShellExecuteW` is used instead of `std::process::Command` because it lets
//! Windows decide what "open" means for a path: an `.exe` is executed, a
//! document is opened with its registered application, a folder opens in
//! Explorer and a shortcut follows its target. `Command` would only handle the
//! executable case.

use crate::logs;
use std::ffi::c_void;

/// Log location shared by every line of this module.
const LOG_LOCATION: &str = "launchpad::launcher";

/// `SW_SHOWNORMAL`: give the started window its normal size and position.
#[cfg(windows)]
const SW_SHOWNORMAL: i32 = 1;

/// Return values of `ShellExecuteW` above this threshold mean success; the
/// values below are the historical error codes (0, 2, 3, 5, 8, ...).
#[cfg(windows)]
const SHELL_EXECUTE_SUCCESS: isize = 32;

/// Starts `path` with its default handler.
///
/// The result is logged by the caller (it knows the entry name); this function
/// only reports the failure text, which contains the raw `ShellExecuteW` code so
/// a log file shows whether Windows refused the path or the handler was missing.
///
/// Folders take a separate route: `ShellExecuteW("open")` hands them to the
/// already running Explorer, which often reuses an existing window *behind* the
/// application, so the click looks like it did nothing. Starting `explorer.exe`
/// with the folder as its argument always opens (or activates) a window in front.
#[cfg(windows)]
pub fn open(path: &str) -> Result<(), String> {
    if std::path::Path::new(path).is_dir() {
        open_folder(path)
    } else {
        open_with_shell(path)
    }
}

/// Opens a folder in a new Explorer window that is guaranteed to come to front.
#[cfg(windows)]
fn open_folder(path: &str) -> Result<(), String> {
    match std::process::Command::new("explorer.exe").arg(path).spawn() {
        Ok(_child) => Ok(()),
        Err(error) => Err(format!("explorer.exe \"{}\" failed: {}.", path, error)),
    }
}

/// Opens a file (or a shortcut) through the shell so its registered handler runs.
#[cfg(windows)]
fn open_with_shell(path: &str) -> Result<(), String> {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let operation = wide("open");
    let file = wide(path);

    // No owner window and no parameters: the started process is independent of
    // the application, so closing Eumgr does not close what it started.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if result > SHELL_EXECUTE_SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "ShellExecuteW(\"{}\") returned code {}.",
            path, result
        ))
    }
}

/// Non Windows fallback: the application is Windows only, but the module has to
/// build everywhere the crate is checked.
#[cfg(not(windows))]
pub fn open(path: &str) -> Result<(), String> {
    Err(format!("Opening \"{}\" is not supported on this platform.", path))
}

/// Logs the outcome of a launch attempt in one place, so every tile click and
/// every entry of a continuous launch produces the same, greppable line.
pub fn log_outcome(entry: &str, path: &str, result: &Result<(), String>) {
    match result {
        Ok(()) => logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Started \"{}\" ({}).", entry, path),
            "info",
        ),
        Err(error) => logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Starting \"{}\" ({}) failed: {}.", entry, path, error),
            "err",
        ),
    }
}
