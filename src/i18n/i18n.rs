//! Localisation.
//!
//! The JSON files in `locales/` are embedded into the binary at compile time, so
//! a released executable never depends on loose files next to it. A missing
//! translation falls back to the key itself, which makes a forgotten key visible
//! in the UI instead of rendering an empty label.

use crate::logs;
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use std::collections::HashMap;

/// Every file of `locales/` (resolved relative to `Cargo.toml`).
static LOCALE_ASSETS: Dir = include_dir!("./locales");

/// One language file: a flat `key -> text` map.
#[derive(Debug, Deserialize, Default)]
pub struct Locale {
    #[serde(flatten)]
    pub texts: HashMap<String, String>,
}

impl Locale {
    /// Loads `<lang>.json` from the embedded assets.
    ///
    /// Logs the language and the number of translations found, so a log file
    /// immediately shows which language file was actually used. Every failure
    /// path is logged as well, because a broken language makes the whole UI
    /// fall back to raw keys.
    pub fn load(lang: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file_name = format!("{}.json", lang);
        let file = match LOCALE_ASSETS.get_file(&file_name) {
            Some(file) => file,
            None => {
                logs::logs_output::logs_output(
                    "i18n::Locale::load",
                    &format!("Language resource \"{}\" is not embedded in the binary.", file_name),
                    "err",
                );
                return Err(format!("Language resource not found.: {file_name}").into());
            }
        };

        let text = match file.contents_utf8() {
            Some(text) => text,
            None => {
                logs::logs_output::logs_output(
                    "i18n::Locale::load",
                    &format!("Language resource \"{}\" is not UTF-8 text.", file_name),
                    "err",
                );
                return Err("Language file is not UTF-8 text.".into());
            }
        };

        let locale: Locale = match serde_json::from_str(text) {
            Ok(locale) => locale,
            Err(error) => {
                logs::logs_output::logs_output(
                    "i18n::Locale::load",
                    &format!("Language resource \"{}\" is not valid JSON: {}.", file_name, error),
                    "err",
                );
                return Err(error.into());
            }
        };
        logs::logs_output::logs_output(
            "i18n::Locale::load",
            &format!("Loaded locale \"{}\" with {} keys.", lang, locale.texts.len()),
            "info",
        );
        Ok(locale)
    }

    /// Returns the translation of `key`, or the key itself when it is missing.
    ///
    /// A missing key is a real bug (the UI then shows something like
    /// "launchpad"), therefore it is logged as a warning with the language.
    pub fn t(&self, key: &str) -> String {
        match self.texts.get(key) {
            Some(text) => text.clone(),
            None => {
                logs::logs_output::logs_output(
                    "i18n::Locale::t",
                    &format!("Missing translation for key \"{}\".", key),
                    "warn",
                );
                key.to_string()
            }
        }
    }
}
