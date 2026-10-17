//! Backend translations: tray menu labels and every user-facing message
//! (errors shown in the pill and the settings window).
//!
//! Strings live in `src-tauri/i18n/<lang>.json` as flat `key → text` maps with
//! `{name}` placeholders, embedded at build time. The active language follows
//! the `ui_language` setting; missing keys fall back to English, then to the
//! key itself.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Supported UI languages (codes match the frontend `Locale`).
pub const LANGUAGES: &[&str] = &["en", "es", "pt", "fr", "de", "it", "ko", "ja", "zh"];

const SOURCES: [(&str, &str); 9] = [
    ("en", include_str!("../i18n/en.json")),
    ("es", include_str!("../i18n/es.json")),
    ("pt", include_str!("../i18n/pt.json")),
    ("fr", include_str!("../i18n/fr.json")),
    ("de", include_str!("../i18n/de.json")),
    ("it", include_str!("../i18n/it.json")),
    ("ko", include_str!("../i18n/ko.json")),
    ("ja", include_str!("../i18n/ja.json")),
    ("zh", include_str!("../i18n/zh.json")),
];

type Table = HashMap<String, String>;

fn tables() -> &'static HashMap<&'static str, Table> {
    static TABLES: OnceLock<HashMap<&'static str, Table>> = OnceLock::new();
    TABLES.get_or_init(|| {
        SOURCES
            .iter()
            .map(|(lang, src)| {
                let table: Table = serde_json::from_str(src).unwrap_or_else(|e| {
                    eprintln!("[whisperaround] i18n/{lang}.json: {e}");
                    Table::new()
                });
                (*lang, table)
            })
            .collect()
    })
}

static CURRENT: RwLock<String> = RwLock::new(String::new());

/// Switch the backend language (unknown codes fall back to English).
pub fn set_language(code: &str) {
    let code = if LANGUAGES.contains(&code) { code } else { "en" };
    *CURRENT.write().unwrap() = code.to_string();
}

fn lookup(key: &str) -> String {
    let lang = CURRENT.read().unwrap().clone();
    let all = tables();
    all.get(lang.as_str())
        .and_then(|t| t.get(key))
        .or_else(|| all.get("en").and_then(|t| t.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Translate `key`.
pub fn t(key: &str) -> String {
    lookup(key)
}

/// Translate `key`, replacing `{name}` placeholders.
pub fn tf(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut s = lookup(key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_parses_and_has_only_known_keys() {
        let en = &tables()["en"];
        for (lang, table) in tables() {
            for key in table.keys() {
                assert!(en.contains_key(key), "{lang}.json has a key missing from en.json: {key}");
            }
        }
    }

    #[test]
    fn placeholders_survive_translation() {
        let en = &tables()["en"];
        for (lang, table) in tables() {
            for (key, text) in table {
                let src = &en[key];
                for part in src.split('{').skip(1) {
                    let name = part.split('}').next().unwrap_or("");
                    if !name.is_empty() {
                        assert!(text.contains(&format!("{{{name}}}")), "{lang}:{key} lost {{{name}}}");
                    }
                }
            }
        }
    }

    #[test]
    fn falls_back_to_english_then_key() {
        set_language("zz");
        assert_eq!(t("tray.toggle"), "Toggle Recording");
        assert_eq!(t("no.such.key"), "no.such.key");
        assert_eq!(tf("no.{x}", &[("x", &1)]), "no.1");
    }
}
