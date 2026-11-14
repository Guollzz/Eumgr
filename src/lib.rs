//! Application entry point: initialises logging, loads the configuration and
//! starts the main window.
//!
//! Keep the order of the steps in [`run`] as it is: logging has to be ready
//! before anything else can report a problem, and the configuration has to be
//! loaded before the window is created so the first paint already uses the
//! stored language.

mod config;
mod i18n;
mod launchpad;
mod logs;
mod windows;
mod workspace;

use std::panic;

/// `true` in real builds. When set to `false` the version string becomes a
/// placeholder, which is handy while developing a release.
const IS_RELEASE: bool = true;

/// Version reported in the About page and in the start-up log lines.
pub const VERSION: &str = if IS_RELEASE {
    env!("CARGO_PKG_VERSION")
} else {
    "x.x-pre-1"
};

/// Starts the application and blocks until the last window is closed.
pub fn run() {
    // Panics are the one failure mode that cannot log itself: route them into
    // the log file so a crash leaves a trace instead of a silent exit.
    panic::set_hook(Box::new(|panic_info| {
        logs::logs_output::logs_output("panic", &format!("{}", panic_info), "err");
    }));

    // Slint and friends report through `log`; forward their warnings/errors.
    logs::worring::set_warning_hook(|message| {
        logs::logs_output::logs_output("warning", message, "warn");
    });

    let log_path = logs::logs_path::init_log_file();
    logs::logs_output::logs_output("lib::run", "Log file created successfully.", "info");
    // Absolute path plus working directory: without both it is hard to tell
    // where the file ended up when the app is started from a shortcut.
    logs::logs_output::logs_output(
        "lib::run",
        &format!(
            "Log file: {:?}",
            std::fs::canonicalize(&log_path).unwrap_or(log_path)
        ),
        "info",
    );
    if let Ok(current_dir) = std::env::current_dir() {
        logs::logs_output::logs_output(
            "lib::run",
            &format!("Working directory: {:?}", current_dir),
            "info",
        );
    }
    logs::logs_output::logs_output("lib::run", "Starting!", "info");
    logs::logs_output::logs_output(
        "lib::run",
        &format!("Version: {} (IS_RELEASE = {}).", VERSION, IS_RELEASE),
        "info",
    );

    // `colored` needs the virtual terminal escape sequences enabled, otherwise
    // the console shows raw escape codes. Piped output (a shortcut, a log
    // collector) does not support it; that is not fatal, the file stays plain.
    #[cfg(windows)]
    if colored::control::set_virtual_terminal(true).is_ok() {
        logs::logs_output::logs_output(
            "lib::run",
            "Coloured console output enabled.",
            "info",
        );
    } else {
        logs::logs_output::logs_output(
            "lib::run",
            "Coloured console output unavailable, console output stays plain.",
            "warn",
        );
    }

    // `main_window` loads (or creates) the configuration again, this early call
    // only makes sure the start-up log says whether a config file existed.
    match config::Config::load_or_create("config.json") {
        Ok(loaded) => logs::logs_output::logs_output(
            "lib::run",
            &format!("Configuration ready, language = \"{}\".", loaded.language),
            "info",
        ),
        Err(error) => logs::logs_output::logs_output(
            "lib::run",
            &format!("Configuration could not be loaded: {}.", error),
            "err",
        ),
    }

    if let Err(error) = windows::main_window::main_window() {
        logs::logs_output::logs_output(
            "lib::run",
            &format!("Main window stopped with an error: {}.", error),
            "err",
        );
    }

    logs::logs_output::logs_output("lib::run", "Stopping!", "info");
}
