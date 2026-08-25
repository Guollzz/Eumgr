use serde::Deserialize;
use std::collections::HashMap;
use include_dir::{include_dir, Dir};

// Embed the entire locales folder into the binary at compile time; the locales folder must be at the same level as Cargo.toml.
static LOCALE_ASSETS: Dir = include_dir!("./locales");

#[derive(Debug, Deserialize, Default)]
pub struct Locale {
    #[serde(flatten)]
    pub texts: HashMap<String, String>,
}

impl Locale {
    // No longer read files from disk; instead, read from resources embedded at compile time.
    pub fn load(lang: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file_name = format!("{}.json", lang);
        let file = LOCALE_ASSETS.get_file(&file_name)
            .ok_or(format!("Language resource not found.: {file_name}"))?;

        let text = file.contents_utf8()
            .ok_or("Language file is not UTF‑8 text.")?;

        let locale: Locale = serde_json::from_str(text)?;
        Ok(locale)
    }

    pub fn t(&self, key: &str) -> String {
        self.texts.get(key).cloned().unwrap_or_else(|| key.to_string())
    }
}