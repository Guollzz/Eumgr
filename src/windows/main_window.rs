use crate::{logs, pages::settings_page};
use slint::{LogicalPosition, ComponentHandle};
use crate::i18n::i18n::Locale;
use chrono::Local;
use slint::Timer;

slint::include_modules!();

fn update_time(app: &MainWindow) {
    let now = Local::now();
    let time_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
    app.set_dynamic_island_text(time_str.into());
}

pub fn main_window() {
    logs::logs_output::logs_output("windows::main_window::main_window", "MainWindow starting.", "info");

    let app = MainWindow::new().unwrap();

    update_time(&app);  // Update time immediately once.
    let handle2 = app.as_weak();
    let timer = Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            if let Some(app) = handle2.upgrade() {
                update_time(&app);
            }
        }
    );


    let mut current_locale = Locale::load("zh-CN").unwrap();
    let update_app_texts = |app: &MainWindow, loc: &Locale| {
        app.set_workspace(loc.t("workspace").into());
        app.set_settings(loc.t("settings").into());
        app.set_general(loc.t("general").into());
        app.set_about(loc.t("about").into());
        logs::logs_output::logs_output("windows::main_window::main_window", "Language loaded.", "info");
    };
    update_app_texts(&app, &current_locale);

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