mod logs;
mod windows;
use std::panic;
mod i18n;

const IS_RELEASE: bool = true;
pub const VERSION: &str = if IS_RELEASE {
    env!("CARGO_PKG_VERSION")
} else {
    "x.x-pre-1"
};

pub fn run() {
    panic::set_hook(Box::new(|panic_info| {
        logs::logs_output::logs_output("panic", &format!("{}", panic_info), "err");
    }));
    logs::worring::set_warning_hook(|_msg| {
        logs::logs_output::logs_output("warning", &_msg, "warn");
    });

    logs::logs_path::init_log_file();
    logs::logs_output::logs_output("lib::run", "Log file created successfully.", "info");
    logs::logs_output::logs_output("lib::run", "Starting!", "info");
    logs::logs_output::logs_output("lib::run", &format!("Version: {}", VERSION), "info");

    windows::main_window::main_window();

    logs::logs_output::logs_output("lib::run", "Stopping!", "info");
}