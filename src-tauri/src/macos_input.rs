//! macOS backend for `native_input`: a lone modifier key (e.g. Right ⌘) and
//! extra mouse buttons, read from `NSEvent` monitors — a global one for events
//! going to other apps and a local one for our own windows. Monitors are
//! passive: the events still reach their app. Key / modifier monitoring needs
//! the Accessibility permission (without it the callbacks silently never
//! fire); mouse buttons don't.

use std::ptr::NonNull;
use std::sync::OnceLock;

use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use tauri::{AppHandle, Manager};

use crate::native_input;
use crate::shortcuts::{self, ModKey};
use crate::state::AppState;

static APP: OnceLock<AppHandle> = OnceLock::new();

// NSEventType values.
const KEY_DOWN: usize = 10;
const FLAGS_CHANGED: usize = 12;
const OTHER_MOUSE_DOWN: usize = 25;
const OTHER_MOUSE_UP: usize = 26;

/// Install the monitors. Must run on the main thread (app setup).
pub fn install(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    let Some(cls) = AnyClass::get(c"NSEvent") else {
        return;
    };
    let mask: u64 = (1 << KEY_DOWN) | (1 << FLAGS_CHANGED) | (1 << OTHER_MOUSE_DOWN) | (1 << OTHER_MOUSE_UP);

    let global = RcBlock::new(|event: NonNull<AnyObject>| {
        // SAFETY: AppKit hands us a live NSEvent for the duration of the call.
        handle(unsafe { event.as_ref() });
    });
    let local = RcBlock::new(|event: NonNull<AnyObject>| -> *mut AnyObject {
        // SAFETY: as above; returning the event lets it through unchanged.
        handle(unsafe { event.as_ref() });
        event.as_ptr()
    });
    // SAFETY: documented NSEvent class methods; AppKit copies the blocks. The
    // returned monitor objects are kept alive for the life of the process.
    unsafe {
        let g: Option<Retained<AnyObject>> =
            msg_send![cls, addGlobalMonitorForEventsMatchingMask: mask, handler: &*global];
        let l: Option<Retained<AnyObject>> =
            msg_send![cls, addLocalMonitorForEventsMatchingMask: mask, handler: &*local];
        std::mem::forget(g);
        std::mem::forget(l);
    }
}

/// `(keyCode, device-dependent flag mask)` for each modifier key.
fn key_info(k: ModKey) -> (u16, usize) {
    match k {
        ModKey::MetaRight => (54, 0x10),
        ModKey::MetaLeft => (55, 0x08),
        ModKey::ShiftLeft => (56, 0x02),
        ModKey::AltLeft => (58, 0x20),
        ModKey::ControlLeft => (59, 0x01),
        ModKey::ShiftRight => (60, 0x04),
        ModKey::AltRight => (61, 0x40),
        ModKey::ControlRight => (62, 0x2000),
        ModKey::Fn => (63, 1 << 23),
    }
}

/// Modifier flags that mean "some modifier is down" (shift/ctrl/alt/cmd/fn).
const ANY_MODIFIER: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);

fn handle(event: &AnyObject) {
    let Some(app) = APP.get() else { return };
    // SAFETY: `type` is valid for every NSEvent.
    let ty: usize = unsafe { msg_send![event, type] };
    let fire = match ty {
        KEY_DOWN => {
            native_input::key_down();
            None
        }
        FLAGS_CHANGED => {
            // SAFETY: keyCode / modifierFlags are valid for flagsChanged events.
            let code: u16 = unsafe { msg_send![event, keyCode] };
            let flags: usize = unsafe { msg_send![event, modifierFlags] };
            match mod_key(code) {
                Some(key) => {
                    let pressed = flags & key_info(key).1 != 0;
                    let others = flags & ANY_MODIFIER & !modifier_class(key) != 0;
                    native_input::modifier(&shortcuts::native_bindings(app), key, pressed, others)
                }
                // Caps Lock and friends: just another key.
                None => {
                    native_input::key_down();
                    None
                }
            }
        }
        OTHER_MOUSE_DOWN | OTHER_MOUSE_UP => {
            // SAFETY: buttonNumber is valid for mouse events.
            let button: isize = unsafe { msg_send![event, buttonNumber] };
            let Ok(button) = u8::try_from(button) else { return };
            let status = *app.state::<AppState>().status.lock().unwrap();
            native_input::mouse(&shortcuts::native_bindings(app), button, ty == OTHER_MOUSE_DOWN, status)
        }
        _ => None,
    };
    if let Some((action, pressed)) = fire {
        shortcuts::fire(app, action, pressed);
    }
}

/// The modifier with this `keyCode`, if it is one.
fn mod_key(code: u16) -> Option<ModKey> {
    use ModKey::*;
    [MetaLeft, MetaRight, AltLeft, AltRight, ShiftLeft, ShiftRight, ControlLeft, ControlRight, Fn]
        .into_iter()
        .find(|k| key_info(*k).0 == code)
}

/// The device-independent flag for a modifier's class (both sides).
fn modifier_class(key: ModKey) -> usize {
    match key {
        ModKey::ShiftLeft | ModKey::ShiftRight => 1 << 17,
        ModKey::ControlLeft | ModKey::ControlRight => 1 << 18,
        ModKey::AltLeft | ModKey::AltRight => 1 << 19,
        ModKey::MetaLeft | ModKey::MetaRight => 1 << 20,
        ModKey::Fn => 1 << 23,
    }
}
