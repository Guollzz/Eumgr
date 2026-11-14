//! Management of the log file itself.
//!
//! One file per run: `logs/log_<timestamp>.log`, created next to the working
//! directory of the process. The handle is kept in a global so every module can
//! write without passing it around.

use chrono::Local;
use once_cell::sync::Lazy;
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Mutex;

/// Currently opened log file. `None` until [`init_log_file`] ran, which makes
/// [`write_logs`] a no-op (log calls before initialisation are simply dropped).
static LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

/// Creates the log file for this run and makes it the active log target.
///
/// Returns the path of the created file so the caller can log it; that one
/// line is usually the most useful information when a user reports a problem.
pub fn init_log_file() -> PathBuf {
    let timestamp = Local::now().format("%Y-%m-%d-%H-%M-%S");
    let log_filename = format!("log_{}.log", timestamp);
    let log_dir = PathBuf::from("logs");

    // `create_dir_all` also succeeds when the folder already exists, which is
    // the normal case for every run after the first one.
    if !log_dir.exists() {
        fs::create_dir_all(&log_dir).expect("Failed to create logs folder.");
    }

    let log_file_path = log_dir.join(log_filename);
    let log_file = fs::File::create(&log_file_path).expect("Failed to create log file.");

    // Take the lock only for the assignment, so a slow disk does not block the
    // other threads longer than necessary.
    {
        let mut guard = LOG_FILE.lock().unwrap();
        *guard = Some(log_file);
    }

    log_file_path
}

/// Folder that holds the log file of the current run and of every older run.
///
/// The path is returned as an absolute one when that is possible, because it is
/// also handed to Explorer (the "open log folder" button): Explorer resolves a
/// relative path against its own working directory, not against ours. The
/// `\\?\` prefix that `canonicalize` adds on Windows is removed again, as
/// Explorer does not accept it.
pub fn log_directory() -> PathBuf {
    let relative = PathBuf::from("logs");
    match fs::canonicalize(&relative) {
        Ok(absolute) => {
            let text = absolute.to_string_lossy();
            let trimmed = text.strip_prefix(r"\\?\").unwrap_or(&text);
            PathBuf::from(trimmed.to_string())
        }
        // The folder does not exist yet (or the drive is unavailable); the
        // relative path is still better than nothing.
        Err(_) => relative,
    }
}

/// Appends one already formatted line to the log file and flushes it.
///
/// Flushing on every line keeps the file usable when the process is killed
/// instead of shutting down cleanly, at the cost of one write syscall per line.
pub fn write_logs(content: &str) {
    let mut guard = LOG_FILE.lock().unwrap();
    if let Some(file) = guard.as_mut() {
        use std::io::Write;
        file.write_all(content.as_bytes()).expect("Failed to write logs");
        file.flush().expect("Failed to flush logs");
    }
}
