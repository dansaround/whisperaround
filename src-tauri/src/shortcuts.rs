//! Global shortcuts: one dispatcher for every action.
//!
//! A binding is stored as a string:
//! - `"Super+Shift+KeyX"` — an accelerator, registered with the global-shortcut
//!   plugin (works everywhere);
//! - `"mod:MetaRight"` — a lone modifier key (macOS and Windows, via
//!   `native_input`; `Fn` is macOS only);
//! - `"mouse:3"` — an extra mouse button, NSEvent numbering (2 = middle,
//!   3 = back, 4 = forward; macOS and Windows);
//! - `""` — unbound.
//!
//! Locking rule: never hold [`ShortcutState`] while calling into the plugin.
//! The plugin keeps its own lock for the whole duration of our handler, so the
//! opposite order would deadlock. For the same reason nothing is ever
//! (un)registered from inside the handler itself — the Escape-while-recording
//! binding is synced from a spawned thread.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState as KeyState};

use crate::i18n::{t, tf};
use crate::pipeline;
use crate::settings::Settings;
use crate::state::{AppState, AppStatus};

/// What a binding does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Toggle,
    PushToTalk,
    Cancel,
    CycleMode,
    /// Mouse button: tap toggles, hold records until release.
    Mouse,
    /// Start or stop a meeting transcript.
    Meeting,
}

/// A lone modifier key, left or right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKey {
    MetaLeft,
    MetaRight,
    AltLeft,
    AltRight,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    Fn,
}

impl ModKey {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "MetaLeft" => Self::MetaLeft,
            "MetaRight" => Self::MetaRight,
            "AltLeft" => Self::AltLeft,
            "AltRight" => Self::AltRight,
            "ShiftLeft" => Self::ShiftLeft,
            "ShiftRight" => Self::ShiftRight,
            "ControlLeft" => Self::ControlLeft,
            "ControlRight" => Self::ControlRight,
            "Fn" => Self::Fn,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Accelerator(Shortcut),
    Modifier(ModKey),
    Mouse(u8),
}

impl Binding {
    /// Parse a stored binding; `Ok(None)` when unbound.
    pub fn parse(s: &str) -> Result<Option<Binding>, String> {
        let s = s.trim();
        if s.is_empty() {
            return Ok(None);
        }
        if let Some(m) = s.strip_prefix("mod:") {
            return ModKey::parse(m)
                .map(|k| Some(Binding::Modifier(k)))
                .ok_or_else(|| tf("shortcut.error.unknownModifier", &[("key", &m)]));
        }
        if let Some(b) = s.strip_prefix("mouse:") {
            return match b.parse::<u8>() {
                Ok(n) if n >= 2 => Ok(Some(Binding::Mouse(n))),
                _ => Err(tf("shortcut.error.invalidMouseButton", &[("button", &b)])),
            };
        }
        s.parse::<Shortcut>()
            .map(|sc| Some(Binding::Accelerator(sc)))
            .map_err(|_| tf("shortcut.error.invalid", &[("shortcut", &s)]))
    }

    /// Native (non-plugin) bindings need a `native_input` backend; Windows
    /// can't see the Fn key.
    fn supported(&self) -> bool {
        match self {
            Binding::Accelerator(_) => true,
            Binding::Modifier(ModKey::Fn) => cfg!(target_os = "macos"),
            _ => cfg!(any(target_os = "macos", windows)),
        }
    }
}

#[derive(Default)]
struct Inner {
    /// Plugin shortcut id → action (includes the cancel binding).
    accel: HashMap<u32, Action>,
    /// Accelerators to keep registered at all times (everything but cancel).
    always: Vec<Shortcut>,
    /// The cancel accelerator, registered only while recording.
    cancel: Option<Shortcut>,
    /// Modifier / mouse bindings, read by the `native_input` backends.
    native: Vec<(Binding, Action)>,
    /// Non-zero while the settings window is recording a shortcut.
    capture: u64,
    capture_seq: u64,
    /// Recording started by push-to-talk (released → stop).
    ptt_active: bool,
}

/// Managed state for the shortcut dispatcher.
#[derive(Default)]
pub struct ShortcutState {
    data: Mutex<Inner>,
}

fn lock(app: &AppHandle) -> MutexGuard<'_, Inner> {
    app.state::<ShortcutState>().inner().data.lock().unwrap()
}

/// Parse every binding in `settings`, rejecting invalid or duplicated ones.
fn collect(settings: &Settings) -> Result<Vec<(Binding, Action)>, String> {
    let all = [
        (&settings.shortcut, Action::Toggle, "shortcut.name.toggle"),
        (&settings.ptt_shortcut, Action::PushToTalk, "shortcut.name.pushToTalk"),
        (&settings.cancel_shortcut, Action::Cancel, "shortcut.name.cancel"),
        (&settings.mode_shortcut, Action::CycleMode, "shortcut.name.changeMode"),
        (&settings.mouse_shortcut, Action::Mouse, "shortcut.name.mouse"),
        (&settings.meeting_shortcut, Action::Meeting, "shortcut.name.meeting"),
    ];
    let mut out: Vec<(Binding, Action)> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for (raw, action, name_key) in all {
        let name = t(name_key);
        let Some(binding) = Binding::parse(raw)
            .map_err(|e| tf("shortcut.error.named", &[("name", &name), ("error", &e)]))?
        else {
            continue;
        };
        if !binding.supported() {
            continue;
        }
        let same = |b: &Binding| match (b, &binding) {
            (Binding::Accelerator(a), Binding::Accelerator(c)) => a.id() == c.id(),
            (a, c) => a == c,
        };
        if let Some(i) = out.iter().position(|(b, _)| same(b)) {
            return Err(tf("shortcut.error.duplicate", &[("name", &name), ("other", &names[i])]));
        }
        out.push((binding, action));
        names.push(name);
    }
    Ok(out)
}

/// Validate `settings`' bindings without touching the live registration.
#[cfg(test)]
pub fn validate(settings: &Settings) -> Result<(), String> {
    collect(settings).map(|_| ())
}

/// (Re)register every binding from `settings`. Not to be called from inside
/// the plugin handler.
pub fn apply(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let bindings = collect(settings)?;
    let capturing;
    {
        let mut inner = lock(app);
        inner.accel.clear();
        inner.always.clear();
        inner.cancel = None;
        inner.native.clear();
        for (binding, action) in &bindings {
            match binding {
                Binding::Accelerator(sc) => {
                    inner.accel.insert(sc.id(), *action);
                    if *action == Action::Cancel {
                        inner.cancel = Some(*sc);
                    } else {
                        inner.always.push(*sc);
                    }
                }
                other => inner.native.push((*other, *action)),
            }
        }
        capturing = inner.capture != 0;
    }
    sync_native();
    if capturing {
        return Ok(()); // `end_capture` registers them
    }
    register_all(app)
}

fn register_all(app: &AppHandle) -> Result<(), String> {
    let (always, cancel) = {
        let inner = lock(app);
        (inner.always.clone(), inner.cancel)
    };
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut errors = Vec::new();
    for sc in always {
        if let Err(e) = gs.register(sc) {
            errors.push(tf("shortcut.error.register", &[("shortcut", &sc), ("error", &e)]));
        }
    }
    if let Some(sc) = cancel {
        if recording(app) {
            let _ = gs.register(sc);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn recording(app: &AppHandle) -> bool {
    *app.state::<AppState>().status.lock().unwrap() == AppStatus::Recording
}

/// Register the cancel shortcut while recording and drop it otherwise, so
/// e.g. Escape isn't stolen from every other app. Runs on its own thread: it
/// may be triggered from inside the plugin handler (see module docs).
pub fn sync_cancel(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let (cancel, capturing) = {
            let inner = lock(&app);
            (inner.cancel, inner.capture != 0)
        };
        let Some(sc) = cancel else { return };
        let gs = app.global_shortcut();
        let want = recording(&app) && !capturing;
        if want && !gs.is_registered(sc) {
            let _ = gs.register(sc);
        } else if !want && gs.is_registered(sc) {
            let _ = gs.unregister(sc);
        }
    });
}

/// Plugin handler: map the shortcut to its action.
pub fn on_accelerator(app: &AppHandle, shortcut: &Shortcut, state: KeyState) {
    let action = {
        let inner = lock(app);
        if inner.capture != 0 {
            return;
        }
        inner.accel.get(&shortcut.id()).copied()
    };
    if let Some(action) = action {
        fire(app, action, state == KeyState::Pressed);
    }
}

/// Native bindings (modifier / mouse) for the `native_input` backends; empty
/// while capturing.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub fn native_bindings(app: &AppHandle) -> Vec<(Binding, Action)> {
    let inner = lock(app);
    if inner.capture != 0 {
        Vec::new()
    } else {
        inner.native.clone()
    }
}

/// Whether the settings window is recording a shortcut.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn capturing(app: &AppHandle) -> bool {
    lock(app).capture != 0
}

/// Which Windows input hooks are needed: `(keyboard, mouse)`. The keyboard
/// one also runs during a capture, to keep a lone Win tap from opening the
/// Start menu while it is being recorded.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn native_hooks(app: &AppHandle) -> (bool, bool) {
    let inner = lock(app);
    let keyboard = inner.capture != 0 || inner.native.iter().any(|(b, _)| matches!(b, Binding::Modifier(_)));
    let mouse = inner.native.iter().any(|(b, _)| matches!(b, Binding::Mouse(_)));
    (keyboard, mouse)
}

/// Tell the Windows backend to (un)install its hooks.
fn sync_native() {
    #[cfg(windows)]
    crate::windows_input::sync();
}

/// Run `action`; `pressed` is false for key/button releases.
pub fn fire(app: &AppHandle, action: Action, pressed: bool) {
    let status = *app.state::<AppState>().status.lock().unwrap();
    let idle = matches!(status, AppStatus::Idle | AppStatus::Error);
    match (action, pressed) {
        (Action::Toggle, true) => pipeline::toggle(app),
        (Action::PushToTalk, true) | (Action::Mouse, true) if idle => {
            set_ptt(app, true);
            pipeline::toggle(app);
        }
        (Action::PushToTalk, false) => {
            if set_ptt(app, false) && status == AppStatus::Recording {
                pipeline::toggle(app);
            }
        }
        (Action::Cancel, true) if status == AppStatus::Recording => {
            set_ptt(app, false);
            pipeline::cancel(app);
        }
        (Action::CycleMode, true) => crate::commands::cycle_mode(app),
        (Action::Meeting, true) => crate::meeting::toggle(app),
        _ => {}
    }
}

/// Set the push-to-talk flag; returns its previous value.
pub fn set_ptt(app: &AppHandle, on: bool) -> bool {
    let mut inner = lock(app);
    std::mem::replace(&mut inner.ptt_active, on)
}

/// While the settings window records a new shortcut, nothing may fire: drop
/// every registration (so the current combo reaches the webview) and mute the
/// native monitors. Restored by [`end_capture`], or after a timeout in case the
/// window goes away mid-capture.
pub fn begin_capture(app: &AppHandle) {
    let seq = {
        let mut inner = lock(app);
        inner.capture_seq += 1;
        inner.capture = inner.capture_seq;
        inner.capture
    };
    let _ = app.global_shortcut().unregister_all();
    sync_native();
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(15));
        let stale = lock(&app).capture == seq;
        if stale {
            end_capture(&app);
        }
    });
}

pub fn end_capture(app: &AppHandle) {
    {
        let mut inner = lock(app);
        if inner.capture == 0 {
            return;
        }
        inner.capture = 0;
    }
    sync_native();
    if let Err(e) = register_all(app) {
        eprintln!("[whisperaround] {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_binding_kind() {
        assert_eq!(Binding::parse("").unwrap(), None);
        assert!(matches!(Binding::parse("Super+Shift+KeyX").unwrap(), Some(Binding::Accelerator(_))));
        assert!(matches!(Binding::parse("Escape").unwrap(), Some(Binding::Accelerator(_))));
        assert_eq!(Binding::parse("mod:MetaRight").unwrap(), Some(Binding::Modifier(ModKey::MetaRight)));
        assert_eq!(Binding::parse("mouse:3").unwrap(), Some(Binding::Mouse(3)));
        assert!(Binding::parse("mouse:0").is_err());
        assert!(Binding::parse("mod:Hyper").is_err());
        assert!(Binding::parse("Shift+Nope").is_err());
    }

    #[test]
    fn duplicate_accelerators_are_rejected() {
        let s = Settings {
            shortcut: "Alt+Shift+KeyK".into(),
            ..Settings::default()
        };
        let err = validate(&s).unwrap_err();
        assert!(err.contains("Change mode"), "{err}");
    }

    #[test]
    fn defaults_are_valid() {
        validate(&Settings::default()).unwrap();
    }
}
