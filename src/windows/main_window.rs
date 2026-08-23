use crate::logs;
use slint::{LogicalPosition, ComponentHandle};

slint::include_modules!();

pub fn main_window() {
    logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow starting.", "info");

    let app = MainWindow::new().unwrap();

    let app_handle = app.as_weak(); // Get a clone of the window handle for subsequent operations.

    app.on_close_requested(move || {
        logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow closed.", "info");
        slint::quit_event_loop().ok();
    });

    app.on_minimize_requested(move || {
        logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow minimized.", "info");
        let Some(inst) = app_handle.upgrade() else {return;};
        let _ = inst.window().set_minimized(true);
    });
    
    let handle1 = app.as_weak();
    app.on_move_window(move |offset_x: f32, offset_y: f32| {
        if let Some(app) = handle1.upgrade() {
            let window = app.window();
            let current_pos = window.position();
            let new_x = current_pos.x as f32 + offset_x;
            let new_y = current_pos.y as f32 + offset_y;
            window.set_position(LogicalPosition::new(new_x, new_y));
        }
    });

    app.run().unwrap();

    logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow stopping.", "info");
}