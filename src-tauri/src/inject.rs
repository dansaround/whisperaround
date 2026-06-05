//! Clipboard write + simulated paste keystroke.

use anyhow::{anyhow, Result};
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Write text to the system clipboard.
pub fn copy_to_clipboard(app: &AppHandle, text: &str) -> Result<()> {
    app.clipboard()
        .write_text(text.to_string())
        .map_err(|e| anyhow!("portapapeles: {e}"))
}

/// Simulate the paste shortcut (Ctrl+V, or Cmd+V on macOS) in the focused app.
pub fn paste() -> Result<()> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| anyhow!("inicializando enigo: {e}"))?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Press)
        .map_err(|e| anyhow!("tecla modificadora: {e}"))?;
    enigo
        .key(Key::Unicode('v'), Click)
        .map_err(|e| anyhow!("tecla V: {e}"))?;
    enigo
        .key(modifier, Release)
        .map_err(|e| anyhow!("soltando modificadora: {e}"))?;
    Ok(())
}
