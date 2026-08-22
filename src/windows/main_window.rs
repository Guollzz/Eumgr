use crate::logs;
use crate::{VERSION};

slint::include_modules!();

pub fn main_window() {
    logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow starting.", "info");
    let app = MainWindow::new().unwrap();
    app.set_window_title(format!("Eumgr v{}", VERSION).into());
    app.run().unwrap();
    logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow stopping.", "info");
}