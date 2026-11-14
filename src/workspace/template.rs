//! Project templates.
//!
//! The built-in template is the structure this application ships with: the
//! folder tree a video project starts from. A user can point the workspace at a
//! folder of their own instead; its contents are then copied into the freshly
//! created project folder. When the configured folder disappeared the built-in
//! structure is used, because creating a project must never fail just because a
//! template path went away.

use std::fs;
use std::path::{Path, PathBuf};

use crate::logs::logs_output::logs_output;

/// Location reported by the log lines of this module.
const LOG_LOCATION: &str = "workspace::template";

/// The folder structure of the built-in template, relative to the project
/// folder. Taken from the author's own project template, so a new project looks
/// exactly like the one he creates by hand.
const BUILTIN_FOLDERS: [&str; 23] = [
    "1.工程文件",
    "2.原始素材/原素材",
    "2.原始素材/图片",
    "2.原始素材/转码素材",
    "2.原始素材/音频",
    "3.素材/视频",
    "3.素材/音乐",
    "3.素材/音效",
    "3.素材/音频",
    "4.特效/导出/图片",
    "4.特效/导出/视频",
    "4.特效/导出/音频",
    "4.特效/素材",
    "5.字幕/工程文件",
    "5.字幕/文稿",
    "5.字幕/音频文件",
    "6.导出/小样",
    "6.导出/成片",
    "6.导出/音频",
    "7.通用素材/logo",
    "7.通用素材/片头片尾",
    "8.封面",
    "9.归档",
];

/// Empty placeholder files that carry the naming convention of the author's
/// template. They are only hints, a project works without them.
const BUILTIN_FILES: [&str; 2] = [
    "2.原始素材/原素材/年-月-日_机型_机位",
    "6.导出/成片/年-月-日_片名_分辨率_编码_格式_导出人_备注_版本",
];

/// Creates one directory, reporting the failure with its full path.
fn make_dir(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|error| {
        logs_output(
            LOG_LOCATION,
            &format!("Failed to create {}: {}.", path.display(), error),
            "err",
        );
        format!("无法创建 {}：{}", path.display(), error)
    })
}

/// Copies every entry of `from` into `to`, recursively.
///
/// The folder itself is not reproduced: only its contents are copied, because
/// the project folder was already created and should *be* the template.
fn copy_contents(from: &Path, to: &Path) -> Result<(), String> {
    let entries = fs::read_dir(from).map_err(|error| {
        logs_output(
            LOG_LOCATION,
            &format!(
                "Failed to read template folder {}: {}.",
                from.display(),
                error
            ),
            "err",
        );
        format!("无法读取模板文件夹 {}：{}", from.display(), error)
    })?;
    for entry in entries.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            make_dir(&target)?;
            copy_contents(&source, &target)?;
        } else {
            fs::copy(&source, &target).map_err(|error| {
                logs_output(
                    LOG_LOCATION,
                    &format!(
                        "Failed to copy {} to {}: {}.",
                        source.display(),
                        target.display(),
                        error
                    ),
                    "err",
                );
                format!("无法复制 {}：{}", source.display(), error)
            })?;
        }
    }
    Ok(())
}

/// Writes the built-in project structure into `destination`.
pub fn builtin(destination: &Path) -> Result<(), String> {
    for folder in BUILTIN_FOLDERS {
        make_dir(&destination.join(folder))?;
    }
    for file in BUILTIN_FILES {
        let path = destination.join(file);
        if let Some(parent) = path.parent() {
            make_dir(parent)?;
        }
        if !path.exists() {
            fs::write(&path, b"").map_err(|error| {
                logs_output(
                    LOG_LOCATION,
                    &format!("Failed to write {}: {}.", path.display(), error),
                    "err",
                );
                format!("无法写入 {}：{}", path.display(), error)
            })?;
        }
    }
    logs_output(
        LOG_LOCATION,
        &format!(
            "Built-in template written to {} ({} folders, {} placeholder files).",
            destination.display(),
            BUILTIN_FOLDERS.len(),
            BUILTIN_FILES.len()
        ),
        "info",
    );
    Ok(())
}

/// Fills `destination` with the template the user configured.
///
/// Returns the template that was used: an empty string means the built-in
/// structure, otherwise the folder that was copied. The caller turns that into
/// a localised text, so no user facing string lives in this module.
pub fn install(template: &str, destination: &Path) -> Result<String, String> {
    let template = template.trim();
    if template.is_empty() {
        builtin(destination)?;
        return Ok(String::new());
    }

    let path = PathBuf::from(template);
    if path.is_dir() {
        logs_output(
            LOG_LOCATION,
            &format!("Using the template folder {}.", path.display()),
            "info",
        );
        copy_contents(&path, destination)?;
        return Ok(path.display().to_string());
    }

    logs_output(
        LOG_LOCATION,
        &format!(
            "The configured template {} does not exist, the built-in structure is used.",
            path.display()
        ),
        "warn",
    );
    builtin(destination)?;
    Ok(String::new())
}
