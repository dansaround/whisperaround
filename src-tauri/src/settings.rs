//! Persisted user settings (stored as JSON in the app config dir).
//!
//! The OpenAI API key lives only here, in the Rust process. It is never sent
//! back to the webview in clear text — the frontend receives a [`SettingsView`]
//! that only exposes whether a key is configured. Where the OS has a credential
//! store ([`secret`]) the key is kept there and left out of the JSON file.
//!
//! The transcription model and language belong to the active [`Mode`]; the
//! legacy top-level `model` field is only read to seed the first mode of a
//! settings file written before modes existed.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::i18n::{t, tf};
use crate::secret;

/// A transcription preset: which model and language to use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mode {
    pub id: String,
    pub name: String,
    /// Icon key understood by the frontend (`sparkle`, `message`, `laptop`, …).
    #[serde(default = "default_mode_icon")]
    pub icon: String,
    pub model: String,
    /// ISO-639-1 code, or empty for auto-detect.
    #[serde(default)]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub openai_api_key: String,
    /// Legacy (pre-modes) model. Only used to seed the first mode on load.
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_auto_paste")]
    pub auto_paste: bool,
    /// Whether the first-launch welcome window has been dismissed.
    #[serde(default)]
    pub onboarded: bool,

    // --- Shortcuts (see `shortcuts::Binding` for the string format) ---------
    #[serde(default = "default_cancel_shortcut")]
    pub cancel_shortcut: String,
    #[serde(default = "default_mode_shortcut")]
    pub mode_shortcut: String,
    #[serde(default)]
    pub ptt_shortcut: String,
    #[serde(default)]
    pub mouse_shortcut: String,

    // --- Appearance ---------------------------------------------------------
    /// `auto` | `light` | `dark`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// `classic` | `mini` | `none`.
    #[serde(default = "default_pill_style")]
    pub pill_style: String,
    #[serde(default = "default_true")]
    pub pill_always_show: bool,
    /// `top` | `bottom`.
    #[serde(default = "default_pill_position")]
    pub pill_position: String,
    /// Pill look: `standard` (themed, translucent) | `onyx` (solid black).
    #[serde(default = "default_pill_skin")]
    pub pill_skin: String,

    // --- Sound --------------------------------------------------------------
    #[serde(default = "default_true")]
    pub sounds_enabled: bool,
    #[serde(default = "default_volume")]
    pub sound_volume: f32,
    /// Start cue: `orus` | `loto` | `okaso`.
    #[serde(default = "default_start_sound")]
    pub start_sound: String,

    // --- Transcription ------------------------------------------------------
    #[serde(default)]
    pub vocabulary: Vec<String>,
    #[serde(default)]
    pub modes: Vec<Mode>,
    #[serde(default)]
    pub active_mode: String,
    /// Input device name; empty = system default.
    #[serde(default)]
    pub input_device: String,
    #[serde(default = "default_true")]
    pub save_history: bool,
    /// Interface language (see `i18n::LANGUAGES`).
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
}

fn default_model() -> String {
    "gpt-4o-mini-transcribe".into()
}
fn default_shortcut() -> String {
    "CmdOrCtrl+Shift+X".into()
}
fn default_auto_paste() -> bool {
    true
}
pub fn default_cancel_shortcut() -> String {
    "Escape".into()
}
pub fn default_mode_shortcut() -> String {
    "Alt+Shift+KeyK".into()
}
fn default_theme() -> String {
    "auto".into()
}
fn default_pill_style() -> String {
    "mini".into()
}
fn default_pill_position() -> String {
    "top".into()
}
fn default_pill_skin() -> String {
    "standard".into()
}
fn default_true() -> bool {
    true
}
fn default_volume() -> f32 {
    0.6
}
fn default_start_sound() -> String {
    "orus".into()
}
fn default_ui_language() -> String {
    "en".into()
}
fn default_mode_icon() -> String {
    "sparkle".into()
}

impl Default for Settings {
    fn default() -> Self {
        let mut s: Settings = serde_json::from_str("{}").expect("defaults deserialize");
        s.normalize();
        s
    }
}

/// Settings as exposed to the frontend (no key in clear text).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub api_key_configured: bool,
    /// Effective model: the active mode's.
    pub model: String,
    pub language: String,
    pub shortcut: String,
    pub cancel_shortcut: String,
    pub mode_shortcut: String,
    pub ptt_shortcut: String,
    pub mouse_shortcut: String,
    pub auto_paste: bool,
    pub theme: String,
    pub pill_style: String,
    pub pill_always_show: bool,
    pub pill_position: String,
    pub pill_skin: String,
    pub sounds_enabled: bool,
    pub sound_volume: f32,
    pub start_sound: String,
    pub vocabulary: Vec<String>,
    pub modes: Vec<Mode>,
    pub active_mode: String,
    pub input_device: String,
    pub save_history: bool,
    pub ui_language: String,
}

/// Partial update from the frontend: every field is optional so the settings
/// window can auto-save one control at a time.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    /// Empty / blank means "keep the stored key".
    pub openai_api_key: Option<String>,
    pub shortcut: Option<String>,
    pub cancel_shortcut: Option<String>,
    pub mode_shortcut: Option<String>,
    pub ptt_shortcut: Option<String>,
    pub mouse_shortcut: Option<String>,
    pub auto_paste: Option<bool>,
    pub theme: Option<String>,
    pub pill_style: Option<String>,
    pub pill_always_show: Option<bool>,
    pub pill_position: Option<String>,
    pub pill_skin: Option<String>,
    pub sounds_enabled: Option<bool>,
    pub sound_volume: Option<f32>,
    pub start_sound: Option<String>,
    pub vocabulary: Option<Vec<String>>,
    pub modes: Option<Vec<Mode>>,
    pub active_mode: Option<String>,
    pub input_device: Option<String>,
    pub save_history: Option<bool>,
    pub ui_language: Option<String>,
}

fn one_of(value: String, allowed: &[&str], field: &str) -> Result<String, String> {
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(tf("settings.invalidValue", &[("field", &field), ("value", &value)]))
    }
}

impl Settings {
    /// Fill in what older files lack: a first mode built from the legacy
    /// model, and a valid active mode.
    pub fn normalize(&mut self) {
        if self.modes.is_empty() {
            self.modes = vec![
                Mode {
                    id: "default".into(),
                    name: "Default".into(),
                    icon: "sparkle".into(),
                    model: self.model.clone(),
                    language: String::new(),
                },
                Mode {
                    id: "local".into(),
                    name: "Local".into(),
                    icon: "laptop".into(),
                    model: "local:small".into(),
                    language: String::new(),
                },
            ];
        }
        if !self.modes.iter().any(|m| m.id == self.active_mode) {
            self.active_mode = self.modes[0].id.clone();
        }
        self.sound_volume = self.sound_volume.clamp(0.0, 1.0);
        if !crate::sounds::START_SOUNDS.contains(&self.start_sound.as_str()) {
            self.start_sound = default_start_sound();
        }
        let vocab = std::mem::take(&mut self.vocabulary);
        self.vocabulary = clean_vocabulary(vocab);
    }

    pub fn active(&self) -> &Mode {
        self.modes
            .iter()
            .find(|m| m.id == self.active_mode)
            .unwrap_or(&self.modes[0])
    }

    pub fn effective_model(&self) -> &str {
        &self.active().model
    }

    pub fn effective_language(&self) -> &str {
        &self.active().language
    }

    /// Change the active mode's model (pill dropdown / Models library).
    pub fn set_model(&mut self, model: String) {
        let id = self.active().id.clone();
        if let Some(m) = self.modes.iter_mut().find(|m| m.id == id) {
            m.model = model.clone();
        }
        self.model = model;
    }

    /// Activate the next mode (wrapping). Returns its name.
    pub fn cycle_mode(&mut self) -> String {
        let i = self
            .modes
            .iter()
            .position(|m| m.id == self.active_mode)
            .unwrap_or(0);
        let next = &self.modes[(i + 1) % self.modes.len()];
        self.active_mode = next.id.clone();
        self.model = next.model.clone();
        next.name.clone()
    }

    /// Whisper prompt built from the vocabulary: steers spelling of names and
    /// jargon without changing the meaning.
    pub fn vocabulary_prompt(&self) -> Option<String> {
        if self.vocabulary.is_empty() {
            None
        } else {
            Some(self.vocabulary.join(", "))
        }
    }

    /// Apply a partial update. Validates enums and mode lists; an empty key
    /// keeps the stored one.
    pub fn apply_patch(&mut self, patch: SettingsPatch) -> Result<(), String> {
        if let Some(key) = patch.openai_api_key {
            let key = key.trim();
            if !key.is_empty() {
                self.openai_api_key = key.to_string();
            }
        }
        if let Some(v) = patch.shortcut {
            self.shortcut = v;
        }
        if let Some(v) = patch.cancel_shortcut {
            self.cancel_shortcut = v;
        }
        if let Some(v) = patch.mode_shortcut {
            self.mode_shortcut = v;
        }
        if let Some(v) = patch.ptt_shortcut {
            self.ptt_shortcut = v;
        }
        if let Some(v) = patch.mouse_shortcut {
            self.mouse_shortcut = v;
        }
        if let Some(v) = patch.auto_paste {
            self.auto_paste = v;
        }
        if let Some(v) = patch.theme {
            self.theme = one_of(v, &["auto", "light", "dark"], "theme")?;
        }
        if let Some(v) = patch.pill_style {
            self.pill_style = one_of(v, &["classic", "mini", "none"], "pillStyle")?;
        }
        if let Some(v) = patch.pill_always_show {
            self.pill_always_show = v;
        }
        if let Some(v) = patch.pill_position {
            self.pill_position = one_of(v, &["top", "bottom"], "pillPosition")?;
        }
        if let Some(v) = patch.pill_skin {
            self.pill_skin = one_of(v, &["standard", "onyx"], "pillSkin")?;
        }
        if let Some(v) = patch.sounds_enabled {
            self.sounds_enabled = v;
        }
        if let Some(v) = patch.sound_volume {
            self.sound_volume = v.clamp(0.0, 1.0);
        }
        if let Some(v) = patch.start_sound {
            self.start_sound = one_of(v, crate::sounds::START_SOUNDS, "startSound")?;
        }
        if let Some(v) = patch.vocabulary {
            self.vocabulary = clean_vocabulary(v);
        }
        if let Some(modes) = patch.modes {
            if modes.is_empty() {
                return Err(t("settings.modes.empty"));
            }
            if modes.iter().any(|m| m.name.trim().is_empty() || m.id.is_empty()) {
                return Err(t("settings.modes.unnamed"));
            }
            self.modes = modes;
        }
        if let Some(v) = patch.active_mode {
            self.active_mode = v;
        }
        if let Some(v) = patch.input_device {
            self.input_device = v;
        }
        if let Some(v) = patch.save_history {
            self.save_history = v;
        }
        if let Some(v) = patch.ui_language {
            self.ui_language = one_of(v, crate::i18n::LANGUAGES, "uiLanguage")?;
        }
        self.normalize();
        self.model = self.effective_model().to_string();
        Ok(())
    }

    /// What gets written to `settings.json`: everything except the key when
    /// the OS credential store holds it.
    fn to_disk(&self) -> Settings {
        let mut disk = self.clone();
        if secret::AVAILABLE {
            disk.openai_api_key.clear();
        }
        disk
    }

    pub fn view(&self) -> SettingsView {
        SettingsView {
            api_key_configured: !self.openai_api_key.trim().is_empty(),
            model: self.effective_model().to_string(),
            language: self.effective_language().to_string(),
            shortcut: self.shortcut.clone(),
            cancel_shortcut: self.cancel_shortcut.clone(),
            mode_shortcut: self.mode_shortcut.clone(),
            ptt_shortcut: self.ptt_shortcut.clone(),
            mouse_shortcut: self.mouse_shortcut.clone(),
            auto_paste: self.auto_paste,
            theme: self.theme.clone(),
            pill_style: self.pill_style.clone(),
            pill_always_show: self.pill_always_show,
            pill_position: self.pill_position.clone(),
            pill_skin: self.pill_skin.clone(),
            sounds_enabled: self.sounds_enabled,
            sound_volume: self.sound_volume,
            start_sound: self.start_sound.clone(),
            vocabulary: self.vocabulary.clone(),
            modes: self.modes.clone(),
            active_mode: self.active_mode.clone(),
            input_device: self.input_device.clone(),
            save_history: self.save_history,
            ui_language: self.ui_language.clone(),
        }
    }
}

/// Trim, drop blanks and case-insensitive duplicates, keep order.
fn clean_vocabulary(words: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in words {
        let w = w.trim().to_string();
        if !w.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&w)) {
            out.push(w);
        }
    }
    out
}

fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| anyhow!(tf("error.configDir", &[("error", &e)])))?;
    Ok(dir.join("settings.json"))
}

/// Load settings, falling back to defaults if the file is missing or invalid.
///
/// A key found in the file (written by versions before the credential store)
/// is moved into the store and removed from the file.
pub fn load(app: &AppHandle) -> Settings {
    let mut settings: Settings = match settings_path(app).ok().and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        None => Settings::default(),
    };
    settings.normalize();
    if !secret::AVAILABLE {
        return settings;
    }
    if settings.openai_api_key.is_empty() {
        match secret::load() {
            Ok(key) => settings.openai_api_key = key.unwrap_or_default(),
            Err(e) => eprintln!("[whisperaround] {e}"),
        }
    } else if let Err(e) = save(app, &settings) {
        eprintln!("[whisperaround] moving the API key to the keychain: {e}");
    }
    settings
}

/// Persist settings to disk (and the key to the credential store), creating
/// the config dir if needed.
pub fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    if secret::AVAILABLE {
        secret::store(&settings.openai_api_key)?;
    }
    let write = || -> Result<()> {
        let path = settings_path(app)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&settings.to_disk())?;
        std::fs::write(path, json)?;
        Ok(())
    };
    write().map_err(|e| anyhow!(tf("settings.saveFailed", &[("error", &e)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-test-1234567890";

    fn with_key() -> Settings {
        Settings {
            openai_api_key: KEY.into(),
            ..Settings::default()
        }
    }

    #[test]
    fn view_never_contains_the_key() {
        let json = serde_json::to_string(&with_key().view()).unwrap();
        assert!(!json.contains(KEY), "key leaked to the webview payload: {json}");
        assert!(json.contains(r#""apiKeyConfigured":true"#));
    }

    #[test]
    fn view_reports_missing_key() {
        assert!(!Settings::default().view().api_key_configured);
        let blank = Settings {
            openai_api_key: "   ".into(),
            ..Settings::default()
        };
        assert!(!blank.view().api_key_configured);
    }

    #[test]
    fn blank_key_in_patch_keeps_stored_key() {
        let mut s = with_key();
        s.apply_patch(SettingsPatch {
            openai_api_key: Some("  ".into()),
            shortcut: Some("Alt+Space".into()),
            auto_paste: Some(false),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.openai_api_key, KEY);
        assert_eq!(s.shortcut, "Alt+Space");
        assert!(!s.auto_paste);
    }

    #[test]
    fn new_key_is_trimmed() {
        let mut s = with_key();
        s.apply_patch(SettingsPatch {
            openai_api_key: Some("  sk-new \n".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.openai_api_key, "sk-new");
    }

    #[test]
    fn old_settings_file_gets_defaults_and_a_mode_from_its_model() {
        // A 0.1.x file without the newer fields.
        let mut s: Settings =
            serde_json::from_str(r#"{"openai_api_key":"sk-x","model":"whisper-1"}"#).unwrap();
        s.normalize();
        assert_eq!(s.shortcut, default_shortcut());
        assert!(s.auto_paste);
        assert!(!s.onboarded);
        assert_eq!(s.pill_style, "mini");
        assert_eq!(s.pill_position, "top");
        assert_eq!(s.pill_skin, "standard");
        assert_eq!(s.cancel_shortcut, "Escape");
        assert_eq!(s.start_sound, "orus");
        assert_eq!(s.ui_language, "en");
        assert_eq!(s.effective_model(), "whisper-1");
        assert_eq!(s.active_mode, "default");
    }

    #[test]
    fn patch_rejects_unknown_enum_values() {
        let mut s = Settings::default();
        let err = s
            .apply_patch(SettingsPatch {
                pill_style: Some("huge".into()),
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.contains("pillStyle"));
        assert_eq!(s.pill_style, "mini");
    }

    #[test]
    fn set_model_edits_the_active_mode_and_cycle_wraps() {
        let mut s = Settings::default();
        s.set_model("whisper-1".into());
        assert_eq!(s.modes[0].model, "whisper-1");
        assert_eq!(s.cycle_mode(), "Local");
        assert_eq!(s.effective_model(), "local:small");
        assert_eq!(s.cycle_mode(), "Default");
        assert_eq!(s.effective_model(), "whisper-1");
    }

    #[test]
    fn unknown_active_mode_falls_back_to_the_first() {
        let mut s = Settings::default();
        s.apply_patch(SettingsPatch {
            active_mode: Some("gone".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.active_mode, "default");
    }

    #[test]
    fn vocabulary_is_cleaned_and_joined() {
        let mut s = Settings::default();
        s.apply_patch(SettingsPatch {
            vocabulary: Some(vec![" Tauri ".into(), "".into(), "tauri".into(), "Svelte".into()]),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(s.vocabulary_prompt().as_deref(), Some("Tauri, Svelte"));
    }

    #[test]
    fn key_stays_out_of_the_file_when_the_os_store_exists() {
        let disk = serde_json::to_string(&with_key().to_disk()).unwrap();
        assert_eq!(disk.contains(KEY), !secret::AVAILABLE);
    }
}
