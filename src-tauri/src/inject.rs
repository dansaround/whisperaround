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

/// Name of the app that has the focus now, as `win::frontmost_app_name` /
/// `macos::frontmost_app_name` report it. `None` elsewhere or when it is
/// WhisperAround itself.
#[cfg(windows)]
pub fn frontmost_app() -> Option<String> {
    crate::win::frontmost_app_name()
}

/// See the Windows version.
#[cfg(target_os = "macos")]
pub fn frontmost_app() -> Option<String> {
    crate::macos::frontmost_app_name()
}

/// See the Windows version.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn frontmost_app() -> Option<String> {
    None
}

/// Whether the focus is still in `start`, the app that was in front when the
/// recording began. Without a known app (and on other platforms) it is `false`,
/// so nothing is typed blind into whatever window is in front.
pub fn same_app(start: &Option<String>) -> bool {
    start.is_some() && frontmost_app() == *start
}

/// Type `text` into the focused app, as the keyboard would (no clipboard).
#[cfg(not(target_os = "macos"))]
pub fn type_text(_app: &AppHandle, text: &str) -> Result<()> {
    #[cfg(windows)]
    wait_for_modifiers_released()?;
    send_text(text)
}

/// Wait until Ctrl, Alt, Shift and Win are up: typed letters with a modifier
/// still held (push-to-talk keeps the shortcut down) would become shortcuts
/// like Ctrl+A in the target app. Gives up after a while.
#[cfg(windows)]
fn wait_for_modifiers_released() -> Result<()> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    let held = || {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            // SAFETY: plain state query, no pointers.
            .any(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0)
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while held() {
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("modifier keys held down"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(())
}

/// On macOS it needs the Accessibility permission and the main thread, like
/// [`paste`]. Call it from a worker thread: it waits for the main thread.
#[cfg(target_os = "macos")]
pub fn type_text(app: &AppHandle, text: &str) -> Result<()> {
    if !crate::macos::accessibility_trusted() {
        crate::macos::request_accessibility();
        return Err(anyhow!(t("error.paste.accessibility")));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let text = text.to_string();
    app.run_on_main_thread(move || {
        let _ = tx.send(send_text(&text));
    })
    .map_err(|e| anyhow!(tf("error.paste.mainThread", &[("error", &e)])))?;
    rx.recv().map_err(|_| anyhow!(t("error.paste.notRun")))?
}

fn send_text(text: &str) -> Result<()> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| anyhow!(tf("error.paste.keyboard", &[("error", &e)])))?;
    enigo
        .text(text)
        .map_err(|e| anyhow!(tf("error.paste.keyboard", &[("error", &e)])))
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
