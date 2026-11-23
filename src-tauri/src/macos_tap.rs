//! System audio on macOS through a Core Audio process tap (macOS 14.2+).
//!
//! A tap is a virtual stream that carries what other processes play. Wrapped
//! in a private aggregate device it shows up, to this process only, as one
//! more input device, so the recorder captures it with cpal like any
//! microphone. This is the macOS counterpart of the WASAPI loopback used on
//! Windows.
//!
//! The tap covers every process except this one, so the app's own start and
//! stop sounds stay out of it. It does not depend on the output device:
//! switching to headphones mid-call keeps the same tap.
//!
//! The first capture makes macOS ask for "System Audio Recording"
//! (`NSAudioCaptureUsageDescription`). Until it is granted the tap delivers
//! silence rather than an error.
//!
//! The tap functions are looked up at runtime: they only exist from macOS
//! 14.2, and linking them directly would keep the app from launching on older
//! systems, where system audio is simply reported as unsupported.

use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

use crate::i18n::{t, tf};

type AudioObjectID = u32;
type OSStatus = i32;

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const SYSTEM_OBJECT: AudioObjectID = 1;
/// `kAudioHardwarePropertyTranslatePIDToProcessObject`.
const TRANSLATE_PID_TO_PROCESS: u32 = u32::from_be_bytes(*b"id2p");
/// `kAudioObjectPropertyScopeGlobal`.
const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
/// `kAudioObjectPropertyElementMain`.
const ELEMENT_MAIN: u32 = 0;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioHardwareCreateAggregateDevice(description: *const AnyObject, device: *mut AudioObjectID) -> OSStatus;
    fn AudioHardwareDestroyAggregateDevice(device: AudioObjectID) -> OSStatus;
    fn AudioObjectGetPropertyData(
        object: AudioObjectID,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
}

// libSystem, always linked.
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

type CreateTapFn = unsafe extern "C" fn(description: *const AnyObject, tap: *mut AudioObjectID) -> OSStatus;
type DestroyTapFn = unsafe extern "C" fn(tap: AudioObjectID) -> OSStatus;

struct TapApi {
    create: CreateTapFn,
    destroy: DestroyTapFn,
}

/// The process tap functions, or `None` before macOS 14.2.
fn api() -> Option<&'static TapApi> {
    static API: OnceLock<Option<TapApi>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let create = dlsym(RTLD_DEFAULT, c"AudioHardwareCreateProcessTap".as_ptr());
        let destroy = dlsym(RTLD_DEFAULT, c"AudioHardwareDestroyProcessTap".as_ptr());
        if create.is_null() || destroy.is_null() || AnyClass::get(c"CATapDescription").is_none() {
            return None;
        }
        Some(TapApi {
            create: std::mem::transmute::<*mut c_void, CreateTapFn>(create),
            destroy: std::mem::transmute::<*mut c_void, DestroyTapFn>(destroy),
        })
    })
    .as_ref()
}

/// Whether this macOS can capture the system audio (14.2 or later).
pub fn supported() -> bool {
    api().is_some()
}

/// This process's Core Audio object, which exists once it has used audio.
fn own_process_object() -> Option<AudioObjectID> {
    let pid = std::process::id() as i32;
    let address = PropertyAddress {
        selector: TRANSLATE_PID_TO_PROCESS,
        scope: SCOPE_GLOBAL,
        element: ELEMENT_MAIN,
    };
    let mut object: AudioObjectID = 0;
    let mut size = size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &address,
            size_of::<i32>() as u32,
            (&pid as *const i32).cast(),
            &mut size,
            (&mut object as *mut AudioObjectID).cast(),
        )
    };
    (status == 0 && object != 0).then_some(object)
}

fn os_error(key: &str, status: OSStatus) -> anyhow::Error {
    anyhow!(tf(key, &[("status", &status)]))
}

/// A process tap on everything the computer plays, exposed to this process as
/// an input device named [`SystemTap::device_name`]. Dropping it removes both.
pub struct SystemTap {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    device_name: String,
}

impl SystemTap {
    pub fn create() -> Result<SystemTap> {
        let api = api().ok_or_else(|| anyhow!(t("error.systemAudio.unsupported")))?;
        // Unique per tap: a dictation and a meeting can capture at once.
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let device_name = format!("WhisperAround System Audio {n}");
        let device_uid = format!("com.whisperaround.app.system-audio.{}.{n}", std::process::id());

        let exclude: Vec<Retained<NSNumber>> = own_process_object().map(NSNumber::new_u32).into_iter().collect();
        let exclude = NSArray::from_retained_slice(&exclude);
        let cls = AnyClass::get(c"CATapDescription").ok_or_else(|| anyhow!(t("error.systemAudio.unsupported")))?;
        let description: Option<Retained<AnyObject>> = unsafe {
            let allocated: Allocated<AnyObject> = msg_send![cls, alloc];
            msg_send![allocated, initStereoGlobalTapButExcludeProcesses: &*exclude]
        };
        let description = description.ok_or_else(|| os_error("error.systemAudio.tap", -1))?;
        unsafe {
            // Only this process sees the tap; it never shows up in Audio MIDI Setup.
            let _: () = msg_send![&*description, setPrivate: Bool::YES];
            let _: () = msg_send![&*description, setName: &*NSString::from_str(&device_name)];
        }

        let mut tap: AudioObjectID = 0;
        let status = unsafe { (api.create)(Retained::as_ptr(&description), &mut tap) };
        if status != 0 {
            return Err(os_error("error.systemAudio.tap", status));
        }
        // From here on, dropping `partial` destroys the tap if the device fails.
        let mut partial = SystemTap {
            tap,
            aggregate: 0,
            device_name,
        };

        let tap_uid: Retained<NSString> = unsafe {
            let uuid: *mut AnyObject = msg_send![&*description, UUID];
            let uid: Option<Retained<NSString>> = msg_send![uuid, UUIDString];
            uid.ok_or_else(|| os_error("error.systemAudio.tap", -1))?
        };
        let yes = NSNumber::new_bool(true);
        let no = NSNumber::new_bool(false);
        let sub_tap: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[&*NSString::from_str("uid"), &*NSString::from_str("drift")],
            &[&*tap_uid as &AnyObject, &*yes],
        );
        let taps = NSArray::from_slice(&[&*sub_tap]);
        let name = NSString::from_str(&partial.device_name);
        let uid = NSString::from_str(&device_uid);
        let keys = ["name", "uid", "private", "stacked", "tapautostart", "taps"].map(NSString::from_str);
        let values: [&AnyObject; 6] = [&*name, &*uid, &*yes, &*no, &*yes, &*taps];
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let description: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&key_refs, &values);

        let mut aggregate: AudioObjectID = 0;
        // NSDictionary is toll-free bridged to the CFDictionary it expects.
        let status = unsafe {
            AudioHardwareCreateAggregateDevice(Retained::as_ptr(&description).cast(), &mut aggregate)
        };
        if status != 0 {
            return Err(os_error("error.systemAudio.device", status));
        }
        partial.aggregate = aggregate;
        Ok(partial)
    }

    /// Name of the input device carrying the tap, as cpal lists it.
    pub fn device_name(&self) -> &str {
        &self.device_name
    }
}

impl Drop for SystemTap {
    fn drop(&mut self) {
        unsafe {
            if self.aggregate != 0 {
                AudioHardwareDestroyAggregateDevice(self.aggregate);
            }
            if let Some(api) = api() {
                (api.destroy)(self.tap);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Plays a system sound and checks that the tap hears it. Needs macOS
    /// 14.2+ and "System Audio Recording" granted to the app running the
    /// tests (the terminal): `cargo test macos_tap -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn the_tap_hears_what_the_computer_plays() {
        assert!(supported(), "needs macOS 14.2 or later");
        let tap = SystemTap::create().expect("create the tap");
        let device = cpal::default_host()
            .input_devices()
            .unwrap()
            .find(|d| d.name().ok().as_deref() == Some(tap.device_name()))
            .expect("the tap's aggregate device is listed as an input");
        let config = device.default_input_config().expect("input config");
        println!("tap device: {} {:?}", tap.device_name(), config);
        let peak = Arc::new(Mutex::new((0usize, 0.0f32)));
        let p = peak.clone();
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    let mut p = p.lock().unwrap();
                    p.0 += data.len();
                    p.1 = data.iter().fold(p.1, |m, s| m.max(s.abs()));
                },
                |e| panic!("stream error: {e}"),
                None,
            )
            .expect("build stream");
        stream.play().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let status = std::process::Command::new("afplay")
            .args(["-v", "0.5", "/System/Library/Sounds/Submarine.aiff"])
            .status()
            .unwrap();
        assert!(status.success());
        std::thread::sleep(Duration::from_millis(300));
        drop(stream);
        let (samples, peak) = *peak.lock().unwrap();
        println!("samples: {samples}, peak: {peak}");
        assert!(samples > 0, "the tap delivered no audio at all");
        assert!(peak > 0.01, "the tap delivered only silence (System Audio Recording not granted?)");
    }
}
