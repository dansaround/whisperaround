//! Audio capture via `cpal` (microphone and, on Windows, system audio), encoded
//! to 16-bit PCM WAV with `hound`.
//!
//! Everything ends up as 16 kHz mono `f32`, which is what Whisper wants anyway.
//! Each source is a *track*: a microphone, or the loopback of the default
//! output device (WASAPI). `cpal::Stream` is `!Send`, so one dedicated thread
//! owns every stream. The [`Recorder`] handle communicates with that thread
//! over channels: it waits for the streams to start (surfacing device errors
//! synchronously) and signals it to stop. While capturing, the thread also
//! reports the input level so the UI can draw a live waveform.
//!
//! Loopback delivers no callbacks while nothing is playing, so each track pads
//! itself with silence by wall clock to stay aligned with real time (and with
//! the other track of a mix).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;

use crate::i18n::{t, tf};

/// Sample rate of every [`Track`] and of the encoded WAV.
pub const SAMPLE_RATE: u32 = 16_000;

/// Where the audio comes from, parsed from the `input_device` setting.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A microphone by name (the system default when empty).
    Mic(String),
    /// The default output device's loopback (Windows only).
    System,
    /// A microphone (default when empty) plus the system audio.
    Mix(String),
}

impl Source {
    /// `""` or a device name → microphone, `"system:"` → system audio,
    /// `"mix:"` / `"mix:<mic name>"` → microphone plus system audio.
    pub fn parse(setting: &str) -> Source {
        if setting == "system:" {
            Source::System
        } else if let Some(mic) = setting.strip_prefix("mix:") {
            Source::Mix(mic.to_string())
        } else {
            Source::Mic(setting.to_string())
        }
    }
}

/// What a [`Track`] was captured from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Mic,
    #[cfg_attr(not(windows), allow(dead_code))]
    System,
}

/// Mono audio at [`SAMPLE_RATE`] from one source.
pub struct Track {
    pub kind: TrackKind,
    pub samples: Vec<f32>,
}

/// The captured audio, one [`Track`] per source.
pub struct Recording {
    pub tracks: Vec<Track>,
}

impl Recording {
    fn longest(&self) -> usize {
        self.tracks.iter().map(|t| t.samples.len()).max().unwrap_or(0)
    }

    /// All tracks summed (padded to the longest) and clamped to `[-1, 1]`.
    pub fn mixed(&self) -> Vec<f32> {
        let mut out = vec![0.0f32; self.longest()];
        for track in &self.tracks {
            for (o, s) in out.iter_mut().zip(&track.samples) {
                *o += s;
            }
        }
        out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
        out
    }

    /// Length of the longest track.
    pub fn duration_ms(&self) -> u64 {
        self.longest() as u64 * 1000 / SAMPLE_RATE as u64
    }

    /// True only if there is a microphone track and it is exactly zero
    /// everywhere (a muted stream, not a quiet room).
    pub fn mic_is_digital_silence(&self) -> bool {
        let mut mics = self.tracks.iter().filter(|t| t.kind == TrackKind::Mic).peekable();
        mics.peek().is_some() && mics.all(|t| t.samples.iter().all(|&s| s == 0.0))
    }

    /// True if every track is exactly zero everywhere (or there is no audio).
    pub fn is_digital_silence(&self) -> bool {
        self.tracks.iter().all(|t| t.samples.iter().all(|&s| s == 0.0))
    }
}

/// Shortfall allowed before padding with silence, so normal callback jitter
/// doesn't insert zeros.
const GAP_TOLERANCE: Duration = Duration::from_millis(100);

/// How many frames to pad so a track that has `have` frames matches the
/// wall-clock `elapsed` at `rate`; zero unless it is behind by more than
/// [`GAP_TOLERANCE`].
fn missing_frames(elapsed: Duration, rate: u32, have: u64) -> u64 {
    let expected = (elapsed.as_secs_f64() * rate as f64) as u64;
    let tolerance = (GAP_TOLERANCE.as_secs_f64() * rate as f64) as u64;
    if expected > have + tolerance {
        expected - have
    } else {
        0
    }
}

/// Linear-interpolation resampling of `input` from `rate` to [`SAMPLE_RATE`].
fn resample(input: &[f32], rate: u32) -> Vec<f32> {
    if rate == SAMPLE_RATE || input.is_empty() {
        return input.to_vec();
    }
    let n_out = (input.len() as f64 * SAMPLE_RATE as f64 / rate as f64).round() as usize;
    let step = rate as f64 / SAMPLE_RATE as f64;
    let last = input.len() - 1;
    (0..n_out)
        .map(|i| {
            let pos = i as f64 * step;
            let idx = (pos as usize).min(last);
            let frac = (pos - idx as f64) as f32;
            let next = input[(idx + 1).min(last)];
            input[idx] + (next - input[idx]) * frac
        })
        .collect()
}

/// Average interleaved frames of `channels` samples down to mono.
fn downmix<T: Copy>(data: &[T], channels: usize, convert: fn(T) -> f32) -> Vec<f32> {
    data.chunks(channels.max(1))
        .map(|frame| frame.iter().map(|&s| convert(s)).sum::<f32>() / frame.len() as f32)
        .collect()
}

/// One track's capture buffer, shared between its stream callback and the
/// [`Recorder`] handle.
struct TrackBuf {
    kind: TrackKind,
    /// Native sample rate of the device.
    rate: u32,
    /// When capture began; the wall clock that gaps are measured against.
    start: Instant,
    /// Frames appended since the start, drained ones included.
    frames: u64,
    /// Mono samples at `rate` not drained yet.
    samples: Vec<f32>,
}

type SharedTrack = Arc<Mutex<TrackBuf>>;

impl TrackBuf {
    fn new(kind: TrackKind, rate: u32) -> Self {
        Self {
            kind,
            rate,
            start: Instant::now(),
            frames: 0,
            samples: Vec::new(),
        }
    }

    /// Pad zeros so that the frames so far plus `pending` match the wall clock.
    fn pad(&mut self, pending: usize) {
        let missing = missing_frames(self.start.elapsed(), self.rate, self.frames + pending as u64);
        if missing > 0 {
            self.samples.resize(self.samples.len() + missing as usize, 0.0);
            self.frames += missing;
        }
    }

    fn append(&mut self, chunk: &[f32]) {
        self.pad(chunk.len());
        self.samples.extend_from_slice(chunk);
        self.frames += chunk.len() as u64;
    }

    /// Take everything not drained yet, resampled to [`SAMPLE_RATE`].
    fn take(&mut self) -> Track {
        self.pad(0);
        let raw = std::mem::take(&mut self.samples);
        Track {
            kind: self.kind,
            samples: resample(&raw, self.rate),
        }
    }
}

/// Handle to an in-progress recording running on its own thread.
pub struct Recorder {
    source: Source,
    tracks: Vec<SharedTrack>,
    stop_tx: Sender<()>,
    handle: JoinHandle<Result<()>>,
}

/// How often the input level is reported while recording (~60 Hz, so the
/// waveform moves every frame).
const LEVEL_INTERVAL: Duration = Duration::from_millis(16);

/// Append a mono chunk to the track and raise the pending level to the
/// chunk's RMS (the reporter resets it after each read, so it holds the
/// loudest chunk since the last report).
fn push_chunk(track: &Mutex<TrackBuf>, level: &AtomicU32, chunk: &[f32]) {
    if chunk.is_empty() {
        return;
    }
    track.lock().unwrap().append(chunk);
    let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
    if rms > f32::from_bits(level.load(Ordering::Relaxed)) {
        level.store(rms.to_bits(), Ordering::Relaxed);
    }
}

/// Map an RMS amplitude to a perceptual `0.0..=1.0` level (-52 dBFS → 0,
/// -22 dBFS → 1), so normal speech — which a laptop mic picks up around
/// -40..-25 dBFS — swings the bars across most of their range instead of
/// barely moving them.
fn normalize_level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 52.0) / 30.0).clamp(0.0, 1.0)
}

/// Build an input stream of sample type `T` that feeds `track`.
fn build_stream<T: cpal::SizedSample + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    convert: fn(T) -> f32,
    track: SharedTrack,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _: &_| push_chunk(&track, &level, &downmix(data, channels, convert)),
        |e| eprintln!("[whisperaround] cpal stream error: {e}"),
        None,
    )
}

/// Start capturing `device` with `config` into a new track of `kind`.
fn open_stream(
    kind: TrackKind,
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    level: &Arc<AtomicU32>,
) -> Result<(cpal::Stream, SharedTrack)> {
    let format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    let track = Arc::new(Mutex::new(TrackBuf::new(kind, stream_config.sample_rate.0)));
    let (tr, lv) = (track.clone(), level.clone());

    let stream = match format {
        cpal::SampleFormat::F32 => build_stream::<f32>(device, &stream_config, |s| s, tr, lv),
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(device, &stream_config, |s| s as f32 / i16::MAX as f32, tr, lv)
        }
        cpal::SampleFormat::U16 => build_stream::<u16>(
            device,
            &stream_config,
            |s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0,
            tr,
            lv,
        ),
        other => return Err(anyhow!(tf("error.mic.sampleFormat", &[("format", &format!("{other:?}"))]))),
    }
    .map_err(|e| anyhow!(tf("error.mic.buildStream", &[("error", &e)])))?;

    stream.play().map_err(|e| anyhow!(tf("error.mic.startStream", &[("error", &e)])))?;
    // Measure gaps from when audio can actually flow, not from device setup.
    track.lock().unwrap().start = Instant::now();
    Ok((stream, track))
}

/// Open the microphone named `name` (the system default when empty or no
/// longer connected).
fn open_mic(name: &str, level: &Arc<AtomicU32>) -> Result<(cpal::Stream, SharedTrack)> {
    let host = cpal::default_host();
    let named = (!name.is_empty())
        .then(|| host.input_devices().ok())
        .flatten()
        .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(name)));
    let device = named
        .or_else(|| host.default_input_device())
        .ok_or_else(|| anyhow!(t("error.mic.noDevice")))?;
    let config = device
        .default_input_config()
        .map_err(|e| anyhow!(tf("error.mic.config", &[("error", &e)])))?;
    open_stream(TrackKind::Mic, &device, config, level)
}

/// Open the loopback of the default output device: cpal turns an input
/// stream on a render device into a WASAPI loopback capture.
#[cfg(windows)]
fn open_system(level: &Arc<AtomicU32>) -> Result<(cpal::Stream, SharedTrack)> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow!(t("error.systemAudio.noDevice")))?;
    let config = device
        .default_output_config()
        .map_err(|e| anyhow!(tf("error.systemAudio.config", &[("error", &e)])))?;
    open_stream(TrackKind::System, &device, config, level)
}

#[cfg(not(windows))]
fn open_system(_: &Arc<AtomicU32>) -> Result<(cpal::Stream, SharedTrack)> {
    Err(anyhow!(t("error.systemAudio.unsupported")))
}

/// Open every stream `source` needs, one track each (microphone first).
fn open_streams(source: &Source, level: &Arc<AtomicU32>) -> Result<(Vec<cpal::Stream>, Vec<SharedTrack>)> {
    // Fail before opening the microphone of a mix we can't complete.
    if !cfg!(windows) && !matches!(source, Source::Mic(_)) {
        return Err(anyhow!(t("error.systemAudio.unsupported")));
    }
    let system = || open_system(level).map_err(|e| anyhow!(tf("error.systemAudio.open", &[("error", &e)])));
    let opened = match source {
        Source::Mic(name) => vec![open_mic(name, level)?],
        Source::System => vec![system()?],
        Source::Mix(name) => vec![open_mic(name, level)?, system()?],
    };
    Ok(opened.into_iter().unzip())
}

impl Recorder {
    /// Open the source described by the `input_device` setting (see
    /// [`Source::parse`]) and start capturing. Returns once the streams are
    /// actually running (or with an error if any could not start).
    /// `on_level` is called every [`LEVEL_INTERVAL`] with the normalized input
    /// level from the capture thread.
    pub fn start(source: &str, on_level: impl Fn(f32) + Send + 'static) -> Result<Recorder> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let source = Source::parse(source);
        let thread_source = source.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<SharedTrack>>>();

        let handle = thread::spawn(move || -> Result<()> {
            // Open the streams; report any setup error back to `start()`.
            let level = Arc::new(AtomicU32::new(0));
            let streams = match open_streams(&thread_source, &level) {
                Ok((streams, tracks)) => {
                    let _ = ready_tx.send(Ok(tracks));
                    streams
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!("{e}")));
                    return Err(e);
                }
            };

            // Capture until stop is signalled, reporting the level meanwhile.
            loop {
                match stop_rx.recv_timeout(LEVEL_INTERVAL) {
                    Err(RecvTimeoutError::Timeout) => {
                        let rms = f32::from_bits(level.swap(0, Ordering::Relaxed));
                        on_level(normalize_level(rms));
                    }
                    _ => break,
                }
            }
            drop(streams); // stops the streams and flushes callbacks
            Ok(())
        });

        match ready_rx.recv() {
            Ok(Ok(tracks)) => Ok(Recorder {
                source,
                tracks,
                stop_tx,
                handle,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!(t("error.recording.threadEnded"))),
        }
    }

    /// The source this recorder was started with.
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// New audio since the previous drain (or the start), per track,
    /// resampled to [`SAMPLE_RATE`]. Callable while recording.
    pub fn drain(&self) -> Vec<Track> {
        self.tracks.iter().map(|t| t.lock().unwrap().take()).collect()
    }

    /// Stop capturing and return everything not drained yet.
    pub fn stop(self) -> Result<Recording> {
        let _ = self.stop_tx.send(());
        let Recorder { tracks, handle, .. } = self;
        handle
            .join()
            .map_err(|_| anyhow!(t("error.recording.threadPanicked")))??;
        let tracks = tracks.iter().map(|t| t.lock().unwrap().take()).collect();
        Ok(Recording { tracks })
    }
}

/// An input device for the settings' microphone picker.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    pub name: String,
    pub is_default: bool,
}

/// Every input device the OS reports, default first.
pub fn input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok());
    let mut out: Vec<InputDevice> = host
        .input_devices()
        .map(|it| {
            it.filter_map(|d| d.name().ok())
                .map(|name| InputDevice {
                    is_default: default.as_deref() == Some(&name),
                    name,
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|d| !d.is_default);
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Encode mono [`SAMPLE_RATE`] samples to an in-memory 16-bit PCM WAV.
pub fn encode_wav(samples: &[f32]) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &s in samples {
            writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(kind: TrackKind, samples: &[f32]) -> Track {
        Track {
            kind,
            samples: samples.to_vec(),
        }
    }

    #[test]
    fn source_parse_recognises_the_special_values() {
        assert_eq!(Source::parse(""), Source::Mic(String::new()));
        assert_eq!(Source::parse("USB Mic"), Source::Mic("USB Mic".into()));
        assert_eq!(Source::parse("system:"), Source::System);
        assert_eq!(Source::parse("mix:"), Source::Mix(String::new()));
        assert_eq!(Source::parse("mix:USB Mic"), Source::Mix("USB Mic".into()));
    }

    #[test]
    fn level_maps_decibels_to_unit_range() {
        assert_eq!(normalize_level(0.0), 0.0);
        assert_eq!(normalize_level(0.001), 0.0); // -60 dBFS: silence
        assert!((normalize_level(0.01) - 0.4).abs() < 1e-4); // -40 dBFS
        assert_eq!(normalize_level(1.0), 1.0); // clipping stays at 1
    }

    #[test]
    fn push_chunk_appends_and_keeps_loudest_rms() {
        let buf = Mutex::new(TrackBuf::new(TrackKind::Mic, SAMPLE_RATE));
        let level = AtomicU32::new(0);
        push_chunk(&buf, &level, &[0.5, -0.5]);
        push_chunk(&buf, &level, &[0.1, -0.1]);
        assert_eq!(buf.lock().unwrap().samples.len(), 4);
        assert_eq!(f32::from_bits(level.load(Ordering::Relaxed)), 0.5);
    }

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix(&[1.0f32, 0.0, -0.5, -0.5], 2, |s| s), vec![0.5, -0.5]);
        assert_eq!(downmix(&[1i16, 3], 1, |s| s as f32), vec![1.0, 3.0]);
    }

    #[test]
    fn missing_frames_ignores_jitter_but_fills_gaps() {
        let rate = 48_000;
        // Within the tolerance: no padding.
        assert_eq!(missing_frames(Duration::from_millis(1000), rate, 48_000 - 4_000), 0);
        assert_eq!(missing_frames(Duration::from_millis(1000), rate, 48_000), 0);
        // Ahead of the clock: no padding.
        assert_eq!(missing_frames(Duration::from_millis(500), rate, 48_000), 0);
        // Two seconds of silence from a loopback with nothing playing.
        assert_eq!(missing_frames(Duration::from_secs(2), rate, 0), 96_000);
        // Behind by more than the tolerance: pad up to the clock.
        assert_eq!(missing_frames(Duration::from_secs(1), rate, 24_000), 24_000);
    }

    #[test]
    fn append_pads_a_late_first_chunk() {
        let mut buf = TrackBuf::new(TrackKind::System, 1_000);
        buf.start = Instant::now() - Duration::from_secs(1);
        buf.append(&[0.5; 100]);
        // ~900 frames of silence, then the chunk itself.
        assert!((buf.samples.len() as i64 - 1_000).abs() < 20);
        assert_eq!(&buf.samples[buf.samples.len() - 100..], &[0.5; 100]);
        assert_eq!(buf.samples[0], 0.0);
    }

    #[test]
    fn take_drains_and_resamples() {
        let mut buf = TrackBuf::new(TrackKind::Mic, 48_000);
        buf.append(&[0.25; 4_800]);
        let first = buf.take();
        assert_eq!(first.kind, TrackKind::Mic);
        assert_eq!(first.samples.len(), 1_600);
        assert!(buf.samples.is_empty());
        assert!(buf.take().samples.is_empty());
    }

    #[test]
    fn resample_changes_length_and_interpolates() {
        let same = [0.1, 0.2, 0.3];
        assert_eq!(resample(&same, SAMPLE_RATE), same);
        assert_eq!(resample(&[0.0; 4_800], 48_000).len(), 1_600);
        assert_eq!(resample(&[0.0; 8_000], 8_000).len(), 16_000);
        // Upsampling 8 kHz → 16 kHz puts the midpoint between samples.
        let up = resample(&[0.0, 1.0], 8_000);
        assert_eq!(up.len(), 4);
        assert!((up[1] - 0.5).abs() < 1e-6);
        assert!(resample(&[], 44_100).is_empty());
    }

    #[test]
    fn mixed_pads_to_the_longest_and_clamps() {
        let rec = Recording {
            tracks: vec![
                track(TrackKind::Mic, &[0.8, -0.8, 0.1]),
                track(TrackKind::System, &[0.5, -0.5, 0.0, 0.2]),
            ],
        };
        let mixed = rec.mixed();
        assert_eq!(mixed.len(), 4);
        assert_eq!(&mixed[..2], &[1.0, -1.0]);
        assert!((mixed[2] - 0.1).abs() < 1e-6);
        assert!((mixed[3] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn duration_follows_the_longest_track() {
        let rec = Recording {
            tracks: vec![track(TrackKind::Mic, &[0.0; 800]), track(TrackKind::System, &[0.0; 16_000])],
        };
        assert_eq!(rec.duration_ms(), 1_000);
    }

    #[test]
    fn silence_helpers() {
        let rec = Recording {
            tracks: vec![track(TrackKind::Mic, &[0.0, 0.0]), track(TrackKind::System, &[0.0, 0.3])],
        };
        assert!(rec.mic_is_digital_silence());
        assert!(!rec.is_digital_silence());

        let only_system = Recording {
            tracks: vec![track(TrackKind::System, &[0.0])],
        };
        assert!(!only_system.mic_is_digital_silence()); // no mic track
        assert!(only_system.is_digital_silence());

        assert!(Recording { tracks: vec![] }.is_digital_silence());
        let live_mic = Recording {
            tracks: vec![track(TrackKind::Mic, &[0.0, 0.01])],
        };
        assert!(!live_mic.mic_is_digital_silence());
    }

    #[test]
    fn wav_is_mono_16_bit_at_16_khz() {
        let wav = encode_wav(&[1.0, -1.0, 0.0]).unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 16_000, 16));
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples, vec![i16::MAX, -i16::MAX, 0]);
    }

    #[test]
    fn wav_clamps_out_of_range_samples() {
        let wav = encode_wav(&[3.0, -3.0]).unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples, vec![i16::MAX, -i16::MAX]);
    }
}
