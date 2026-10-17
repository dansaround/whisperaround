//! macOS-only integration: the privacy permissions dictation needs and the
//! floating-window behaviour.
//!
//! - **Microphone**: macOS asks the first time the app opens an input stream;
//!   if the user said no, `cpal` keeps "recording" pure silence, so the status
//!   is checked explicitly to give a useful error.
//! - **Accessibility**: required to post the simulated Cmd+V. Without it the
//!   keystroke is silently dropped, so it is checked before pasting.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::msg_send;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use serde::Serialize;
use tauri::WebviewWindow;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const AnyObject) -> bool;
    static kAXTrustedCheckOptionPrompt: &'static NSString;
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeAudio: &'static NSString;
}

const PRIVACY_PANE: &str = "x-apple.systempreferences:com.apple.preference.security";

/// State of the two permissions, sent to the welcome window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub microphone: MicPermission,
    pub accessibility: bool,
}

/// `AVAuthorizationStatus`, lowercased for the frontend.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MicPermission {
    Undetermined,
    Denied,
    Granted,
}

pub fn permissions() -> Permissions {
    Permissions {
        microphone: microphone(),
        accessibility: accessibility_trusted(),
    }
}

/// Whether the app may post keyboard events (Cmd+V).
pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Ask for Accessibility: macOS shows its own dialog pointing to System
/// Settings and lists the app there (still switched off) so the user only has
/// to flip the toggle.
pub fn request_accessibility() -> bool {
    let key: &NSString = unsafe { kAXTrustedCheckOptionPrompt };
    let yes = NSNumber::new_bool(true);
    let options: Retained<NSDictionary<NSString, NSNumber>> =
        NSDictionary::from_slices(&[key], &[&*yes]);
    let ptr = Retained::as_ptr(&options) as *const AnyObject;
    unsafe { AXIsProcessTrustedWithOptions(ptr) }
}

pub fn microphone() -> MicPermission {
    let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
        return MicPermission::Undetermined;
    };
    let media: &NSString = unsafe { AVMediaTypeAudio };
    let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: media] };
    match status {
        0 => MicPermission::Undetermined,
        3 => MicPermission::Granted,
        // 1 = restricted (parental controls / MDM), 2 = denied.
        _ => MicPermission::Denied,
    }
}

/// Show the system microphone prompt (only does something while the status is
/// still undetermined; afterwards the user has to go to System Settings).
pub fn request_microphone() {
    let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
        return;
    };
    let media: &NSString = unsafe { AVMediaTypeAudio };
    let handler = block2::RcBlock::new(|_granted: Bool| {});
    let _: () = unsafe {
        msg_send![cls, requestAccessForMediaType: media, completionHandler: &*handler]
    };
}

/// Open System Settings › Privacy & Security at the given section
/// (`Privacy_Microphone`, `Privacy_Accessibility`).
pub fn open_privacy_settings(section: &str) {
    let _ = std::process::Command::new("open")
        .arg(format!("{PRIVACY_PANE}?{section}"))
        .spawn();
}

/// Let the floating pill follow the user across Spaces and over full-screen
/// apps, like a system overlay, without showing up in Cmd+Tab.
pub fn make_overlay(window: &WebviewWindow) {
    // NSWindowCollectionBehavior: CanJoinAllSpaces | Stationary |
    // IgnoresCycle | FullScreenAuxiliary.
    const BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
    let Ok(ns_window) = window.ns_window() else {
        return;
    };
    let ns_window = ns_window as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    // SAFETY: `ns_window` is the live NSWindow of this Tauri window and setup
    // runs on the main thread.
    unsafe {
        let _: () = msg_send![ns_window, setCollectionBehavior: BEHAVIOR];
    }
}

/// Name of the app the user is working in (for the history's "Apps used"),
/// ignoring WhisperAround itself (e.g. after a click on the pill).
pub fn frontmost_app_name() -> Option<String> {
    let cls = AnyClass::get(c"NSWorkspace")?;
    // SAFETY: plain NSWorkspace / NSRunningApplication getters; nil-checked.
    unsafe {
        let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
        if ws.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![ws, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let bundle: *mut NSString = msg_send![app, bundleIdentifier];
        if !bundle.is_null() && (*bundle).to_string() == "com.whisperaround.app" {
            return None;
        }
        let name: *mut NSString = msg_send![app, localizedName];
        (!name.is_null()).then(|| (*name).to_string())
    }
}

/// Let a webview render above 60 fps on ProMotion displays. WebKit caps page
/// rendering near 60 fps by default (`PreferPageRenderingUpdatesNear60FPSEnabled`);
/// that flag is only reachable through WKPreferences' private feature API, so
/// every selector is checked first and nothing happens if it's missing.
pub fn unlock_high_refresh(window: &WebviewWindow) {
    use objc2::sel;
    let _ = window.with_webview(|wv| {
        // SAFETY: `inner()` is the live WKWebView; every private selector is
        // probed with respondsToSelector: before being sent.
        unsafe {
            let webview = wv.inner() as *mut AnyObject;
            if webview.is_null() {
                return;
            }
            let Some(cls) = AnyClass::get(c"WKPreferences") else {
                return;
            };
            let has_features: Bool = msg_send![cls, respondsToSelector: sel!(_features)];
            if !has_features.as_bool() {
                return;
            }
            let config: *mut AnyObject = msg_send![webview, configuration];
            let prefs: *mut AnyObject = msg_send![config, preferences];
            let can_set: Bool = msg_send![prefs, respondsToSelector: sel!(_setEnabled:forFeature:)];
            if prefs.is_null() || !can_set.as_bool() {
                return;
            }
            let features: *mut AnyObject = msg_send![cls, _features];
            if features.is_null() {
                return;
            }
            let count: usize = msg_send![features, count];
            for i in 0..count {
                let feature: *mut AnyObject = msg_send![features, objectAtIndex: i];
                let key: *mut NSString = msg_send![feature, key];
                if !key.is_null() && (*key).to_string() == "PreferPageRenderingUpdatesNear60FPSEnabled" {
                    let _: () = msg_send![prefs, _setEnabled: Bool::NO, forFeature: feature];
                }
            }
        }
    });
}
