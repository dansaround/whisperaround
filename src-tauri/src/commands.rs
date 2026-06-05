//! Tauri commands exposed to the frontend (`src/lib/ipc.ts`).

use tauri::{AppHandle, State};

use crate::pipeline;
use crate::settings::{self, SettingsUpdate, SettingsView};
use crate::state::{AppState, AppStatus};

/// Current pipeline status (used to seed the UI on mount).
#[tauri::command]
pub fn get_status(state: State<AppState>) -> AppStatus {
    *state.status.lock().unwrap()
}

/// Manually toggle recording — equivalent to pressing the global shortcut.
#[tauri::command]
pub fn toggle_recording(app: AppHandle) {
    pipeline::toggle(&app);
}

/// Current settings, with the API key reduced to a boolean flag.
#[tauri::command]
pub fn load_settings(state: State<AppState>) -> SettingsView {
    state.settings.lock().unwrap().view()
}

/// Persist settings and return the refreshed view.
///
/// An empty `openai_api_key` leaves the stored key untouched.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    update: SettingsUpdate,
) -> Result<SettingsView, String> {
    let mut settings = state.settings.lock().unwrap();
    if !update.openai_api_key.trim().is_empty() {
        settings.openai_api_key = update.openai_api_key.trim().to_string();
    }
    settings.model = update.model;
    settings.shortcut = update.shortcut;
    settings.auto_paste = update.auto_paste;

    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    Ok(settings.view())
}
