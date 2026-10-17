//! Tauri commands exposed to the frontend (`src/lib/ipc.ts`).

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::downloads;
use crate::history;
use crate::i18n::tf;
use crate::local;
use crate::pipeline;
use crate::recorder;
use crate::settings::{self, Settings, SettingsPatch, SettingsView};
use crate::shortcuts;
use crate::sounds;
use crate::state::{set_status, AppState, AppStatus};
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

/// Apply a partial settings update, persist it and return the refreshed view.
///
/// An empty `openaiApiKey` leaves the stored key untouched. Shortcut changes
/// are re-registered live (rolled back if registration fails), and appearance
/// changes are pushed to every window via `settings-changed`.
#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    patch: SettingsPatch,
) -> Result<SettingsView, String> {
    let mut settings = state.settings.lock().unwrap();
    let old = settings.clone();
    settings.apply_patch(patch)?;

    if shortcuts_differ(&old, &settings) {
        if let Err(e) = shortcuts::apply(&app, &settings) {
            *settings = old;
            let _ = shortcuts::apply(&app, &settings);
            return Err(e);
        }
    }

    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    let view = settings.view();
    let snapshot = settings.clone();
    drop(settings);

    if old.ui_language != snapshot.ui_language {
        crate::i18n::set_language(&snapshot.ui_language);
        crate::refresh_tray(&app);
        window::localize_titles(&app);
    }
    if old.effective_model() != snapshot.effective_model() {
        warm_local_model(&app, snapshot.effective_model());
    }
    apply_appearance(&app, &old, &snapshot);
    let _ = app.emit("settings-changed", view.clone());
    Ok(view)
}

fn shortcuts_differ(a: &Settings, b: &Settings) -> bool {
    (&a.shortcut, &a.cancel_shortcut, &a.mode_shortcut, &a.ptt_shortcut, &a.mouse_shortcut)
        != (&b.shortcut, &b.cancel_shortcut, &b.mode_shortcut, &b.ptt_shortcut, &b.mouse_shortcut)
}

/// Push appearance changes to the windows: native theme (title bars, and the
/// webviews' `prefers-color-scheme`), pill position and the "none" style.
fn apply_appearance(app: &AppHandle, old: &Settings, new: &Settings) {
    if old.theme != new.theme {
        app.set_theme(theme_of(&new.theme));
    }
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let bottom = new.pill_position == "bottom";
    if old.pill_position != new.pill_position {
        let size = main
            .inner_size()
            .ok()
            .zip(main.scale_factor().ok())
            .map(|(s, f)| (s.width as f64 / f, s.height as f64 / f))
            .unwrap_or(window::PILL_SIZE);
        window::layout_main(&main, size, bottom);
    }
    if (old.pill_style == "none") != (new.pill_style == "none") {
        if new.pill_style == "none" {
            let _ = main.hide();
        } else {
            window::layout_main(&main, window::PILL_SIZE, bottom);
            let _ = main.show();
        }
    }
}

pub fn theme_of(theme: &str) -> Option<tauri::Theme> {
    match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// Update only the active mode's transcription model (pill dropdown, Models
/// library), leaving everything else untouched.
#[tauri::command]
pub fn set_model(app: AppHandle, state: State<AppState>, model: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.set_model(model.clone());
    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    let view = settings.view();
    drop(settings);
    warm_local_model(&app, &model);
    let _ = app.emit("settings-changed", view);
    Ok(())
}

/// Activate the next mode ("Change mode" shortcut). The pill shows its name.
pub fn cycle_mode(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut settings = state.settings.lock().unwrap();
    if settings.modes.len() < 2 {
        return;
    }
    let name = settings.cycle_mode();
    if let Err(e) = settings::save(app, &settings) {
        eprintln!("[whisperaround] saving mode: {e}");
    }
    let view = settings.view();
    let model = settings.effective_model().to_string();
    drop(settings);
    warm_local_model(app, &model);
    let _ = app.emit("settings-changed", view);
    let _ = app.emit("mode-changed", name);
}

/// Discard the current recording (pill × button).
#[tauri::command]
pub fn cancel_recording(app: AppHandle) {
    shortcuts::set_ptt(&app, false);
    pipeline::cancel(&app);
}

/// Let clicks through the pill window while the pill is hidden (always-show
/// off). The window itself stays on screen: bringing it back with `show()`
/// would steal focus on macOS and break auto-paste.
#[tauri::command]
pub fn set_pill_interactive(app: AppHandle, interactive: bool) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_ignore_cursor_events(!interactive);
    }
}

/// The settings window is recording a new shortcut: mute every binding.
#[tauri::command]
pub fn begin_shortcut_capture(app: AppHandle) {
    shortcuts::begin_capture(&app);
}

#[tauri::command]
pub fn end_shortcut_capture(app: AppHandle) {
    shortcuts::end_capture(&app);
}

/// Microphones for the settings picker. Enumerating devices can block, so it
/// runs off the main thread.
#[tauri::command]
pub async fn list_input_devices() -> Vec<recorder::InputDevice> {
    tauri::async_runtime::spawn_blocking(recorder::input_devices)
        .await
        .unwrap_or_default()
}

/// Play a sound from the Sound page (explicit start cue and volume, even with
/// sounds switched off, so the user can audition them).
#[tauri::command]
pub fn preview_sound(start_sound: String, sound: String, volume: f32) -> Result<(), String> {
    let sound = sounds::Sound::parse(&sound).ok_or_else(|| tf("error.sound.unknown", &[("sound", &sound)]))?;
    sounds::play_with(&start_sound, sound, volume);
    Ok(())
}

#[tauri::command]
pub fn get_history(app: AppHandle) -> history::History {
    history::snapshot(&app)
}

#[tauri::command]
pub fn delete_history_entry(app: AppHandle, id: u64) -> Result<(), String> {
    history::delete(&app, id).map_err(|e| tf("error.history.save", &[("error", &e)]))
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    history::clear(&app).map_err(|e| tf("error.history.save", &[("error", &e)]))
}

/// If `model` is a local faster-whisper model, prepare it in the background.
/// The first-time venv install is surfaced as `Processing` so the pill shows
/// it is busy; a plain model load stays silent.
pub fn warm_local_model(app: &AppHandle, model: &str) {
    let Some(name) = local::model_name(model) else {
        return;
    };
    let (app, name) = (app.clone(), name.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let installing = local::needs_setup(&app);
        if installing {
            set_status(&app, AppStatus::Processing, None);
        }
        match local::warm(&app, &name) {
            // Only clear the busy state we set (don't clobber a new recording).
            Ok(()) if installing => {
                let current = *app.state::<AppState>().status.lock().unwrap();
                if current == AppStatus::Processing {
                    set_status(&app, AppStatus::Idle, None);
                }
            }
            Ok(()) => {}
            Err(e) => set_status(&app, AppStatus::Error, Some(e.to_string())),
        }
    });
}

/// Downloadable local models available on this machine and their state.
#[tauri::command]
pub fn local_models(app: AppHandle) -> Vec<downloads::LocalModel> {
    downloads::list(&app)
}

/// Download a local model (progress on `model-download`), then preload it if
/// it is the active one.
#[tauri::command]
pub async fn download_model(app: AppHandle, model: String) -> Result<(), String> {
    let name = local::model_name(&model).ok_or_else(|| tf("error.model.notLocal", &[("model", &model)]))?;
    downloads::download(&app, name).await.map_err(|e| e.to_string())?;
    let active = app.state::<AppState>().settings.lock().unwrap().effective_model().to_string();
    if active == model {
        warm_local_model(&app, &model);
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_model_download() {
    downloads::cancel();
}

/// Resize the floating window to `width`×`height` logical px. The frontend
/// decides sizes so it can sequence them around its morph animations. Kept
/// anchored to the top-center so the pill grows in place, wherever it was
/// dragged.
#[tauri::command]
pub fn fit_window(app: AppHandle, width: f64, height: f64) {
    if let Some(window) = app.get_webview_window("main") {
        window::resize_anchored(&window, (width, height), window::pill_at_bottom(&app));
    }
}

/// Version of the system Python 3 (e.g. `"Python 3.12.7"`), or `None` if it
/// is missing. Only local faster-whisper models need it. Runs off the main
/// thread since probing the launchers spawns processes.
#[tauri::command]
pub async fn check_python() -> Option<String> {
    tauri::async_runtime::spawn_blocking(local::detect_python)
        .await
        .ok()
        .flatten()
}

/// Microsoft Store page for Python 3.12 (well covered by faster-whisper wheels).
const PYTHON_STORE_URL: &str = "ms-windows-store://pdp/?productid=9NCVDN91XZQP";
const PYTHON_DOWNLOAD_URL: &str = "https://www.python.org/downloads/";

/// Open the Python install page: the Microsoft Store on Windows, python.org
/// elsewhere.
#[tauri::command]
pub fn open_python_install() -> Result<(), String> {
    let result = if cfg!(windows) {
        std::process::Command::new("explorer").arg(PYTHON_STORE_URL).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(PYTHON_DOWNLOAD_URL).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(PYTHON_DOWNLOAD_URL).spawn()
    };
    result.map(|_| ()).map_err(|e| tf("error.python.openPage", &[("error", &e)]))
}

/// macOS privacy permissions (microphone / Accessibility) for the welcome
/// window; `None` on other platforms, where nothing has to be granted.
#[tauri::command]
pub fn check_permissions() -> Option<serde_json::Value> {
    #[cfg(target_os = "macos")]
    return serde_json::to_value(crate::macos::permissions()).ok();
    #[cfg(not(target_os = "macos"))]
    None
}

/// Native backdrop behind the settings window (`windowEffects` in
/// tauri.conf.json): macOS vibrancy, Windows 11 Mica, or `"none"` (opaque).
#[tauri::command]
pub fn window_material() -> &'static str {
    #[cfg(target_os = "macos")]
    return "vibrancy";
    #[cfg(windows)]
    return if crate::win::mica_supported() { "mica" } else { "none" };
    #[cfg(not(any(target_os = "macos", windows)))]
    "none"
}

/// Ask macOS for a permission (`"microphone"` or `"accessibility"`). Once the
/// user has answered the system prompt, only System Settings can change it, so
/// that pane is opened instead.
#[tauri::command]
pub fn request_permission(kind: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::macos::{self, MicPermission};
        match kind.as_str() {
            "microphone" if macos::microphone() == MicPermission::Undetermined => {
                macos::request_microphone()
            }
            "microphone" => macos::open_privacy_settings("Privacy_Microphone"),
            "accessibility" => {
                if !macos::request_accessibility() {
                    macos::open_privacy_settings("Privacy_Accessibility");
                }
            }
            other => return Err(tf("error.permission.unknown", &[("kind", &other)])),
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Ok(())
    }
}

/// Dismiss the welcome window for good (persisted in settings).
#[tauri::command]
pub fn finish_onboarding(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.onboarded = true;
    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window("welcome") {
        let _ = window.hide();
    }
    Ok(())
}

/// Reveal the welcome window (first launch, or from the tray).
pub fn show_welcome_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("welcome") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Version of a newer release published on GitHub, or `None` if up to date.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    let check_failed = |e: tauri_plugin_updater::Error| tf("error.update.check", &[("error", &e)]);
    let updater = app.updater().map_err(check_failed)?;
    let update = updater.check().await.map_err(check_failed)?;
    Ok(update.map(|u| u.version))
}

/// Download the newer release (signature-checked against the public key in
/// `tauri.conf.json`), install it and restart.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let check_failed = |e: tauri_plugin_updater::Error| tf("error.update.check", &[("error", &e)]);
    let updater = app.updater().map_err(check_failed)?;
    let Some(update) = updater.check().await.map_err(check_failed)? else {
        return Ok(());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| tf("error.update.install", &[("error", &e)]))?;
    app.restart();
}

/// On launch, look for a newer release and, if there is one, open the
/// settings window where the update banner offers to install it.
pub fn check_update_on_launch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match check_update(app.clone()).await {
            Ok(Some(version)) => {
                eprintln!("[whisperaround] update available: {version}");
                show_settings_window(&app);
            }
            Ok(None) => {}
            Err(e) => eprintln!("[whisperaround] update check failed: {e}"),
        }
    });
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

const RELEASES_URL: &str = "https://github.com/dansaround/whisperaround/releases";

/// Open the release notes on GitHub ("View all changes").
#[tauri::command]
pub fn open_releases() -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(RELEASES_URL)
        .spawn()
        .map(|_| ())
        .map_err(|e| tf("error.browser.open", &[("error", &e)]))
}
