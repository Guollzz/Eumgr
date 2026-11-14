//! Backend of the workspace page.
//!
//! The page is only the presentation layer: the file tree, the project creation
//! and the automatic classification all live here so that every filesystem
//! operation can log what it did. Errors are never swallowed — the reason a file
//! could not be copied is the most valuable line in the log file when a user
//! reports "my files did not arrive".

pub mod classify;
pub mod project;
pub mod template;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::logs::logs_output::logs_output;

/// Location reported by the log lines of this module.
const LOG_LOCATION: &str = "workspace";

/// Upper bound of the rows handed to the UI.
///
/// A working directory may contain an enormous tree while the page can only
/// scroll through it; scanning millions of entries would freeze the window.
const MAX_ROWS: usize = 2000;

/// One visible line of the file tree.
pub struct Row {
    /// Nesting level, 0 for the direct children of the working directory.
    pub depth: i32,
    /// File or folder name shown in the row.
    pub name: String,
    /// Absolute path, used as the identity when a row is clicked.
    pub path: String,
    /// True for folders, they can be expanded and collapsed.
    pub is_dir: bool,
    /// True when a folder row is currently expanded.
    pub expanded: bool,
    /// True when a folder row has at least one entry to expand.
    pub has_children: bool,
}

/// Reads a directory sorted the way a file manager does: folders first, then
/// files, both compared case insensitively.
///
/// A failing directory is logged and treated as empty, so one protected folder
/// cannot stop the whole tree from being drawn.
fn sorted_entries(dir: &Path) -> Vec<(PathBuf, bool)> {
    let mut entries: Vec<(PathBuf, bool)> = Vec::new();
    match fs::read_dir(dir) {
        Ok(read) => {
            for entry in read {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        entries.push((path, is_dir));
                    }
                    Err(error) => {
                        logs_output(
                            LOG_LOCATION,
                            &format!("Failed to read an entry of {}: {}.", dir.display(), error),
                            "warn",
                        );
                    }
                }
            }
        }
        Err(error) => {
            logs_output(
                LOG_LOCATION,
                &format!("Failed to read directory {}: {}.", dir.display(), error),
                "warn",
            );
            return entries;
        }
    }
    entries.sort_by_key(|(path, is_dir)| {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        (!*is_dir, name)
    });
    entries
}

/// True when a folder contains at least one entry, used to decide whether the
/// expand arrow of a row means anything.
fn dir_has_entries(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|mut read| read.next().is_some())
        .unwrap_or(false)
}

/// Appends the rows of one directory level and, for expanded folders, their
/// children as well.
fn push_children(dir: &Path, depth: i32, expanded: &HashSet<PathBuf>, rows: &mut Vec<Row>) {
    for (path, is_dir) in sorted_entries(dir) {
        if rows.len() >= MAX_ROWS {
            return;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let is_expanded = is_dir && expanded.contains(&path);
        let has_children = is_dir && dir_has_entries(&path);
        rows.push(Row {
            depth,
            name,
            path: path.display().to_string(),
            is_dir,
            expanded: is_expanded,
            has_children,
        });
        if is_expanded {
            push_children(&path, depth + 1, expanded, rows);
        }
    }
}

/// Flattens the part of the tree that is currently expanded into drawable rows.
///
/// The page receives a flat list plus the depth of each row and indents them, so
/// Slint never has to recurse.
pub fn flatten(root: &Path, expanded: &HashSet<PathBuf>) -> Vec<Row> {
    let mut rows = Vec::new();
    if root.as_os_str().is_empty() || !root.is_dir() {
        return rows;
    }
    push_children(root, 0, expanded, &mut rows);
    rows
}

/// Turns the configured directory into a path that can be scanned.
///
/// An empty or missing directory is the normal state of a fresh installation and
/// simply yields no rows.
pub fn usable_directory(directory: &str) -> Option<PathBuf> {
    let trimmed = directory.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_dir() { Some(path) } else { None }
}

/// Canonical form used to compare two paths that should mean the same folder.
///
/// `canonicalize` fails for a path that does not exist yet, so the fallback just
/// compares the text form, which is good enough for the "already in place" check.
pub fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase(),
    }
}

/// True when `child` is inside `parent`, used to refuse a target folder that
/// would make a copy recurse into the folder it is copying.
pub fn is_inside(child: &Path, parent: &Path) -> bool {
    let child = fs::canonicalize(child).unwrap_or_else(|_| child.to_path_buf());
    let parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    child != parent && child.starts_with(&parent)
}
