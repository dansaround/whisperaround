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
//!
//! The default devices can change mid-recording (headphones plugged in), and a
//! device that goes away stops its stream without a word. The capture thread
//! watches for both and reopens the stream into the same track, so the track
//! keeps its timeline and the gap is padded like any other silence.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    /// Counts the times the track's stream was reopened on another device, so
    /// a listener can tell that what it learned about the signal path is stale.
    pub generation: u64,
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

/// Frames at [`SAMPLE_RATE`] that `len` frames at `rate` become.
fn out_len(len: usize, rate: u32) -> u64 {
    (len as f64 * SAMPLE_RATE as f64 / rate as f64).round() as u64
}

/// Linear-interpolation resampling of `input` from `rate` to [`SAMPLE_RATE`].
fn resample(input: &[f32], rate: u32) -> Vec<f32> {
    if rate == SAMPLE_RATE || input.is_empty() {
        return input.to_vec();
    }
    let n_out = out_len(input.len(), rate) as usize;
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
/// [`Recorder`] handle. Its stream can be swapped for another device while
/// recording, so audio is kept at [`SAMPLE_RATE`] and the rate may change.
struct TrackBuf {
    kind: TrackKind,
    /// Native sample rate of the device feeding `pending` right now.
    rate: u32,
    /// When capture began; the wall clock that gaps are measured against.
    start: Instant,
    /// Frames at [`SAMPLE_RATE`] already drained.
    drained: u64,
    /// Resampled mono samples at [`SAMPLE_RATE`] not drained yet.
    done: Vec<f32>,
    /// Mono samples at `rate` not resampled yet.
    pending: Vec<f32>,
    /// Streams opened on this track so far.
    generation: u64,
}

type SharedTrack = Arc<Mutex<TrackBuf>>;

impl TrackBuf {
    fn new(kind: TrackKind, rate: u32) -> Self {
        Self {
            kind,
            rate,
            start: Instant::now(),
            drained: 0,
            done: Vec::new(),
            pending: Vec::new(),
            generation: 0,
        }
    }

    /// Frames at [`SAMPLE_RATE`] produced since the start, drained and
    /// pending ones included.
    fn produced(&self) -> u64 {
        self.drained + self.done.len() as u64 + out_len(self.pending.len(), self.rate)
    }

    /// Resample everything pending into `done`.
    fn flush(&mut self) {
        let raw = std::mem::take(&mut self.pending);
        self.done.extend(resample(&raw, self.rate));
    }

    /// Pad zeros so that the frames so far plus `incoming` match the wall clock.
    fn pad(&mut self, incoming: u64) {
        let missing = missing_frames(self.start.elapsed(), SAMPLE_RATE, self.produced() + incoming);
        if missing > 0 {
            self.flush();
            self.done.resize(self.done.len() + missing as usize, 0.0);
        }
    }

    fn append(&mut self, chunk: &[f32]) {
        self.pad(out_len(chunk.len(), self.rate));
        self.pending.extend_from_slice(chunk);
    }

    /// Continue with a new stream, on a device running at `rate`. Audio so far
    /// keeps its old rate.
    fn set_rate(&mut self, rate: u32) {
        self.flush();
        self.rate = rate;
        self.generation += 1;
    }

    /// Take everything not drained yet, at [`SAMPLE_RATE`].
    fn take(&mut self) -> Track {
        self.pad(0);
        self.flush();
        self.drained += self.done.len() as u64;
        Track {
            kind: self.kind,
            samples: std::mem::take(&mut self.done),
            generation: self.generation,
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

/// Build an input stream of sample type `T` that feeds `track`. An error from
/// the stream (typically its device went away) sets `failed` for the watcher.
fn build_stream<T: cpal::SizedSample + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    convert: fn(T) -> f32,
    track: SharedTrack,
    level: Arc<AtomicU32>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels as usize;
    device.build_input_stream(
        config,
        move |data: &[T], _: &_| push_chunk(&track, &level, &downmix(data, channels, convert)),
        move |e| {
            eprintln!("[whisperaround] cpal stream error: {e}");
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )
}

/// Start capturing `device` into `track`: the input of a microphone, or the
/// loopback of an output device for system audio (cpal turns an input stream
/// on a render device into a WASAPI loopback capture). The track takes the
/// device's sample rate, so it may be a different one than before.
fn start_stream(
    kind: TrackKind,
    device: &cpal::Device,
    track: &SharedTrack,
    level: &Arc<AtomicU32>,
    failed: &Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let config = match kind {
        TrackKind::Mic => device
            .default_input_config()
            .map_err(|e| anyhow!(tf("error.mic.config", &[("error", &e)])))?,
        TrackKind::System => device
            .default_output_config()
            .map_err(|e| anyhow!(tf("error.systemAudio.config", &[("error", &e)])))?,
    };
    let format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    track.lock().unwrap().set_rate(stream_config.sample_rate.0);
    let (tr, lv, fl) = (track.clone(), level.clone(), failed.clone());

    let stream = match format {
        cpal::SampleFormat::F32 => build_stream::<f32>(device, &stream_config, |s| s, tr, lv, fl),
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(device, &stream_config, |s| s as f32 / i16::MAX as f32, tr, lv, fl)
        }
        cpal::SampleFormat::U16 => build_stream::<u16>(
            device,
            &stream_config,
            |s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0,
            tr,
            lv,
            fl,
        ),
        other => return Err(anyhow!(tf("error.mic.sampleFormat", &[("format", &format!("{other:?}"))]))),
    }
    .map_err(|e| anyhow!(tf("error.mic.buildStream", &[("error", &e)])))?;

    stream.play().map_err(|e| anyhow!(tf("error.mic.startStream", &[("error", &e)])))?;
    Ok(stream)
}

/// The microphone named `name`, or the default input when it is empty or no
/// longer connected.
fn find_input(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    (!name.is_empty())
        .then(|| host.input_devices().ok())
        .flatten()
        .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(name)))
        .or_else(|| host.default_input_device())
}

/// Picks the device a [`Slot`] should capture from, as it is right now.
type Target = Box<dyn Fn() -> Option<cpal::Device>>;

/// How often the capture thread checks that each stream still runs on the
/// device it should: plugging in headphones changes the defaults, and WASAPI
/// keeps capturing the old device until it is told otherwise.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// A track and the stream feeding it. The stream is reopened in place when
/// its device changes or it fails, so the track keeps going.
struct Slot {
    kind: TrackKind,
    track: SharedTrack,
    level: Arc<AtomicU32>,
    target: Target,
    /// Name of the device `stream` runs on; empty when there is none.
    device: String,
    stream: Option<cpal::Stream>,
    /// Set by the stream's error callback.
    failed: Arc<AtomicBool>,
}

impl Slot {
    /// Start the stream on `device`, replacing any stream already running.
    fn open(&mut self, device: &cpal::Device, name: String) -> Result<()> {
        // Stop the old stream first, so the two never feed the track at once.
        self.stream = None;
        let failed = Arc::new(AtomicBool::new(false));
        self.stream = Some(start_stream(self.kind, device, &self.track, &self.level, &failed)?);
        self.device = name;
        self.failed = failed;
        Ok(())
    }

    /// Reopen the stream if it failed or its device is no longer the one that
    /// should be used. A failed attempt is retried on the next check.
    fn refresh(&mut self) {
        let Some(device) = (self.target)() else {
            // Nothing to capture from right now; resume once there is.
            self.stream = None;
            self.device.clear();
            return;
        };
        let name = device.name().unwrap_or_else(|_| self.device.clone());
        let healthy = self.stream.is_some() && !self.failed.load(Ordering::Relaxed);
        if healthy && name == self.device {
            return;
        }
        if let Err(e) = self.open(&device, name) {
            eprintln!("[whisperaround] reopening the {:?} track: {e}", self.kind);
        }
    }
}

/// Open the first stream for `kind`, on the device `target` picks now.
fn open_slot(kind: TrackKind, target: Target, level: &Arc<AtomicU32>) -> Result<Slot> {
    let mut slot = Slot {
        kind,
        track: Arc::new(Mutex::new(TrackBuf::new(kind, SAMPLE_RATE))),
        level: level.clone(),
        target,
        device: String::new(),
        stream: None,
        failed: Arc::new(AtomicBool::new(false)),
    };
    let device = (slot.target)().ok_or_else(|| {
        anyhow!(match kind {
            TrackKind::Mic => t("error.mic.noDevice"),
            TrackKind::System => t("error.systemAudio.noDevice"),
        })
    })?;
    let name = device.name().unwrap_or_default();
    slot.open(&device, name)?;
    // Measure gaps from when audio can actually flow, not from device setup.
    slot.track.lock().unwrap().start = Instant::now();
    Ok(slot)
}

/// Open every stream `source` needs, one track each (microphone first).
fn open_streams(source: &Source, level: &Arc<AtomicU32>) -> Result<Vec<Slot>> {
    // Fail before opening the microphone of a mix we can't complete.
    if !cfg!(windows) && !matches!(source, Source::Mic(_)) {
        return Err(anyhow!(t("error.systemAudio.unsupported")));
    }
    let mic = |name: &str| -> Target {
        let name = name.to_string();
        Box::new(move || find_input(&name))
    };
    let system = || {
        open_slot(TrackKind::System, Box::new(|| cpal::default_host().default_output_device()), level)
            .map_err(|e| anyhow!(tf("error.systemAudio.open", &[("error", &e)])))
    };
    Ok(match source {
        Source::Mic(name) => vec![open_slot(TrackKind::Mic, mic(name), level)?],
        Source::System => vec![system()?],
        Source::Mix(name) => vec![open_slot(TrackKind::Mic, mic(name), level)?, system()?],
    })
}

impl Recorder {
    /// Open the source described by the `input_device` setting (see
    /// [`Source::parse`]) and start capturing. Returns once the streams are
    /// actually running (or with an error if any could not start). Until
    /// [`Recorder::stop`], each stream follows its device (see `Slot`).
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
            let mut slots = match open_streams(&thread_source, &level) {
                Ok(slots) => {
                    let _ = ready_tx.send(Ok(slots.iter().map(|s| s.track.clone()).collect()));
                    slots
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!("{e}")));
                    return Err(e);
                }
            };

            // Capture until stop is signalled, reporting the level meanwhile
            // and reopening any stream whose device changed or failed.
            let mut checked = Instant::now();
            loop {
                match stop_rx.recv_timeout(LEVEL_INTERVAL) {
                    Err(RecvTimeoutError::Timeout) => {
                        let rms = f32::from_bits(level.swap(0, Ordering::Relaxed));
                        on_level(normalize_level(rms));
                        if checked.elapsed() >= WATCH_INTERVAL {
                            checked = Instant::now();
                            slots.iter_mut().for_each(Slot::refresh);
                        }
                    }
                    _ => break,
                }
            }
            drop(slots); // stops the streams and flushes callbacks
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

    /// Mixed audio (as in [`Recording::mixed`]) from frame `from` on, without
    /// draining, so it is still in the final recording. Stops at the shortest
    /// track so a mix stays aligned. Only for recorders that are never drained.
    pub fn mixed_since(&self, from: usize) -> Vec<f32> {
        // Flush without draining: `done` keeps every sample, so the indexes
        // match the ones of the final recording.
        let mut tracks: Vec<std::sync::MutexGuard<'_, TrackBuf>> =
            self.tracks.iter().map(|t| t.lock().unwrap()).collect();
        for track in tracks.iter_mut() {
            track.flush();
        }
        let end = tracks.iter().map(|t| t.done.len()).min().unwrap_or(0);
        if end <= from {
            return Vec::new();
        }
        let mut out = vec![0.0f32; end - from];
        for track in &tracks {
            for (o, s) in out.iter_mut().zip(&track.done[from..end]) {
                *o += s;
            }
        }
        out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
        out
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
            generation: 0,
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
        assert_eq!(buf.lock().unwrap().pending.len(), 4);
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
        let out = buf.take().samples;
        // A second at SAMPLE_RATE: silence first, then the chunk (1 kHz → 16 kHz).
        assert!((out.len() as i64 - 16_000).abs() < 20);
        assert_eq!(out[0], 0.0);
        assert!(out[out.len() - 1_600..].iter().all(|&s| s == 0.5));
    }

    #[test]
    fn take_drains_and_resamples() {
        let mut buf = TrackBuf::new(TrackKind::Mic, 48_000);
        buf.append(&[0.25; 4_800]);
        let first = buf.take();
        assert_eq!(first.kind, TrackKind::Mic);
        assert_eq!(first.samples.len(), 1_600);
        assert!(buf.done.is_empty() && buf.pending.is_empty());
        assert!(buf.take().samples.is_empty());
    }

    #[test]
    fn draining_does_not_make_the_track_look_behind() {
        // A meeting drains every second; audio after a drain must not be
        // preceded by silence for the time already drained.
        let mut buf = TrackBuf::new(TrackKind::System, SAMPLE_RATE);
        buf.start = Instant::now() - Duration::from_secs(1);
        buf.append(&[0.5; 16_000]);
        assert_eq!(buf.take().samples.len(), 16_000);
        buf.append(&[0.5; 160]);
        assert_eq!(buf.take().samples, vec![0.5; 160]);
    }

    #[test]
    fn set_rate_keeps_each_second_its_length() {
        // A device switch from 1 kHz to 2 kHz mid-recording.
        let mut buf = TrackBuf::new(TrackKind::System, 1_000);
        buf.append(&[0.5; 1_000]);
        buf.set_rate(2_000);
        buf.append(&[0.5; 2_000]);
        assert_eq!(buf.take().samples.len(), 32_000);
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
