//! Persisted user settings (stored as JSON in the app config dir).
//!
//! The OpenAI API key lives only here, in the Rust process. It is never sent
//! back to the webview in clear text — the frontend receives a [`SettingsView`]
//! that only exposes whether a key is configured.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub openai_api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_auto_paste")]
    pub auto_paste: bool,
}

fn default_model() -> String {
    "whisper-1".into()
}
fn default_shortcut() -> String {
    "CmdOrCtrl+Shift+X".into()
}
fn default_auto_paste() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            model: default_model(),
            shortcut: default_shortcut(),
            auto_paste: default_auto_paste(),
        }
    }
}

/// Settings as exposed to the frontend (no key in clear text).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub api_key_configured: bool,
    pub model: String,
    pub shortcut: String,
    pub auto_paste: bool,
}

/// Settings update coming from the frontend.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    /// Empty string means "keep the stored key".
    pub openai_api_key: String,
    pub model: String,
    pub shortcut: String,
    pub auto_paste: bool,
}

impl Settings {
    pub fn view(&self) -> SettingsView {
        SettingsView {
            api_key_configured: !self.openai_api_key.trim().is_empty(),
            model: self.model.clone(),
            shortcut: self.shortcut.clone(),
            auto_paste: self.auto_paste,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| anyhow!("resolving config dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

/// Load settings, falling back to defaults if the file is missing or invalid.
pub fn load(app: &AppHandle) -> Settings {
    match settings_path(app).ok().and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        None => Settings::default(),
    }
}

/// Persist settings to disk, creating the config dir if needed.
pub fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(settings)?;
    std::fs::write(path, json)?;
    Ok(())
}
