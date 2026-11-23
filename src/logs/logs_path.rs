//! Management of the log file itself.
//!
//! One file per run: `logs/log_<timestamp>.log`, created next to the working
//! directory of the process. The handle is kept in a global so every module can
//! write without passing it around.

use chrono::Local;
use once_cell::sync::Lazy;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Currently opened log file. `None` until [`init_log_file`] ran, which makes
/// [`write_logs`] a no-op (log calls before initialisation are simply dropped).
static LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

/// Path of the log file of this run, remembered so the "clear logs" action can
/// keep exactly this file and delete every log of an older run.
static CURRENT_LOG_PATH: Lazy<Mutex<Option<PathBuf>>> = Lazy::new(|| Mutex::new(None));

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
    {
        let mut guard = CURRENT_LOG_PATH.lock().unwrap();
        *guard = Some(log_file_path.clone());
    }

    log_file_path
}

/// Path of the log file of this run, when [`init_log_file`] has run.
pub fn current_log_path() -> Option<PathBuf> {
    CURRENT_LOG_PATH.lock().unwrap().clone()
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

/// True for the files this application writes: `log_<timestamp>.log`.
///
/// The check keeps a file the user put into `logs/` by hand (a note, another
/// program's log) safe from the "clear logs" action.
fn is_log_file(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with("log_") && name.to_ascii_lowercase().ends_with(".log")
}

/// Deletes every log file of `directory` except `current_name`.
///
/// Split out from [`clear_old_logs`] so the selection rules can be tested
/// without initialising the real log file. Files that cannot be removed are
/// logged and skipped: one locked file must not stop the rest from going away.
fn remove_old_logs(directory: &Path, current_name: &OsStr) -> Result<usize, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "Failed to read the log folder {}: {}.",
            directory.display(),
            error
        )
    })?;

    let mut removed = 0usize;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                crate::logs::logs_output::logs_output(
                    "logs::logs_path::clear_old_logs",
                    &format!(
                        "Failed to read an entry of {}: {}.",
                        directory.display(),
                        error
                    ),
                    "warn",
                );
                continue;
            }
        };
        let path = entry.path();
        if !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        if name.as_os_str() == current_name || !is_log_file(&name) {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(error) => crate::logs::logs_output::logs_output(
                "logs::logs_path::clear_old_logs",
                &format!("Failed to remove {}: {}.", path.display(), error),
                "warn",
            ),
        }
    }
    Ok(removed)
}

/// Removes every log file of an older run and keeps the log of this run.
///
/// Returns how many files were deleted. The current file is matched by its
/// file name, because both paths use the same `logs/` folder the application
/// opened at start-up.
pub fn clear_old_logs() -> Result<usize, String> {
    let current = current_log_path()
        .ok_or_else(|| "The log file of this run is not known yet.".to_string())?;
    let directory = current.parent().unwrap_or_else(|| Path::new("logs"));
    let current_name = current.file_name().ok_or_else(|| {
        format!(
            "The current log path {} has no file name.",
            current.display()
        )
    })?;

    let removed = remove_old_logs(directory, current_name)?;
    crate::logs::logs_output::logs_output(
        "logs::logs_path::clear_old_logs",
        &format!(
            "Deleted {} old log file(s) from {} (kept {}).",
            removed,
            directory.display(),
            current.display()
        ),
        "info",
    );
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique temporary folder for one test.
    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "eumgr-logs-test-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn removes_only_old_log_files() {
        let directory = temp_dir("old");
        fs::write(directory.join("log_2026-01-01-00-00-00.log"), b"old").unwrap();
        fs::write(directory.join("log_2026-01-02-00-00-00.log"), b"current").unwrap();
        fs::write(directory.join("note.log"), b"not ours").unwrap();
        fs::write(directory.join("log_today.txt"), b"not a log").unwrap();
        fs::create_dir_all(directory.join("log_2026-01-03-00-00-00.log")).unwrap();

        let removed = remove_old_logs(&directory, OsStr::new("log_2026-01-02-00-00-00.log")).unwrap();

        assert_eq!(removed, 1);
        assert!(directory.join("log_2026-01-02-00-00-00.log").exists());
        assert!(!directory.join("log_2026-01-01-00-00-00.log").exists());
        assert!(directory.join("note.log").exists());
        assert!(directory.join("log_today.txt").exists());
        assert!(directory.join("log_2026-01-03-00-00-00.log").is_dir());

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn keeps_the_current_log_even_when_it_is_the_only_one() {
        let directory = temp_dir("only");
        fs::write(directory.join("log_2026-02-01-00-00-00.log"), b"current").unwrap();

        let removed = remove_old_logs(&directory, OsStr::new("log_2026-02-01-00-00-00.log")).unwrap();

        assert_eq!(removed, 0);
        assert!(directory.join("log_2026-02-01-00-00-00.log").exists());

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn clear_old_logs_keeps_the_log_of_this_run() {
        let directory = temp_dir("current");
        let current = directory.join("log_2026-03-01-00-00-00.log");
        fs::write(&current, b"current").unwrap();
        fs::write(directory.join("log_2026-02-01-00-00-00.log"), b"old").unwrap();
        fs::write(directory.join("notes.txt"), b"keep").unwrap();

        {
            let mut guard = CURRENT_LOG_PATH.lock().unwrap();
            *guard = Some(current.clone());
        }
        let removed = clear_old_logs().unwrap();
        {
            let mut guard = CURRENT_LOG_PATH.lock().unwrap();
            *guard = None;
        }

        assert_eq!(removed, 1);
        assert!(current.exists());
        assert!(!directory.join("log_2026-02-01-00-00-00.log").exists());
        assert!(directory.join("notes.txt").exists());

        fs::remove_dir_all(&directory).unwrap();
    }
}
