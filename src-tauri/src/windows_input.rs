//! Windows backend for `native_input`: a lone modifier key (e.g. Right Ctrl)
//! and extra mouse buttons, read from low-level hooks (`WH_KEYBOARD_LL` /
//! `WH_MOUSE_LL`). The hooks only observe — every event is passed on — and
//! need no permission. They are installed only while some binding (or a
//! shortcut capture in the settings window) needs them.
//!
//! The hooks live on their own thread with a message loop. Windows drops a
//! hook that is slow to return, and starting a recording opens the mic, so
//! shortcuts fire on a separate worker thread.
//!
//! Tapping Win alone opens the Start menu and tapping Alt alone focuses the
//! menu bar of the focused app. When such a tap triggers a shortcut (or is
//! being recorded), a no-op key is injected before the release goes through
//! so Windows doesn't treat it as a tap.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;

use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LCONTROL,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, PM_NOREMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::native_input::{self, Fire};
use crate::shortcuts::{self, ModKey};
use crate::state::AppState;

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Hands shortcuts to the worker thread.
static FIRE: OnceLock<Sender<Fire>> = OnceLock::new();
/// Id of the hook thread (0 until its message queue exists).
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
/// Posted to the hook thread to (un)install the hooks.
const WM_SYNC: u32 = WM_APP + 1;

/// The modifier pressed while nothing else was (`ModKey as u8 + 1`), cleared
/// as soon as another key goes down.
static ALONE: AtomicU8 = AtomicU8::new(0);

/// Unassigned virtual key, injected to stop a lone Win / Alt tap from
/// opening the Start menu / menu bar (the trick AutoHotkey uses).
const MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);
/// Scan code of the fake Left Ctrl that AltGr layouts (Spanish, German…)
/// send with Right Alt, and whether it is down.
const ALTGR_FAKE_CTRL: u32 = 0x21D;
static FAKE_CTRL: AtomicBool = AtomicBool::new(false);

/// Every modifier the hook tracks, by virtual key.
const MODIFIERS: [(VIRTUAL_KEY, ModKey); 8] = [
    (VK_LWIN, ModKey::MetaLeft),
    (VK_RWIN, ModKey::MetaRight),
    (VK_LMENU, ModKey::AltLeft),
    (VK_RMENU, ModKey::AltRight),
    (VK_LSHIFT, ModKey::ShiftLeft),
    (VK_RSHIFT, ModKey::ShiftRight),
    (VK_LCONTROL, ModKey::ControlLeft),
    (VK_RCONTROL, ModKey::ControlRight),
];

/// Start the hook and worker threads. The hooks themselves are installed by
/// [`sync`], once there is something to listen for.
pub fn install(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    let (tx, rx) = mpsc::channel::<Fire>();
    let _ = FIRE.set(tx);
    let worker = app.clone();
    std::thread::Builder::new()
        .name("native-shortcuts".into())
        .spawn(move || {
            for (action, pressed) in rx {
                shortcuts::fire(&worker, action, pressed);
            }
        })
        .expect("spawning the shortcut worker");
    std::thread::Builder::new()
        .name("input-hooks".into())
        .spawn(hook_thread)
        .expect("spawning the input hook thread");
}

/// Install or remove the hooks to match the current bindings. Cheap; called
/// whenever the bindings or the capture state change.
pub fn sync() {
    let thread = HOOK_THREAD.load(Ordering::Acquire);
    if thread != 0 {
        // SAFETY: plain message post to our own hook thread.
        let _ = unsafe { PostThreadMessageW(thread, WM_SYNC, WPARAM(0), LPARAM(0)) };
    }
}

fn hook_thread() {
    let mut keyboard: Option<HHOOK> = None;
    let mut mouse: Option<HHOOK> = None;
    let mut msg = MSG::default();
    // SAFETY: standard Win32 message loop on this thread. PeekMessageW
    // creates the queue before the thread id is published, so no post is lost.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        HOOK_THREAD.store(GetCurrentThreadId(), Ordering::Release);
        update_hooks(&mut keyboard, &mut mouse);
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_SYNC {
                update_hooks(&mut keyboard, &mut mouse);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Runs on the hook thread (hooks belong to the thread that set them).
unsafe fn update_hooks(keyboard: &mut Option<HHOOK>, mouse: &mut Option<HHOOK>) {
    let Some(app) = APP.get() else { return };
    let (want_keyboard, want_mouse) = shortcuts::native_hooks(app);
    let module = GetModuleHandleW(None).ok().map(Into::into);
    for (slot, want, id, proc) in [
        (keyboard, want_keyboard, WH_KEYBOARD_LL, keyboard_proc as HookProc),
        (mouse, want_mouse, WH_MOUSE_LL, mouse_proc as HookProc),
    ] {
        match (slot.is_some(), want) {
            (false, true) => match SetWindowsHookExW(id, Some(proc), module, 0) {
                Ok(hook) => *slot = Some(hook),
                Err(e) => eprintln!("[whisperaround] input hook: {e}"),
            },
            (true, false) => {
                if let Some(hook) = slot.take() {
                    let _ = UnhookWindowsHookEx(hook);
                }
                if id == WH_KEYBOARD_LL {
                    ALONE.store(0, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }
}

type HookProc = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        // Skip injected input: our own paste and mask keys, other tools' macros.
        if info.flags.0 & LLKHF_INJECTED.0 == 0 {
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            on_key(info.vkCode, info.scanCode, down);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT.
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        if info.flags & LLMHF_INJECTED == 0 {
            let msg = wparam.0 as u32;
            // NSEvent numbering: 2 = middle, 3 = back (XBUTTON1), 4 = forward.
            let button = match msg {
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some(2),
                WM_XBUTTONDOWN | WM_XBUTTONUP => match (info.mouseData >> 16) as u16 {
                    1 => Some(3),
                    2 => Some(4),
                    _ => None,
                },
                _ => None,
            };
            if let Some(button) = button {
                on_mouse(button, matches!(msg, WM_MBUTTONDOWN | WM_XBUTTONDOWN));
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn on_key(vk: u32, scan: u32, down: bool) {
    let Some(app) = APP.get() else { return };
    let vk = VIRTUAL_KEY(vk as u16);
    let Some(key) = mod_key(vk) else {
        if down {
            ALONE.store(0, Ordering::Relaxed);
            native_input::key_down();
        }
        return;
    };
    if key == ModKey::ControlLeft && scan == ALTGR_FAKE_CTRL {
        FAKE_CTRL.store(down, Ordering::Relaxed);
        return;
    }
    // The system key state is read instead of tracked here, so a release
    // Windows never shows us (e.g. after Win+L) can't leave a key stuck. It
    // isn't updated for the event being hooked yet: a key that is already
    // down is auto-repeating.
    if down && is_down(vk) {
        return;
    }
    let others = MODIFIERS.iter().any(|&(v, k)| class(k) != class(key) && is_down(v));
    let tag = key as u8 + 1;
    if down {
        let alone = !MODIFIERS.iter().any(|&(v, _)| v != vk && is_down(v));
        ALONE.store(if alone { tag } else { 0 }, Ordering::Relaxed);
    }

    let fire = native_input::modifier(&shortcuts::native_bindings(app), key, down, others);
    if !down {
        let alone = ALONE.swap(0, Ordering::Relaxed) == tag;
        if alone && opens_menu(key) && (fire.is_some() || shortcuts::capturing(app)) {
            send_mask_key();
        }
    }
    if let Some(fire) = fire {
        dispatch(fire);
    }
}

/// Whether `vk` is down, per the system (AltGr's fake Left Ctrl excluded).
fn is_down(vk: VIRTUAL_KEY) -> bool {
    if vk == VK_LCONTROL && FAKE_CTRL.load(Ordering::Relaxed) {
        return false;
    }
    // SAFETY: plain key state query.
    let state = unsafe { GetAsyncKeyState(vk.0 as i32) };
    state < 0
}

fn on_mouse(button: u8, down: bool) {
    let Some(app) = APP.get() else { return };
    let status = *app.state::<AppState>().status.lock().unwrap();
    if let Some(fire) = native_input::mouse(&shortcuts::native_bindings(app), button, down, status) {
        dispatch(fire);
    }
}

fn dispatch(fire: Fire) {
    if let Some(tx) = FIRE.get() {
        let _ = tx.send(fire);
    }
}

fn mod_key(vk: VIRTUAL_KEY) -> Option<ModKey> {
    MODIFIERS.iter().find(|(v, _)| *v == vk).map(|(_, k)| *k)
}

/// Both sides of a modifier compare equal.
fn class(key: ModKey) -> u8 {
    use ModKey::*;
    match key {
        MetaLeft | MetaRight => 0,
        AltLeft | AltRight => 1,
        ShiftLeft | ShiftRight => 2,
        ControlLeft | ControlRight => 3,
        Fn => 4,
    }
}

/// Keys whose lone tap Windows acts on (Start menu / menu bar).
fn opens_menu(key: ModKey) -> bool {
    matches!(
        key,
        ModKey::MetaLeft | ModKey::MetaRight | ModKey::AltLeft | ModKey::AltRight
    )
}

fn send_mask_key() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: MASK_KEY,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [key(Default::default()), key(KEYEVENTF_KEYUP)];
    // SAFETY: two well-formed keyboard INPUTs.
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}
