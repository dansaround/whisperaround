//! WhisperAround — application setup: state, global shortcut, system tray.

mod commands;
mod inject;
mod pipeline;
mod recorder;
mod settings;
mod state;
mod whisper;
mod window;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
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

            // Pin the floating pill to the top-center of the screen on launch.
            // Also disable the OS window shadow at runtime (belt-and-suspenders
            // with the config flag) so the transparent window shows no box.
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.set_shadow(false);
                window::layout_main(&main, window::pill_size());
            }

            setup_global_shortcut(app, &shortcut_str)?;
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Keep running in the background (tray) instead of quitting on close.
            // Closing the pill hides it to the tray; closing settings just hides it.
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
            commands::set_model,
            commands::set_pill_mode,
            commands::expand_panel,
            commands::open_settings,
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

    match app.global_shortcut().register(shortcut) {
        Ok(()) => eprintln!("[whisperaround] global shortcut registered: {shortcut_str}"),
        Err(e) => eprintln!("[whisperaround] failed to register shortcut '{shortcut_str}': {e}"),
    }
    Ok(())
}

/// Build a minimal tray (Show pill / Settings / Quit) so the app lives in the
/// background. Left-clicking the icon shows the pill.
fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // "Toggle Recording" works regardless of window focus, so it doubles as a
    // background trigger when the global shortcut can't fire (e.g. under WSLg).
    let toggle = MenuItem::with_id(app, "toggle", "Toggle Recording", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Mostrar píldora", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &show, &settings, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("missing default window icon")?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("WhisperAround")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => pipeline::toggle(app),
            "show" => show_pill(app),
            "settings" => commands::show_settings_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click on the tray icon brings the pill back.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_pill(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Show + focus the floating pill, re-pinning it to the top-center.
fn show_pill(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        window::layout_main(&window, window::pill_size());
        let _ = window.show();
        let _ = window.set_focus();
    }
}
