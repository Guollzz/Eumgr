//! Automatic classification of the files that were added to the workspace page.
//!
//! Every added file is matched against the user defined rules. A rule either
//! looks at the file extension (`kind == "ext"`) or at a keyword inside the file
//! name (`kind == "keyword"`), and points at a destination folder.
//!
//! The rules that shape this implementation come from the user request:
//! * files are **copied**, never moved, so nothing is lost when a rule is wrong;
//! * two rules with the same condition but different folders produce two copies;
//! * adding a folder never flattens it: the copies land in
//!   `<target>/<folder name>/<path inside that folder>`;
//! * when a file would land exactly where it already is, the copy is skipped;
//! * if a destination path already exists, a numbered suffix is added instead of
//!   overwriting somebody else's file.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::ClassifyRule;
use crate::logs::logs_output::logs_output;
use crate::workspace::{is_inside, same_path};

/// Location reported by the log lines of this module.
const LOG_LOCATION: &str = "workspace::classify";

/// How many copies were made and what was left out, shown by the page.
pub struct Outcome {
    /// Files written to at least one destination folder.
    pub copied: usize,
    /// Files without a matching rule, or already in the right folder.
    pub skipped: usize,
    /// Files that could not be written, the reason is in the log file.
    pub failed: usize,
}

/// One resolved destination of a rule: the folder it writes to.
fn rule_target(rule: &ClassifyRule, workdir: &Path) -> Option<PathBuf> {
    let target = rule.target.trim();
    if target.is_empty() {
        return None;
    }
    let path = PathBuf::from(target);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(workdir.join(path))
    }
}

/// True when the rule selects this file.
///
/// The comparison is case insensitive: `.MP3` matches `.mp3` and a keyword
/// matches regardless of how the file was capitalised.
fn matches(rule: &ClassifyRule, file: &Path) -> bool {
    let value = rule.value.trim();
    if value.is_empty() {
        return false;
    }
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if rule.kind == "ext" {
        let mut extension = value.to_lowercase();
        if !extension.starts_with('.') {
            extension.insert(0, '.');
        }
        name.ends_with(&extension)
    } else {
        name.contains(&value.to_lowercase())
    }
}

/// Adds a destination unless the same folder is already in the list.
///
/// Two rules that point at the same folder are collapsed into one entry, while
/// two rules with the same condition but different folders both survive — that
/// is exactly the "copy twice" behaviour the user asked for.
fn push_target(targets: &mut Vec<PathBuf>, target: PathBuf) {
    if targets.iter().any(|known| same_path(known, &target)) {
        return;
    }
    targets.push(target);
}

/// All folders this file has to be copied into.
///
/// Rules of kind `other` are the fallback: they only apply to a file that no
/// ordinary rule matched, which is how "everything else goes to the leftovers
/// folder" is expressed.
fn targets_for(file: &Path, rules: &[ClassifyRule], workdir: &Path) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = Vec::new();
    for rule in rules.iter().filter(|rule| rule.kind != "other") {
        if !matches(rule, file) {
            continue;
        }
        if let Some(target) = rule_target(rule, workdir) {
            push_target(&mut targets, target);
        }
    }
    if targets.is_empty() {
        for rule in rules.iter().filter(|rule| rule.kind == "other") {
            if let Some(target) = rule_target(rule, workdir) {
                push_target(&mut targets, target);
            }
        }
    }
    targets
}

/// A destination that does not exist yet.
///
/// Overwriting is never acceptable here: the destination folder is the user's
/// own library, so a clash becomes "name (2).ext" instead of a lost file.
fn unique_destination(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for index in 2..1000 {
        let candidate = parent.join(format!("{} ({}){}", stem, index, extension));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

/// Copies one file, creating the destination folders on the way.
///
/// Returns true when the copy happened. Failures are logged with the OS error
/// and reported as `false` so the caller can count them.
fn copy_file(from: &Path, to: &Path) -> bool {
    if let Some(parent) = to.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            logs_output(
                LOG_LOCATION,
                &format!("Failed to create {}: {}.", parent.display(), error),
                "err",
            );
            return false;
        }
    }
    let destination = unique_destination(to);
    match fs::copy(from, &destination) {
        Ok(_) => {
            logs_output(
                LOG_LOCATION,
                &format!("Copied {} to {}.", from.display(), destination.display()),
                "info",
            );
            true
        }
        Err(error) => {
            logs_output(
                LOG_LOCATION,
                &format!(
                    "Failed to copy {} to {}: {}.",
                    from.display(),
                    destination.display(),
                    error
                ),
                "err",
            );
            false
        }
    }
}

/// One file that has to be copied, plus the path it keeps below the added folder.
struct Pending {
    file: PathBuf,
    relative: PathBuf,
}

/// Collects every file inside an added folder, remembering the relative path.
fn collect(dir: &Path, base: &Path, files: &mut Vec<Pending>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            logs_output(
                LOG_LOCATION,
                &format!("Failed to read {}: {}.", dir.display(), error),
                "err",
            );
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, files);
        } else {
            let relative = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            files.push(Pending {
                file: path,
                relative,
            });
        }
    }
}

/// Copies one file that was added on its own.
fn copy_single(file: &Path, rules: &[ClassifyRule], workdir: &Path, outcome: &mut Outcome) {
    let targets = targets_for(file, rules, workdir);
    if targets.is_empty() {
        logs_output(
            LOG_LOCATION,
            &format!("No rule matches {}, it is skipped.", file.display()),
            "info",
        );
        outcome.skipped += 1;
        return;
    }
    let name = file.file_name().unwrap_or_default();
    let mut copied_any = false;
    let mut skipped_here = false;
    for target in targets {
        if let Some(parent) = file.parent() {
            if same_path(&target, parent) {
                logs_output(
                    LOG_LOCATION,
                    &format!(
                        "{} already lives in {}, no copy needed.",
                        file.display(),
                        target.display()
                    ),
                    "info",
                );
                skipped_here = true;
                continue;
            }
        }
        if copy_file(file, &target.join(name)) {
            copied_any = true;
        } else {
            outcome.failed += 1;
        }
    }
    if copied_any {
        outcome.copied += 1;
    } else if skipped_here {
        outcome.skipped += 1;
    }
}

/// Copies a whole added folder, keeping its name and its inner structure.
fn copy_folder(
    folder: &Path,
    rules: &[ClassifyRule],
    workdir: &Path,
    outcome: &mut Outcome,
) {
    let mut files = Vec::new();
    collect(folder, folder, &mut files);
    if files.is_empty() {
        logs_output(
            LOG_LOCATION,
            &format!("The added folder {} is empty, nothing to copy.", folder.display()),
            "warn",
        );
        outcome.skipped += 1;
        return;
    }
    let folder_name = folder
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = folder.parent().map(|p| p.to_path_buf());
    let mut copied_any = false;
    let mut skipped_here = false;
    for pending in &files {
        for target in targets_for(&pending.file, rules, workdir) {
            // Copying into the folder that already holds this file, or into the
            // added folder itself, would duplicate or even recurse — skip both.
            if let Some(parent) = &parent {
                if same_path(&target, parent) {
                    skipped_here = true;
                    continue;
                }
            }
            if is_inside(&target, folder) {
                logs_output(
                    LOG_LOCATION,
                    &format!(
                        "Target {} is inside the added folder {}, it is skipped.",
                        target.display(),
                        folder.display()
                    ),
                    "warn",
                );
                skipped_here = true;
                continue;
            }
            let destination = target.join(&folder_name).join(&pending.relative);
            // Counted per file: a folder can spread over several targets and
            // the summary has to say how many files were actually copied.
            if copy_file(&pending.file, &destination) {
                outcome.copied += 1;
                copied_any = true;
            } else {
                outcome.failed += 1;
            }
        }
    }
    if !copied_any && skipped_here {
        outcome.skipped += 1;
    }
}

/// Classifies every added path and copies the matches into their folders.
///
/// `workdir` resolves the relative target folders of the rules; an added path
/// that no longer exists is reported as a failure instead of aborting the run.
pub fn process(sources: &[String], rules: &[ClassifyRule], workdir: &Path) -> Outcome {
    let mut outcome = Outcome {
        copied: 0,
        skipped: 0,
        failed: 0,
    };
    if rules.is_empty() {
        logs_output(
            LOG_LOCATION,
            "No classification rule is configured, every file is skipped.",
            "warn",
        );
        outcome.skipped = sources.len();
        return outcome;
    }
    logs_output(
        LOG_LOCATION,
        &format!(
            "Classifying {} added path(s) with {} rule(s) into {}.",
            sources.len(),
            rules.len(),
            workdir.display()
        ),
        "info",
    );
    for source in sources {
        let path = PathBuf::from(source);
        if path.is_dir() {
            copy_folder(&path, rules, workdir, &mut outcome);
        } else if path.is_file() {
            copy_single(&path, rules, workdir, &mut outcome);
        } else {
            logs_output(
                LOG_LOCATION,
                &format!("{} does not exist any more, it is skipped.", path.display()),
                "err",
            );
            outcome.failed += 1;
        }
    }
    logs_output(
        LOG_LOCATION,
        &format!(
            "Classification finished: {} copied, {} skipped, {} failed.",
            outcome.copied, outcome.skipped, outcome.failed
        ),
        "info",
    );
    outcome
}
