use colored::*;
use chrono::Local;
use crate::logs::logs_path::write_logs;

pub fn logs_output(location: &str, msg: &str, logs_type: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    match logs_type {
        "info" => {
            let log_entry = format!("[{}] {}(EunjonManager):[INFO] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[INFO] {}", timestamp, location, msg);
            write_logs(&log_entry);
            println!("{}", colored_msg.white());
            
        },
        "warn" => {
            let log_entry = format!("[{}] {}(EunjonManager):[WARNING] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[WARNING] {}", timestamp, location, msg);
            println!("{}", colored_msg.yellow());
            write_logs(&log_entry);
        },
        "err" => {
            let log_entry = format!("[{}] {}(EunjonManager):[ERROR] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[ERROR] {}", timestamp, location, msg);
            println!("{}", colored_msg.red());
            write_logs(&log_entry);
        },
        _ => {
            let log_entry = format!("[{}] {}(EunjonManager):[INCORRECT INFO TYPE] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[INCORRECT INFO TYPE] {}", timestamp, location, msg);
            println!("{}", colored_msg.green());
            write_logs(&log_entry);
        },
    }
}