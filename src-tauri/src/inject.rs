//! Clipboard write + simulated paste keystroke.

use anyhow::{anyhow, Result};
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "macos")]
use crate::i18n::t;
use crate::i18n::tf;

/// Write text to the system clipboard.
pub fn copy_to_clipboard(app: &AppHandle, text: &str) -> Result<()> {
    app.clipboard()
        .write_text(text.to_string())
        .map_err(|e| anyhow!(tf("error.clipboard", &[("error", &e)])))
}

/// Simulate the paste shortcut (Ctrl+V, or Cmd+V on macOS) in the focused app.
#[cfg(not(target_os = "macos"))]
pub fn paste(_app: &AppHandle) -> Result<()> {
    send_paste_keys()
}

/// On macOS the keystroke needs the Accessibility permission (otherwise it is
/// silently dropped) and has to be posted from the main thread: enigo resolves
/// the `V` key through the Text Input Sources API, which asserts it runs there.
#[cfg(target_os = "macos")]
pub fn paste(app: &AppHandle) -> Result<()> {
    if !crate::macos::accessibility_trusted() {
        crate::macos::request_accessibility();
        return Err(anyhow!(t("error.paste.accessibility")));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(send_paste_keys());
    })
    .map_err(|e| anyhow!(tf("error.paste.mainThread", &[("error", &e)])))?;
    rx.recv().map_err(|_| anyhow!(t("error.paste.notRun")))?
}

fn send_paste_keys() -> Result<()> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| anyhow!(tf("error.paste.keyboard", &[("error", &e)])))?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Press)
        .map_err(|e| anyhow!(tf("error.paste.modifier", &[("error", &e)])))?;
    enigo
        .key(Key::Unicode('v'), Click)
        .map_err(|e| anyhow!(tf("error.paste.keyV", &[("error", &e)])))?;
    enigo
        .key(modifier, Release)
        .map_err(|e| anyhow!(tf("error.paste.release", &[("error", &e)])))?;
    Ok(())
}
