//! Launchpad domain logic.
//!
//! This module only holds the parts that do not talk to the OS: how a tile is
//! labelled, which entries a continuous launch starts and how the summary text
//! of the "will be started" box is built. The two unsafe parts live next to it:
//!
//! * [`dialog`] opens the native file/folder pickers,
//! * [`launcher`] starts a file or folder with its default handler.

pub mod dialog;
pub mod drop_target;
pub mod launcher;

use crate::config::LaunchpadItem;
use std::path::Path;

/// How many names the summary box shows before it collapses the rest into
/// "+N". Three names fit into the 320px box of the launchpad toolbar.
const SUMMARY_MAX_NAMES: usize = 3;

/// Result of adding a path to the tile list.
pub enum AddOutcome {
    /// The entry was appended, this is its index.
    Added(usize),
    /// The path is already on the launchpad.
    Duplicate,
    /// The path does not name a readable file or folder.
    Invalid,
}

/// Label of a tile: the file name without its extension ("code.exe" ->
/// "code"), or the folder name for directories. Falls back to the full path
/// when the name cannot be determined, so a tile is never blank.
pub fn entry_name(path: &Path) -> String {
    let is_dir = path.is_dir();
    let name = if is_dir {
        path.file_name().map(|name| name.to_string_lossy().into_owned())
    } else {
        path.file_stem().map(|name| name.to_string_lossy().into_owned())
    };
    name.filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// First character of a name, upper cased: the letter drawn on the tile icon.
/// Returns "?" for an empty name so the icon plate is never empty.
pub fn letter(name: &str) -> String {
    name.chars()
        .next()
        .map(|first| first.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Normalises a path so two entries for the same target compare equal.
///
/// Windows paths are case insensitive, so comparison is done on a lower cased
/// canonical form. `canonicalize` also resolves `..` segments and 8.3 names; if
/// it fails (file gone, permission) the plain lower cased path is used.
fn comparable(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_lowercase()
}

/// Appends `path` to `items` unless the same target is already present.
///
/// The caller owns the configuration, so this only mutates the vector; saving
/// and refreshing the UI is done by the window after the outcome is known.
pub fn add_item(items: &mut Vec<LaunchpadItem>, path: &Path) -> AddOutcome {
    if !path.exists() {
        return AddOutcome::Invalid;
    }

    let candidate = comparable(path);
    if items
        .iter()
        .any(|item| comparable(Path::new(&item.path)) == candidate)
    {
        return AddOutcome::Duplicate;
    }

    let index = items.len();
    items.push(LaunchpadItem {
        name: entry_name(path),
        path: path.to_string_lossy().into_owned(),
    });
    AddOutcome::Added(index)
}

/// Removes the tile at `index` and drops it from the continuous launch list.
///
/// Returns the removed entry so the caller can log what disappeared; a removed
/// entry must not stay in the continuous list, otherwise a launch would start a
/// process that is no longer on the launchpad.
pub fn remove_item(
    items: &mut Vec<LaunchpadItem>,
    continuous: &mut Vec<String>,
    index: usize,
) -> Option<LaunchpadItem> {
    if index >= items.len() {
        return None;
    }
    let removed = items.remove(index);
    let removed_key = comparable(Path::new(&removed.path));
    continuous.retain(|path| comparable(Path::new(path)) != removed_key);
    Some(removed)
}

/// Replaces the selection state of one entry.
///
/// Returns the new state (`true` = selected) or `None` when the index does not
/// exist. The list keeps the tile order instead of the click order, so a
/// continuous launch always starts in the same sequence.
pub fn toggle_continuous(
    items: &[LaunchpadItem],
    continuous: &mut Vec<String>,
    index: usize,
) -> Option<bool> {
    let item = items.get(index)?;
    let key = comparable(Path::new(&item.path));
    match continuous
        .iter()
        .position(|path| comparable(Path::new(path)) == key)
    {
        Some(position) => {
            continuous.remove(position);
            Some(false)
        }
        None => {
            continuous.push(item.path.clone());
            Some(true)
        }
    }
}

/// Whether an entry takes part in a continuous launch.
pub fn is_continuous(continuous: &[String], item: &LaunchpadItem) -> bool {
    let key = comparable(Path::new(&item.path));
    continuous
        .iter()
        .any(|path| comparable(Path::new(path)) == key)
}

/// Text of the box that shows what a continuous launch would start.
///
/// Empty selection renders `none_text` (a localised "nothing selected"), long
/// selections keep the first entries and collapse the rest into "+N" so the box
/// never grows wider than the toolbar allows.
pub fn continuous_summary(
    items: &[LaunchpadItem],
    continuous: &[String],
    none_text: &str,
) -> String {
    let names: Vec<&str> = items
        .iter()
        .filter(|item| is_continuous(continuous, item))
        .map(|item| item.name.as_str())
        .collect();

    if names.is_empty() {
        return none_text.to_string();
    }
    if names.len() <= SUMMARY_MAX_NAMES {
        return names.join(", ");
    }
    format!(
        "{}, +{}",
        names[..SUMMARY_MAX_NAMES].join(", "),
        names.len() - SUMMARY_MAX_NAMES
    )
}
