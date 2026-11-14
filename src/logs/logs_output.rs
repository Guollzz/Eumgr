//! Formatting and dispatching of log lines.
//!
//! Every line goes to two sinks: the log file (via [`crate::logs::logs_path`])
//! and stdout with a colour per level, so that a debug build shows the same
//! information in the console.

use crate::logs::logs_path::write_logs;
use chrono::Local;
use colored::*;

/// Writes one log line.
///
/// * `location` - `module::function` (or `module::Type::method`) that logs; it is
///   part of the line so a log file can be grepped per code path.
/// * `msg` - human readable description of what happened.
/// * `logs_type` - one of `"info"`, `"warn"`, `"err"`; anything else is written
///   as an error about the wrong type, which keeps the log parseable.
pub fn logs_output(location: &str, msg: &str, logs_type: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    match logs_type {
        "info" => {
            let log_entry = format!("[{}] {}(EunjonManager):[INFO] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[INFO] {}", timestamp, location, msg);
            write_logs(&log_entry);
            println!("{}", colored_msg.white());
        }
        "warn" => {
            let log_entry = format!("[{}] {}(EunjonManager):[WARNING] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[WARNING] {}", timestamp, location, msg);
            println!("{}", colored_msg.yellow());
            write_logs(&log_entry);
        }
        "err" => {
            let log_entry = format!("[{}] {}(EunjonManager):[ERROR] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[ERROR] {}", timestamp, location, msg);
            println!("{}", colored_msg.red());
            write_logs(&log_entry);
        }
        _ => {
            let log_entry = format!("[{}] {}(EunjonManager):[INCORRECT INFO TYPE] {}\n", timestamp, location, msg);
            let colored_msg = format!("[{}] {}(EunjonManager):[INCORRECT INFO TYPE] {}", timestamp, location, msg);
            println!("{}", colored_msg.green());
            write_logs(&log_entry);
        }
    }
}
