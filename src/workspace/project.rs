//! Creation of a new project inside the workspace directory.
//!
//! The project folder is named `<YYYY-MM-DD>_<name>`, matching the naming of the
//! template the user works with, and is filled from the configured template.

use std::fs;
use std::path::{Path, PathBuf};

use crate::logs::logs_output::logs_output;
use crate::workspace::template;

/// Location reported by the log lines of this module.
const LOG_LOCATION: &str = "workspace::project";

/// Characters Windows forbids inside a file or folder name.
const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Checks a project name and returns it without surrounding spaces.
///
/// An invalid name is rejected with a message the page can show to the user: an
/// unreadable folder name would only fail later, deep inside `create_dir_all`.
pub fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("请先输入项目名称".to_string());
    }
    if name == "." || name == ".." {
        return Err("项目名称不能是 . 或 ..".to_string());
    }
    if let Some(bad) = name.chars().find(|c| FORBIDDEN.contains(c)) {
        return Err(format!("项目名称不能包含 {} 字符", bad));
    }
    Ok(name.to_string())
}

/// Builds the folder name of a project: `<date>_<name>`.
fn folder_name(name: &str) -> String {
    format!("{}_{}", chrono::Local::now().format("%Y-%m-%d"), name)
}

/// Creates `<workdir>/<date>_<name>` and fills it from the template.
///
/// Returns the created folder. Failures leave a log line behind and are reported
/// as a message the page can display.
pub fn create(workdir: &Path, name: &str, template_setting: &str) -> Result<PathBuf, String> {
    let name = clean_name(name)?;
    if !workdir.is_dir() {
        logs_output(
            LOG_LOCATION,
            &format!("The workspace directory {} does not exist.", workdir.display()),
            "err",
        );
        return Err(format!("工作目录 {} 不存在", workdir.display()));
    }
    let folder = workdir.join(folder_name(&name));
    if folder.exists() {
        logs_output(
            LOG_LOCATION,
            &format!("The project folder {} already exists.", folder.display()),
            "warn",
        );
        return Err(format!("{} 已经存在", folder.display()));
    }
    fs::create_dir_all(&folder).map_err(|error| {
        logs_output(
            LOG_LOCATION,
            &format!("Failed to create {}: {}.", folder.display(), error),
            "err",
        );
        format!("无法创建 {}：{}", folder.display(), error)
    })?;
    logs_output(
        LOG_LOCATION,
        &format!("Created the project folder {}.", folder.display()),
        "info",
    );

    // The folder exists from here on; a template failure keeps it in place so the
    // user does not lose a project that was already named correctly.
    let used = template::install(template_setting, &folder)?;
    logs_output(
        LOG_LOCATION,
        &format!("Project {} filled from {}.", folder.display(), used),
        "info",
    );
    Ok(folder)
}
