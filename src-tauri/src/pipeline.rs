//! The core toggle pipeline: Idle → Recording → Processing → Idle.
//!
//! Driven both by the global shortcut handler and the manual `toggle_recording`
//! command, so the logic lives here in one place.

use tauri::{AppHandle, Emitter, Manager};

use crate::history::{self, Entry};
use crate::i18n::{t, tf};
use crate::sounds::{self, Sound};
use crate::state::{set_status, AppState, AppStatus};
use crate::{inject, local, recorder, whisper};

/// Translation key for "the microphone picked up nothing".
#[cfg(target_os = "macos")]
const SILENT_MIC: &str = "error.mic.silent";
#[cfg(not(target_os = "macos"))]
const SILENT_MIC: &str = "error.mic.silentMuted";

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
    #[cfg(target_os = "macos")]
    if let Err(msg) = check_mic_permission() {
        set_status(app, AppStatus::Error, Some(msg));
        return;
    }

    let state = app.state::<AppState>();
    #[cfg(target_os = "macos")]
    {
        *state.recording_app.lock().unwrap() = crate::macos::frontmost_app_name();
    }
    #[cfg(windows)]
    {
        *state.recording_app.lock().unwrap() = crate::win::frontmost_app_name();
    }
    // Plays while the mic opens; short enough not to matter to Whisper.
    sounds::play(app, Sound::Start);
    let device = state.settings.lock().unwrap().input_device.clone();
    // Live input level for the pill's waveform (`level` event, 0.0..=1.0).
    let level_app = app.clone();
    let on_level = move |level: f32| {
        let _ = level_app.emit("level", level);
    };
    match recorder::Recorder::start(&device, on_level) {
        Ok(rec) => {
            *state.recorder.lock().unwrap() = Some(rec);
            set_status(app, AppStatus::Recording, None);
        }
        Err(e) => set_status(app, AppStatus::Error, Some(tf("error.mic.open", &[("error", &e)]))),
    }
}

/// Discard the current recording without transcribing it.
pub fn cancel(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(rec) = state.recorder.lock().unwrap().take() else {
        return;
    };
    let _ = rec.stop();
    // Cancelling is deliberately silent.
    set_status(app, AppStatus::Idle, None);
}

fn stop_and_transcribe(app: &AppHandle) {
    let state = app.state::<AppState>();

    let Some(rec) = state.recorder.lock().unwrap().take() else {
        set_status(app, AppStatus::Idle, None);
        return;
    };
    sounds::play(app, Sound::Stop);
    set_status(app, AppStatus::Processing, None);

    let recording = match rec.stop() {
        Ok(r) => r,
        Err(e) => {
            set_status(app, AppStatus::Error, Some(tf("error.recording.failed", &[("error", &e)])));
            return;
        }
    };

    // Exact digital silence means the OS handed us a muted stream (on macOS:
    // microphone permission denied), not a quiet room.
    if recording.samples.iter().all(|&s| s == 0.0) {
        set_status(app, AppStatus::Error, Some(t(SILENT_MIC)));
        return;
    }

    let wav = match recorder::encode_wav(&recording) {
        Ok(w) => w,
        Err(e) => {
            set_status(app, AppStatus::Error, Some(tf("error.recording.encode", &[("error", &e)])));
            return;
        }
    };

    let frames = recording.samples.len() / recording.channels.max(1) as usize;
    let duration_ms = frames as u64 * 1000 / recording.sample_rate.max(1) as u64;
    let focused_app = state.recording_app.lock().unwrap().take();
    let settings = state.settings.lock().unwrap().clone();
    let model = settings.effective_model().to_string();
    let opts = whisper::Options {
        language: settings.effective_language().to_string(),
        prompt: settings.vocabulary_prompt(),
    };
    let app = app.clone();

    // Off-load the network call / local inference so the UI stays responsive.
    tauri::async_runtime::spawn(async move {
        let result = match local::model_name(&model) {
            Some(name) => {
                let (app2, name, opts) = (app.clone(), name.to_string(), opts.clone());
                tauri::async_runtime::spawn_blocking(move || {
                    local::transcribe(&app2, &name, &wav, &opts)
                })
                .await
                .unwrap_or_else(|e| Err(anyhow::anyhow!(tf("error.local.failed", &[("error", &e)]))))
            }
            None => whisper::transcribe(&settings.openai_api_key, &model, wav, &opts).await,
        };
        match result {
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
                    if let Err(e) = inject::paste(&app) {
                        set_status(&app, AppStatus::Error, Some(tf("error.autoPaste", &[("error", &e)])));
                        return;
                    }
                }
                set_status(&app, AppStatus::Idle, None);
                let entry = Entry {
                    id: history::now_ms(),
                    ts: history::now_ms(),
                    words: text.split_whitespace().count() as u32,
                    text,
                    duration_ms,
                    model,
                    mode: settings.active().name.clone(),
                    app: focused_app,
                };
                history::record(&app, entry, settings.save_history);
            }
            Err(e) => set_status(&app, AppStatus::Error, Some(e.to_string())),
        }
    });
}

/// Fail fast when macOS will not give us the microphone: recording anyway
/// would only capture silence.
#[cfg(target_os = "macos")]
fn check_mic_permission() -> Result<(), String> {
    use crate::macos::{self, MicPermission};
    match macos::microphone() {
        MicPermission::Granted => Ok(()),
        MicPermission::Undetermined => {
            macos::request_microphone();
            Err(t("error.mic.allowAccess"))
        }
        MicPermission::Denied => {
            macos::open_privacy_settings("Privacy_Microphone");
            Err(t("error.mic.blocked"))
        }
    }
}
