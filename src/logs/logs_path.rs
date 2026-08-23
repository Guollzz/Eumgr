use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::fs::File;
use once_cell::sync::Lazy;  

static LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

pub fn init_log_file() {
    let timestamp = Local::now().format("%Y-%m-%d-%H-%M-%S");
    let log_filename = format!("log_{}.log", timestamp);
    let log_dir = PathBuf::from("logs");
    
    if !log_dir.exists() {
        fs::create_dir(&log_dir).expect("Failed to create logs folder.");
    }
    
    let log_file_path = log_dir.join(log_filename);
    let log_file = fs::File::create(log_file_path).expect("Failed to create log file.");
    
    let mut guard = LOG_FILE.lock().unwrap();
    *guard = Some(log_file);
}

pub fn write_logs(content: &str) {
    let mut guard = LOG_FILE.lock().unwrap();
    if let Some(file) = guard.as_mut() {
        use std::io::Write;
        file.write_all(content.as_bytes()).expect("Failed to write logs");
        file.flush().expect("Failed to flush logs");
    }
}