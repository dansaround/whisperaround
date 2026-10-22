//! Transcription history and all-time usage stats (`history.json` in the app
//! data dir). The history is capped; the stats keep counting regardless, so the
//! Home page's "All time" numbers survive trimming or clearing the list.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Unix time in ms.
    pub ts: u64,
    pub text: String,
    pub words: u32,
    /// Length of the recorded audio.
    pub duration_ms: u64,
    pub model: String,
    pub mode: String,
    /// App that had focus when the recording started (macOS).
    pub app: Option<String>,
    /// A meeting transcript (multi-line text, never pasted).
    #[serde(default)]
    pub meeting: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stats {
    pub words: u64,
    pub audio_ms: u64,
    pub sessions: u64,
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// Oldest first.
    pub entries: Vec<Entry>,
    pub stats: Stats,
}

/// Managed state wrapper.
#[derive(Default)]
pub struct HistoryState(pub Mutex<History>);

impl History {
    fn add(&mut self, entry: Entry, keep: bool) {
        self.stats.words += entry.words as u64;
        self.stats.audio_ms += entry.duration_ms;
        self.stats.sessions += 1;
        if let Some(app) = &entry.app {
            if !self.stats.apps.contains(app) {
                self.stats.apps.push(app.clone());
            }
        }
        if keep {
            self.entries.push(entry);
            let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
            self.entries.drain(..excess);
        }
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow!(crate::i18n::tf("error.dataDir", &[("error", &e)])))?;
    Ok(dir.join("history.json"))
}

pub fn load(app: &AppHandle) -> History {
    path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, history: &History) -> Result<()> {
    let p = path(app)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(p, serde_json::to_vec(history)?)?;
    Ok(())
}

/// Record a finished transcription; the text itself is only kept when the
/// user has history enabled.
pub fn record(app: &AppHandle, entry: Entry, keep: bool) {
    let state = app.state::<HistoryState>();
    let mut h = state.0.lock().unwrap();
    h.add(entry.clone(), keep);
    if let Err(e) = save(app, &h) {
        eprintln!("[whisperaround] saving history: {e}");
    }
    drop(h);
    let _ = app.emit("history", ());
}

pub fn snapshot(app: &AppHandle) -> History {
    app.state::<HistoryState>().0.lock().unwrap().clone()
}

pub fn delete(app: &AppHandle, id: u64) -> Result<()> {
    let state = app.state::<HistoryState>();
    let mut h = state.0.lock().unwrap();
    h.entries.retain(|e| e.id != id);
    save(app, &h)?;
    drop(h);
    let _ = app.emit("history", ());
    Ok(())
}

pub fn clear(app: &AppHandle) -> Result<()> {
    let state = app.state::<HistoryState>();
    let mut h = state.0.lock().unwrap();
    h.entries.clear();
    save(app, &h)?;
    drop(h);
    let _ = app.emit("history", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, words: u32, app: Option<&str>) -> Entry {
        Entry {
            id,
            ts: id,
            text: "hola".into(),
            words,
            duration_ms: 1000,
            model: "whisper-1".into(),
            mode: "Default".into(),
            app: app.map(Into::into),
            meeting: false,
        }
    }

    #[test]
    fn stats_count_even_when_text_is_not_kept() {
        let mut h = History::default();
        h.add(entry(1, 5, Some("Notes")), true);
        h.add(entry(2, 7, Some("Notes")), false);
        h.add(entry(3, 1, Some("Mail")), true);
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.stats.words, 13);
        assert_eq!(h.stats.sessions, 3);
        assert_eq!(h.stats.apps, vec!["Notes", "Mail"]);
    }

    #[test]
    fn entries_saved_before_meetings_still_load() {
        let old = r#"{"id":1,"ts":1,"text":"hi","words":1,"durationMs":5,"model":"m","mode":"Default","app":null}"#;
        let e: Entry = serde_json::from_str(old).unwrap();
        assert!(!e.meeting);
    }

    #[test]
    fn history_is_capped_dropping_the_oldest() {
        let mut h = History::default();
        for i in 0..(MAX_ENTRIES as u64 + 5) {
            h.add(entry(i, 1, None), true);
        }
        assert_eq!(h.entries.len(), MAX_ENTRIES);
        assert_eq!(h.entries[0].id, 5);
    }
}
