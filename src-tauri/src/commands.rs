//! Tauri commands exposed to the frontend (`src/lib/ipc.ts`).

use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::pipeline;
use crate::settings::{self, SettingsUpdate, SettingsView};
use crate::state::{AppState, AppStatus};
use crate::window;

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
/// An empty `openai_api_key` leaves the stored key untouched. If the shortcut
/// changed, it is re-registered live so no restart is needed.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    update: SettingsUpdate,
) -> Result<SettingsView, String> {
    let mut settings = state.settings.lock().unwrap();
    let old_shortcut = settings.shortcut.clone();

    if !update.openai_api_key.trim().is_empty() {
        settings.openai_api_key = update.openai_api_key.trim().to_string();
    }
    settings.model = update.model;
    settings.shortcut = update.shortcut;
    settings.auto_paste = update.auto_paste;

    if settings.shortcut != old_shortcut {
        rebind_shortcut(&app, &old_shortcut, &settings.shortcut)?;
    }

    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    Ok(settings.view())
}

/// Update only the transcription model (used by the pill's model dropdown),
/// leaving the API key and other settings untouched.
#[tauri::command]
pub fn set_model(app: AppHandle, state: State<AppState>, model: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.model = model;
    settings::save(&app, &settings).map_err(|e| e.to_string())
}

/// Fit the compact pill window to a UI mode ("idle" | "icons" | "tip" | "menu"
/// | "rec"). Anchored to its top-center so the pill grows in place and stays
/// wherever the user dragged it.
#[tauri::command]
pub fn set_pill_mode(app: AppHandle, mode: String) {
    if let Some(window) = app.get_webview_window("main") {
        window::resize_anchored(&window, window::size_for_mode(&mode), window::Anchor::TopCenter);
    }
}

/// Grow the floating window into the expanded panel, centered on its current
/// position (does not snap back to the top of the screen).
#[tauri::command]
pub fn expand_panel(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        window::resize_anchored(&window, window::PANEL_SIZE, window::Anchor::Center);
    }
}

/// Show and focus the standalone settings window.
#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings_window(&app);
}

/// Shared helper: reveal the settings window (used by the command and the tray).
pub fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Swap the registered global shortcut, restoring the old one on failure.
fn rebind_shortcut(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    let new_sc: Shortcut = new
        .parse()
        .map_err(|_| format!("atajo inválido: '{new}'"))?;

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    match gs.register(new_sc) {
        Ok(()) => {
            eprintln!("[whisperaround] global shortcut re-registered: {new}");
            Ok(())
        }
        Err(e) => {
            // Best-effort restore of the previous binding.
            if let Ok(old_sc) = old.parse::<Shortcut>() {
                let _ = gs.register(old_sc);
            }
            Err(format!("no se pudo registrar el atajo '{new}': {e}"))
        }
    }
}
