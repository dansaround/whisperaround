//! WhisperAround — application setup: state, global shortcut, system tray.

mod commands;
mod inject;
mod pipeline;
mod recorder;
mod settings;
mod state;
mod whisper;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use state::AppState;

const FALLBACK_SHORTCUT: &str = "CmdOrCtrl+Shift+X";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Load persisted settings and seed shared state.
            let loaded = settings::load(&app.handle());
            let shortcut_str = loaded.shortcut.clone();
            app.manage(AppState::new(loaded));

            setup_global_shortcut(app, &shortcut_str)?;
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Keep running in the background (tray) instead of quitting on close.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::toggle_recording,
            commands::load_settings,
            commands::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Register the configured global shortcut; the handler drives the toggle pipeline.
fn setup_global_shortcut(
    app: &mut tauri::App,
    shortcut_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut: Shortcut = shortcut_str
        .parse()
        .or_else(|_| FALLBACK_SHORTCUT.parse())
        .expect("valid fallback shortcut");

    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, _shortcut, event| {
                // Fire on key-down only; toggle ignores repeated presses while busy.
                if event.state() == ShortcutState::Pressed {
                    pipeline::toggle(app);
                }
            })
            .build(),
    )?;
    app.global_shortcut().register(shortcut)?;
    Ok(())
}

/// Build a minimal tray (Show / Quit) so the app can live in the background.
fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Mostrar", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("missing default window icon")?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("WhisperAround")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
