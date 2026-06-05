//! Shared application state and the status state-machine.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::recorder::Recorder;
use crate::settings::Settings;

/// Lifecycle of the dictation pipeline. Serialized lowercase to match the
/// `AppStatus` union on the frontend (`src/lib/types.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Idle,
    Recording,
    Processing,
    Error,
}

/// Payload for the `status` event emitted to the webview.
#[derive(Serialize, Clone)]
struct StatusEvent {
    status: AppStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Process-wide state managed by Tauri (`app.manage`).
pub struct AppState {
    pub status: Mutex<AppStatus>,
    /// The active recorder while in `Recording`, taken out on stop.
    pub recorder: Mutex<Option<Recorder>>,
    pub settings: Mutex<Settings>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            status: Mutex::new(AppStatus::Idle),
            recorder: Mutex::new(None),
            settings: Mutex::new(settings),
        }
    }
}

/// Update the stored status and notify the frontend in one place.
pub fn set_status(app: &AppHandle, status: AppStatus, message: Option<String>) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.status.lock().unwrap() = status;
    }
    if let Some(ref msg) = message {
        eprintln!("[whisperaround] status={status:?} msg={msg}");
    }
    let _ = app.emit("status", StatusEvent { status, message });
}
