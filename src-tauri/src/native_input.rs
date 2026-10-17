//! Shortcut sources the global-shortcut plugin can't express — a lone
//! modifier key (e.g. Right ⌘ / Right Ctrl) and extra mouse buttons — shared
//! by the platform backends: `macos_input` (NSEvent monitors) and
//! `windows_input` (low-level hooks). The backends translate native events
//! into the calls below and fire whatever they return.
//!
//! - Modifier bound to Toggle (or Change mode / Cancel): fires on release, and
//!   only if no other key was pressed meanwhile, so Right ⌘+C still copies.
//! - Modifier bound to Push to Talk: press starts, release stops.
//! - Mouse button: a tap toggles; holding records until the button is released.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::shortcuts::{Action, Binding, ModKey};
use crate::state::AppStatus;

/// A shortcut to run: `(action, pressed)`, as taken by `shortcuts::fire`.
pub type Fire = (Action, bool);

/// Which modifier is currently held alone (`ModKey as u8 + 1`, 0 = none) and
/// whether another key was pressed while it was.
static HELD: AtomicU8 = AtomicU8::new(0);
static TAINTED: AtomicBool = AtomicBool::new(false);
/// Mouse hold tracking: when the bound button went down, and whether that
/// press started the recording.
static MOUSE_DOWN: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
/// Holding the mouse button longer than this means push-to-talk.
const HOLD: Duration = Duration::from_millis(350);

/// A non-modifier key went down: a held modifier is now part of a combo.
pub fn key_down() {
    TAINTED.store(true, Ordering::Relaxed);
}

/// Modifier `key` went down or up. `others` says whether a modifier of
/// another kind (shift / control / alt / command) is held too.
pub fn modifier(bindings: &[(Binding, Action)], key: ModKey, pressed: bool, others: bool) -> Option<Fire> {
    // Another modifier changing while ours is held turns the tap into a combo.
    if held_key().is_some_and(|held| held != key) {
        TAINTED.store(true, Ordering::Relaxed);
    }
    let action = bindings.iter().find_map(|(b, a)| match b {
        Binding::Modifier(k) if *k == key => Some(*a),
        _ => None,
    })?;
    let tag = key as u8 + 1;
    if pressed {
        // Only arm when it is the sole modifier held.
        if others {
            return None;
        }
        HELD.store(tag, Ordering::Relaxed);
        TAINTED.store(false, Ordering::Relaxed);
        (action == Action::PushToTalk).then_some((action, true))
    } else if HELD.load(Ordering::Relaxed) == tag {
        HELD.store(0, Ordering::Relaxed);
        if action == Action::PushToTalk {
            Some((action, false))
        } else {
            (!TAINTED.load(Ordering::Relaxed)).then_some((action, true))
        }
    } else {
        None
    }
}

fn held_key() -> Option<ModKey> {
    use ModKey::*;
    let tag = HELD.load(Ordering::Relaxed);
    [
        MetaLeft,
        MetaRight,
        AltLeft,
        AltRight,
        ShiftLeft,
        ShiftRight,
        ControlLeft,
        ControlRight,
        Fn,
    ]
    .into_iter()
    .find(|k| *k as u8 + 1 == tag)
}

/// Mouse `button` (NSEvent numbering: 2 = middle, 3 = back, 4 = forward)
/// went down or up while the pipeline was in `status`.
pub fn mouse(bindings: &[(Binding, Action)], button: u8, down: bool, status: AppStatus) -> Option<Fire> {
    let action = bindings.iter().find_map(|(b, a)| match b {
        Binding::Mouse(n) if *n == button => Some(*a),
        _ => None,
    })?;
    if action != Action::Mouse {
        return down.then_some((action, true));
    }
    let mut held = MOUSE_DOWN.lock().unwrap();
    if down {
        match status {
            AppStatus::Idle | AppStatus::Error => {
                *held = Some((Instant::now(), true));
                Some((Action::Toggle, true))
            }
            AppStatus::Recording => {
                *held = Some((Instant::now(), false));
                Some((Action::Toggle, true))
            }
            AppStatus::Processing => None,
        }
    } else {
        let (at, started) = held.take()?;
        (started && at.elapsed() >= HOLD && status == AppStatus::Recording).then_some((Action::Toggle, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The state is global, so every scenario runs in one test.
    #[test]
    fn modifier_taps_holds_and_combos() {
        let toggle = [(Binding::Modifier(ModKey::MetaRight), Action::Toggle)];
        let ptt = [(Binding::Modifier(ModKey::ControlRight), Action::PushToTalk)];

        // A clean tap fires on release.
        assert_eq!(modifier(&toggle, ModKey::MetaRight, true, false), None);
        assert_eq!(
            modifier(&toggle, ModKey::MetaRight, false, false),
            Some((Action::Toggle, true))
        );

        // Right ⌘+C is a combo, not a tap.
        modifier(&toggle, ModKey::MetaRight, true, false);
        key_down();
        assert_eq!(modifier(&toggle, ModKey::MetaRight, false, false), None);

        // Pressed while another modifier is held: never armed.
        assert_eq!(modifier(&toggle, ModKey::MetaRight, true, true), None);
        assert_eq!(modifier(&toggle, ModKey::MetaRight, false, false), None);

        // Another modifier joining in taints the tap.
        modifier(&toggle, ModKey::MetaRight, true, false);
        modifier(&toggle, ModKey::ShiftLeft, true, true);
        assert_eq!(modifier(&toggle, ModKey::MetaRight, false, true), None);

        // Push to talk: press starts, release stops, even after other keys.
        assert_eq!(
            modifier(&ptt, ModKey::ControlRight, true, false),
            Some((Action::PushToTalk, true))
        );
        key_down();
        assert_eq!(
            modifier(&ptt, ModKey::ControlRight, false, false),
            Some((Action::PushToTalk, false))
        );

        // Unbound keys do nothing.
        assert_eq!(modifier(&toggle, ModKey::AltLeft, true, false), None);
        assert_eq!(modifier(&toggle, ModKey::AltLeft, false, false), None);
    }

    #[test]
    fn mouse_tap_and_hold() {
        let b = [(Binding::Mouse(3), Action::Mouse)];
        assert_eq!(mouse(&b, 4, true, AppStatus::Idle), None);
        // Tap: down starts, a quick release keeps recording.
        assert_eq!(mouse(&b, 3, true, AppStatus::Idle), Some((Action::Toggle, true)));
        assert_eq!(mouse(&b, 3, false, AppStatus::Recording), None);
        // Next tap stops.
        assert_eq!(mouse(&b, 3, true, AppStatus::Recording), Some((Action::Toggle, true)));
        assert_eq!(mouse(&b, 3, false, AppStatus::Processing), None);
        // Busy transcribing: ignored.
        assert_eq!(mouse(&b, 3, true, AppStatus::Processing), None);
        // Hold: releasing after the threshold stops.
        mouse(&b, 3, true, AppStatus::Idle);
        std::thread::sleep(HOLD + Duration::from_millis(20));
        assert_eq!(mouse(&b, 3, false, AppStatus::Recording), Some((Action::Toggle, true)));
    }
}
