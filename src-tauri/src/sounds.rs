//! UI sound effects: start (user's choice of Orus / Loto / Okaso), stop and
//! error. Cancelling and finishing a transcript are silent.
//!
//! The recorded WAVs live in `sounds/` (16-bit PCM) and are embedded in the
//! binary. Playback goes through one long-lived output thread (a
//! `cpal::Stream` is `!Send`) that mixes voices into the default output
//! device, resampling on the fly. The stream is opened on first use and closed
//! after a while of silence so the audio device can idle.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Choices for the start cue.
pub const START_SOUNDS: &[&str] = &["orus", "loto", "okaso"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Start,
    Stop,
    Error,
}

impl Sound {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "start" => Self::Start,
            "stop" => Self::Stop,
            "error" => Self::Error,
            _ => return None,
        })
    }
}

/// Embedded clips: the start cues in `START_SOUNDS` order, then stop and
/// error.
static WAVS: [&[u8]; 5] = [
    include_bytes!("../sounds/start/orus.wav"),
    include_bytes!("../sounds/start/loto.wav"),
    include_bytes!("../sounds/start/okaso.wav"),
    include_bytes!("../sounds/events/stop.wav"),
    include_bytes!("../sounds/events/error.wav"),
];

/// Index into [`WAVS`]; `start_sound` picks the start cue (Orus if unknown).
fn slot(sound: Sound, start_sound: &str) -> usize {
    match sound {
        Sound::Start => START_SOUNDS.iter().position(|x| *x == start_sound).unwrap_or(0),
        Sound::Stop => 3,
        Sound::Error => 4,
    }
}

/// Decoded mono samples + their rate, cached per slot.
type Clip = Arc<(Vec<f32>, u32)>;
static CLIPS: OnceLock<Mutex<Vec<Option<Clip>>>> = OnceLock::new();

fn clip(slot: usize) -> Option<Clip> {
    let cache = CLIPS.get_or_init(|| Mutex::new(vec![None; WAVS.len()]));
    let mut cache = cache.lock().unwrap();
    if let Some(c) = &cache[slot] {
        return Some(c.clone());
    }
    let mut reader = hound::WavReader::new(std::io::Cursor::new(WAVS[slot])).ok()?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = reader
        .samples::<i16>()
        .filter_map(Result::ok)
        .map(|v| v as f32 / i16::MAX as f32)
        .collect();
    let mono = raw.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    let c: Clip = Arc::new((mono, spec.sample_rate));
    cache[slot] = Some(c.clone());
    Some(c)
}

struct Voice {
    clip: Clip,
    pos: f64,
    gain: f32,
}

struct Play {
    clip: Clip,
    gain: f32,
}

static PLAYER: OnceLock<Mutex<Sender<Play>>> = OnceLock::new();

/// How long the output stream stays open after the last sound.
const IDLE_CLOSE: Duration = Duration::from_secs(20);

fn player() -> &'static Mutex<Sender<Play>> {
    PLAYER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Play>();
        std::thread::Builder::new()
            .name("sounds".into())
            .spawn(move || {
                let voices: Arc<Mutex<Vec<Voice>>> = Arc::default();
                let rate = Arc::new(AtomicU32::new(48_000));
                let failed = Arc::new(AtomicBool::new(false));
                let mut stream: Option<cpal::Stream> = None;
                loop {
                    match rx.recv_timeout(IDLE_CLOSE) {
                        Ok(play) => {
                            if stream.is_none() || failed.load(Ordering::Relaxed) {
                                failed.store(false, Ordering::Relaxed);
                                voices.lock().unwrap().clear();
                                stream = match open(&voices, &rate, &failed) {
                                    Ok(s) => Some(s),
                                    Err(e) => {
                                        eprintln!("[whisperaround] sound: {e}");
                                        None
                                    }
                                };
                            }
                            if stream.is_some() {
                                voices.lock().unwrap().push(Voice {
                                    clip: play.clip,
                                    pos: 0.0,
                                    gain: play.gain,
                                });
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if voices.lock().unwrap().is_empty() {
                                stream = None;
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            })
            .expect("spawning the sound thread");
        Mutex::new(tx)
    })
}

fn open(
    voices: &Arc<Mutex<Vec<Voice>>>,
    rate: &Arc<AtomicU32>,
    failed: &Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let config = device.default_output_config()?;
    rate.store(config.sample_rate().0, Ordering::Relaxed);
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, voices, failed),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, voices, failed),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, voices, failed),
        other => return Err(anyhow::anyhow!("unsupported output format: {other:?}")),
    }?;
    stream.play()?;
    Ok(stream)
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    voices: &Arc<Mutex<Vec<Voice>>>,
    failed: &Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let channels = config.channels.max(1) as usize;
    let out_rate = config.sample_rate.0 as f64;
    let voices = voices.clone();
    let failed = failed.clone();
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &_| {
            let mut voices = voices.lock().unwrap();
            for frame in data.chunks_mut(channels) {
                let mut mix = 0.0f32;
                for v in voices.iter_mut() {
                    let (samples, src_rate) = &*v.clip;
                    let i = v.pos as usize;
                    if i + 1 < samples.len() {
                        let t = (v.pos - i as f64) as f32;
                        mix += (samples[i] * (1.0 - t) + samples[i + 1] * t) * v.gain;
                    }
                    v.pos += *src_rate as f64 / out_rate;
                }
                let s = T::from_sample(mix.clamp(-1.0, 1.0));
                frame.iter_mut().for_each(|x| *x = s);
            }
            voices.retain(|v| (v.pos as usize) + 1 < v.clip.0.len());
        },
        move |e| {
            eprintln!("[whisperaround] output stream: {e}");
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    Ok(stream)
}

/// Perceptual volume curve: the slider feels linear to the ear.
fn gain(volume: f32) -> f32 {
    let v = volume.clamp(0.0, 1.0);
    v * v
}

fn send(clip: Clip, volume: f32) {
    let _ = player().lock().unwrap().send(Play {
        clip,
        gain: gain(volume),
    });
}

/// Play `sound` at an explicit volume (settings preview); `start_sound`
/// picks the start cue.
pub fn play_with(start_sound: &str, sound: Sound, volume: f32) {
    if let Some(clip) = clip(slot(sound, start_sound)) {
        send(clip, volume);
    }
}

/// Play `sound` with the user's settings (no-op when sounds are off).
pub fn play(app: &AppHandle, sound: Sound) {
    let (enabled, start, volume) = {
        let s = app.state::<AppState>();
        let s = s.settings.lock().unwrap();
        (s.sounds_enabled, s.start_sound.clone(), s.sound_volume)
    };
    if enabled && volume > 0.0 {
        play_with(&start, sound, volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_sound_decodes() {
        for i in 0..WAVS.len() {
            let c = clip(i).unwrap_or_else(|| panic!("slot {i}"));
            assert!(c.0.len() > 10_000, "slot {i} too short");
            assert_eq!(c.1, 44_100);
        }
    }

    #[test]
    fn start_cue_follows_the_setting() {
        assert_eq!(slot(Sound::Start, "loto"), 1);
        assert_eq!(slot(Sound::Start, "okaso"), 2);
        assert_eq!(slot(Sound::Start, "gone"), 0);
        assert_eq!(slot(Sound::Error, "loto"), 4);
    }
}
