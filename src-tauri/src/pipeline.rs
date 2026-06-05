//! The core toggle pipeline: Idle → Recording → Processing → Idle.
//!
//! Driven both by the global shortcut handler and the manual `toggle_recording`
//! command, so the logic lives here in one place.

use tauri::Manager;
use tauri::AppHandle;

use crate::settings::Settings;
use crate::state::{set_status, AppState, AppStatus};
use crate::{inject, recorder, whisper};

/// Advance the state machine based on the current status.
pub fn toggle(app: &AppHandle) {
    let current = {
        let state = app.state::<AppState>();
        let status = *state.status.lock().unwrap();
        status
    };

    match current {
        AppStatus::Idle | AppStatus::Error => start(app),
        AppStatus::Recording => stop_and_transcribe(app),
        // Busy transcribing — ignore further toggles until done.
        AppStatus::Processing => {}
    }
}

fn start(app: &AppHandle) {
    let state = app.state::<AppState>();
    match recorder::Recorder::start() {
        Ok(rec) => {
            *state.recorder.lock().unwrap() = Some(rec);
            set_status(app, AppStatus::Recording, None);
        }
        Err(e) => set_status(app, AppStatus::Error, Some(format!("micrófono: {e}"))),
    }
}

fn stop_and_transcribe(app: &AppHandle) {
    let state = app.state::<AppState>();

    let Some(rec) = state.recorder.lock().unwrap().take() else {
        set_status(app, AppStatus::Idle, None);
        return;
    };
    set_status(app, AppStatus::Processing, None);

    let recording = match rec.stop() {
        Ok(r) => r,
        Err(e) => {
            set_status(app, AppStatus::Error, Some(format!("grabación: {e}")));
            return;
        }
    };

    let wav = match recorder::encode_wav(&recording) {
        Ok(w) => w,
        Err(e) => {
            set_status(app, AppStatus::Error, Some(format!("codificando WAV: {e}")));
            return;
        }
    };

    let settings: Settings = state.settings.lock().unwrap().clone();
    let app = app.clone();

    // Off-load the network call so the UI thread stays responsive.
    tauri::async_runtime::spawn(async move {
        match whisper::transcribe(&settings.openai_api_key, &settings.model, wav).await {
            Ok(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    set_status(&app, AppStatus::Idle, None);
                    return;
                }
                if let Err(e) = inject::copy_to_clipboard(&app, &text) {
                    set_status(&app, AppStatus::Error, Some(e.to_string()));
                    return;
                }
                if settings.auto_paste {
                    // Let the clipboard write settle before the paste keystroke.
                    std::thread::sleep(std::time::Duration::from_millis(120));
                    if let Err(e) = inject::paste() {
                        set_status(&app, AppStatus::Error, Some(format!("auto-paste: {e}")));
                        return;
                    }
                }
                set_status(&app, AppStatus::Idle, None);
            }
            Err(e) => set_status(&app, AppStatus::Error, Some(e.to_string())),
        }
    });
}
