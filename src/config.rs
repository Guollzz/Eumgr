//! Reading and writing `config.json`.
//!
//! The file lives in the working directory of the process (next to the
//! executable when started normally) and stores everything the application has
//! to remember between two starts:
//!
//! * the UI language,
//! * the page the user was on when the application was closed (including the
//!   selected row of the settings page),
//! * the entries of the launchpad (name + absolute path),
//! * which of those entries belong to a continuous launch.
//!
//! Every field is `#[serde(default)]`, so a file written by an older version
//! (which only knew `language`) still loads: the missing sections simply fall
//! back to their defaults instead of failing to parse.

use crate::logs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// One tile of the launchpad.
///
/// `name` is a snapshot taken when the entry was added: it is what the tile
/// shows, so renaming the file on disk does not silently relabel the tile.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchpadItem {
    /// Label shown under the tile icon.
    pub name: String,
    /// Absolute path that is started when the tile is clicked.
    pub path: String,
}

/// The page the application was showing when it was closed.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PageState {
    /// Operation area page: 0 = workspace, 1 = launchpad, 2 = settings.
    #[serde(default)]
    pub main: i32,
    /// Selected row of the settings page: 0 = general, 1 = about.
    #[serde(default)]
    pub settings: i32,
}

/// Everything the launchpad has to remember.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct LaunchpadState {
    /// Tiles, in display order.
    #[serde(default)]
    pub items: Vec<LaunchpadItem>,
    /// Paths of the entries that a continuous launch starts, in click order is
    /// not preserved on purpose: the tile order decides, see
    /// [`crate::launchpad::continuous_paths`].
    #[serde(default)]
    pub continuous: Vec<String>,
}

/// One rule of the automatic file classification.
///
/// A rule matches a file either by its extension (`kind == "ext"`, `value` like
/// ".mp3") or by a substring of its name (`kind == "keyword"`). Every matching
/// rule copies the file into its own `target` folder, so two rules with the same
/// condition but different folders deliberately produce two copies.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ClassifyRule {
    /// "ext" or "keyword"; anything else is treated as "keyword" when matching.
    #[serde(default)]
    pub kind: String,
    /// Extension (".mp3") or the keyword to look for in the file name.
    #[serde(default)]
    pub value: String,
    /// Destination folder, absolute or relative to the workspace directory.
    #[serde(default)]
    pub target: String,
}

/// Everything the workspace page has to remember.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorkspaceState {
    /// Directory the file tree shows and that projects are created in.
    #[serde(default)]
    pub directory: String,
    /// Custom template: a folder or a .zip. Empty means the default template.
    #[serde(default)]
    pub template: String,
    /// Classification rules, in display order.
    #[serde(default)]
    pub rules: Vec<ClassifyRule>,
}

/// Contents of `config.json`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    /// Language code of the UI, must match a file name in `locales/`
    /// (for example "zh-CN" or "en").
    #[serde(default = "default_language")]
    pub language: String,
    /// Last visible page, restored on the next start.
    #[serde(default)]
    pub page: PageState,
    /// Launchpad tiles and continuous launch selection.
    #[serde(default)]
    pub launchpad: LaunchpadState,
    /// Workspace directory, project template and classification rules.
    #[serde(default)]
    pub workspace: WorkspaceState,
}

/// Language used when the file does not name one.
fn default_language() -> String {
    "zh-CN".to_string()
}

impl Config {
    /// Values used when `config.json` does not exist yet.
    pub fn default() -> Self {
        Config {
            language: default_language(),
            page: PageState::default(),
            launchpad: LaunchpadState::default(),
            workspace: WorkspaceState::default(),
        }
    }

    /// Writes the whole configuration as pretty printed JSON.
    ///
    /// A failed write is logged with the OS error before it is returned, so the
    /// log file always explains why a setting did not survive a restart.
    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        if let Err(error) = fs::write(path, &json) {
            logs::logs_output::logs_output(
                "config::Config::save",
                &format!("Failed to write {}: {}.", path, error),
                "err",
            );
            return Err(error.into());
        }
        logs::logs_output::logs_output(
            "config::Config::save",
            &format!("Configuration saved to {}.", path),
            "info",
        );
        Ok(())
    }

    /// Reads and parses the whole configuration.
    ///
    /// Both steps are logged when they fail: a broken or unreadable file is the
    /// most common reason for a window that falls back to the default language.
    /// Values that cannot be shown by the UI (a page index that does not exist)
    /// are clamped here instead of leaving the window on a blank page.
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let json = match fs::read_to_string(path) {
            Ok(json) => json,
            Err(error) => {
                logs::logs_output::logs_output(
                    "config::Config::load",
                    &format!("Failed to read {}: {}.", path, error),
                    "err",
                );
                return Err(error.into());
            }
        };
        let mut config: Config = match serde_json::from_str(&json) {
            Ok(config) => config,
            Err(error) => {
                logs::logs_output::logs_output(
                    "config::Config::load",
                    &format!("Failed to parse {}: {}.", path, error),
                    "err",
                );
                return Err(error.into());
            }
        };
        config.clamp_pages(path);
        logs::logs_output::logs_output(
            "config::Config::load",
            &format!(
                "Configuration loaded from {} (language = \"{}\", page = {}/{}, {} launchpad item(s), {} continuous, workspace \"{}\", template \"{}\", {} rule(s)).",
                path,
                config.language,
                config.page.main,
                config.page.settings,
                config.launchpad.items.len(),
                config.launchpad.continuous.len(),
                config.workspace.directory,
                config.workspace.template,
                config.workspace.rules.len()
            ),
            "info",
        );
        Ok(config)
    }

    /// Clamps the stored page indices to the pages that actually exist.
    ///
    /// A hand edited or outdated file may name a page that is gone; without
    /// this the window would start on an empty operation area.
    fn clamp_pages(&mut self, path: &str) {
        let main = self.page.main;
        self.page.main = main.clamp(0, 2);
        if self.page.main != main {
            logs::logs_output::logs_output(
                "config::Config::load",
                &format!(
                    "{}: page.main {} is out of range, clamped to {}.",
                    path, main, self.page.main
                ),
                "warn",
            );
        }

        let settings = self.page.settings;
        self.page.settings = settings.clamp(0, 1);
        if self.page.settings != settings {
            logs::logs_output::logs_output(
                "config::Config::load",
                &format!(
                    "{}: page.settings {} is out of range, clamped to {}.",
                    path, settings, self.page.settings
                ),
                "warn",
            );
        }
    }

    /// Loads the configuration, creating it with defaults when it is missing.
    pub fn load_or_create(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if Path::new(path).exists() {
            Config::load(path)
        } else {
            let config = Config::default();
            config.save(path)?;
            logs::logs_output::logs_output(
                "config::Config::load_or_create",
                &format!(
                    "{} was missing, wrote defaults (language = \"{}\").",
                    path, config.language
                ),
                "info",
            );
            Ok(config)
        }
    }
}
