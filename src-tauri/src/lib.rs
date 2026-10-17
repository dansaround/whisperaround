//! WhisperAround — application setup: state, global shortcut, system tray.

mod commands;
mod downloads;
mod history;
mod i18n;
mod inject;
mod local;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_input;
#[cfg(any(target_os = "macos", windows))]
mod native_input;
mod pipeline;
mod recorder;
mod secret;
mod settings;
mod shortcuts;
mod sounds;
mod state;
mod whisper;
#[cfg(windows)]
mod win;
mod window;
#[cfg(windows)]
mod windows_input;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

use settings::Settings;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Launching the app again while it runs (Start menu, desktop shortcut)
    // brings up the settings window, like `RunEvent::Reopen` on macOS. Must
    // be the first plugin.
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        commands::show_settings_window(app);
    }));
    builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Manage the state up-front (before setup) so the webview can never win
        // a race and invoke a `State<AppState>` command before it exists. Setup
        // then loads the persisted settings into this already-managed state.
        .manage(AppState::new(Settings::default()))
        .manage(shortcuts::ShortcutState::default())
        .manage(history::HistoryState::default())
        .setup(|app| {
            // Menu-bar app on macOS: no Dock icon or Cmd+Tab entry (the bundle's
            // Info.plist says the same; this covers `tauri dev`).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Load persisted settings into the already-managed state.
            let loaded = settings::load(&app.handle());
            let model = loaded.effective_model().to_string();
            let onboarded = loaded.onboarded;
            let bottom = loaded.pill_position == "bottom";
            let hide_pill = loaded.pill_style == "none";
            i18n::set_language(&loaded.ui_language);
            app.set_theme(commands::theme_of(&loaded.theme));
            *app.state::<AppState>().settings.lock().unwrap() = loaded;
            *app.state::<history::HistoryState>().0.lock().unwrap() = history::load(&app.handle());
            window::localize_titles(&app.handle());
            // Preload a local faster-whisper model if it is the active one.
            commands::warm_local_model(&app.handle(), &model);

            // Pin the floating pill to the top-center of the screen on launch.
            // Also disable the OS window shadow at runtime (belt-and-suspenders
            // with the config flag) so the transparent window shows no box.
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.set_shadow(false);
                #[cfg(target_os = "macos")]
                {
                    macos::make_overlay(&main);
                    macos::unlock_high_refresh(&main);
                }
                window::layout_main(&main, window::PILL_SIZE, bottom);
                if hide_pill {
                    let _ = main.hide();
                }
            }
            #[cfg(target_os = "macos")]
            if let Some(settings) = app.get_webview_window("settings") {
                macos::unlock_high_refresh(&settings);
            }
            // The welcome window also lists the macOS permissions there.
            #[cfg(target_os = "macos")]
            if let Some(welcome) = app.get_webview_window("welcome") {
                let _ = welcome.set_size(tauri::LogicalSize::new(460.0, 680.0));
                let _ = welcome.center();
            }

            setup_global_shortcut(app)?;
            #[cfg(target_os = "macos")]
            macos_input::install(&app.handle());
            #[cfg(windows)]
            windows_input::install(app.handle());
            setup_tray(app)?;
            if !onboarded {
                commands::show_welcome_window(&app.handle());
            } else {
                // Launched by hand (Spotlight, Finder, Launchpad): open the
                // settings so there's something to see.
                commands::show_settings_window(&app.handle());
            }
            // Dev builds aren't released, so there is nothing to update to.
            if !cfg!(debug_assertions) {
                commands::check_update_on_launch(&app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Keep running in the background (tray) instead of quitting on close.
            // Closing the pill hides it to the tray; closing settings just hides it.
            // A shortcut capture must never outlive the settings window.
            if window.label() == "settings" {
                if let WindowEvent::Focused(false) | WindowEvent::CloseRequested { .. } = event {
                    shortcuts::end_capture(window.app_handle());
                }
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
                // Closing the welcome window counts as dismissing it.
                if window.label() == "welcome" {
                    let app = window.app_handle();
                    let _ = commands::finish_onboarding(app.clone(), app.state::<AppState>());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::toggle_recording,
            commands::load_settings,
            commands::update_settings,
            commands::set_model,
            commands::cancel_recording,
            commands::set_pill_interactive,
            commands::begin_shortcut_capture,
            commands::end_shortcut_capture,
            commands::list_input_devices,
            commands::preview_sound,
            commands::get_history,
            commands::delete_history_entry,
            commands::clear_history,
            commands::open_releases,
            commands::fit_window,
            commands::open_settings,
            commands::check_python,
            commands::open_python_install,
            commands::check_permissions,
            commands::window_material,
            commands::request_permission,
            commands::finish_onboarding,
            commands::check_update,
            commands::install_update,
            commands::local_models,
            commands::download_model,
            commands::cancel_model_download,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Opening the already-running app again (Spotlight, Finder, the
            // Dock) brings up the settings window.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                commands::show_settings_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

/// Install the global-shortcut plugin (every binding goes through
/// `shortcuts::on_accelerator`) and register the configured bindings.
fn setup_global_shortcut(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, shortcut, event| {
                shortcuts::on_accelerator(app, shortcut, event.state());
            })
            .build(),
    )?;

    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if let Err(e) = shortcuts::apply(&app.handle(), &settings) {
        eprintln!("[whisperaround] shortcuts: {e}");
    }
    Ok(())
}

/// Build a minimal tray (Show pill / Settings / Welcome / Quit) so the app lives in the
/// background. Left-clicking the icon shows the pill.
fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = tray_menu(app.handle())?;

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
            "welcome" => commands::show_welcome_window(app),
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

/// Tray menu in the current UI language.
fn tray_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // "Toggle Recording" works regardless of window focus, so it doubles as a
    // background trigger when the global shortcut can't fire (e.g. under WSLg).
    let toggle = MenuItem::with_id(app, "toggle", i18n::t("tray.toggle"), true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", i18n::t("tray.show"), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", i18n::t("tray.settings"), true, None::<&str>)?;
    let welcome = MenuItem::with_id(app, "welcome", i18n::t("tray.welcome"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::t("tray.quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&toggle, &show, &settings, &welcome, &quit])
}

/// Rebuild the tray menu after the UI language changed.
pub fn refresh_tray(app: &tauri::AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Show the floating pill, re-pinned to its screen edge. With "always show"
/// off it is briefly revealed; with the "none" style there is no pill, so the
/// settings open instead.
fn show_pill(app: &tauri::AppHandle) {
    let (style, bottom) = {
        let s = app.state::<AppState>();
        let s = s.settings.lock().unwrap();
        (s.pill_style.clone(), s.pill_position == "bottom")
    };
    if style == "none" {
        commands::show_settings_window(app);
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        window::layout_main(&window, window::PILL_SIZE, bottom);
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit_to("main", "reveal-pill", ());
    }
}
