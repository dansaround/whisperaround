//! Native microphone capture via `cpal`, encoded to 16-bit PCM WAV with `hound`.
//!
//! `cpal::Stream` is `!Send`, so the stream is owned by a dedicated thread. The
//! [`Recorder`] handle communicates with that thread over channels: it waits for
//! the stream to start (surfacing device errors synchronously) and signals it to
//! stop, after which the captured samples are returned. While capturing, the
//! thread also reports the input level so the UI can draw a live waveform.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::i18n::{t, tf};

/// Raw captured audio (interleaved `f32` frames in `[-1.0, 1.0]`).
pub struct Recording {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Handle to an in-progress recording running on its own thread.
pub struct Recorder {
    stop_tx: Sender<()>,
    handle: JoinHandle<Result<Recording>>,
}

/// How often the input level is reported while recording (~60 Hz, so the
/// waveform moves every frame).
const LEVEL_INTERVAL: Duration = Duration::from_millis(16);

/// Append a converted chunk to the buffer and raise the pending level to the
/// chunk's RMS (the reporter resets it after each read, so it holds the
/// loudest chunk since the last report).
fn push_chunk(buf: &Mutex<Vec<f32>>, level: &AtomicU32, chunk: impl Iterator<Item = f32>) {
    let mut b = buf.lock().unwrap();
    let start = b.len();
    b.extend(chunk);
    let new = &b[start..];
    if new.is_empty() {
        return;
    }
    let rms = (new.iter().map(|s| s * s).sum::<f32>() / new.len() as f32).sqrt();
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

impl Recorder {
    /// Open the input device named `device_name` (the system default when
    /// empty or no longer connected) and start capturing. Returns once the
    /// stream is actually running (or with an error if it could not start).
    /// `on_level` is called every [`LEVEL_INTERVAL`] with the normalized input
    /// level from the capture thread.
    pub fn start(device_name: &str, on_level: impl Fn(f32) + Send + 'static) -> Result<Recorder> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let device_name = device_name.to_string();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

        let handle = thread::spawn(move || -> Result<Recording> {
            // Build the stream; report any setup error back to `start()`.
            let level = Arc::new(AtomicU32::new(0));
            let level_cb = level.clone();
            let setup = (|| -> Result<(cpal::Stream, Arc<Mutex<Vec<f32>>>, u32, u16)> {
                let host = cpal::default_host();
                let named = (!device_name.is_empty())
                    .then(|| host.input_devices().ok())
                    .flatten()
                    .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(&device_name)));
                let device = named
                    .or_else(|| host.default_input_device())
                    .ok_or_else(|| anyhow!(t("error.mic.noDevice")))?;
                let config = device
                    .default_input_config()
                    .map_err(|e| anyhow!(tf("error.mic.config", &[("error", &e)])))?;

                let sample_rate = config.sample_rate().0;
                let channels = config.channels();
                let sample_format = config.sample_format();
                let stream_config: cpal::StreamConfig = config.into();

                let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
                let buf_cb = buffer.clone();
                let err_fn = |e| eprintln!("[whisperaround] cpal stream error: {e}");

                let stream = match sample_format {
                    cpal::SampleFormat::F32 => device.build_input_stream(
                        &stream_config,
                        move |data: &[f32], _: &_| {
                            push_chunk(&buf_cb, &level_cb, data.iter().copied());
                        },
                        err_fn,
                        None,
                    ),
                    cpal::SampleFormat::I16 => device.build_input_stream(
                        &stream_config,
                        move |data: &[i16], _: &_| {
                            push_chunk(
                                &buf_cb,
                                &level_cb,
                                data.iter().map(|&s| s as f32 / i16::MAX as f32),
                            );
                        },
                        err_fn,
                        None,
                    ),
                    cpal::SampleFormat::U16 => device.build_input_stream(
                        &stream_config,
                        move |data: &[u16], _: &_| {
                            push_chunk(
                                &buf_cb,
                                &level_cb,
                                data.iter()
                                    .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                            );
                        },
                        err_fn,
                        None,
                    ),
                    other => return Err(anyhow!(tf("error.mic.sampleFormat", &[("format", &format!("{other:?}"))]))),
                }
                .map_err(|e| anyhow!(tf("error.mic.buildStream", &[("error", &e)])))?;

                stream.play().map_err(|e| anyhow!(tf("error.mic.startStream", &[("error", &e)])))?;
                Ok((stream, buffer, sample_rate, channels))
            })();

            let (stream, buffer, sample_rate, channels) = match setup {
                Ok(parts) => {
                    let _ = ready_tx.send(Ok(()));
                    parts
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
            drop(stream); // stops the stream and flushes callbacks

            let samples = Arc::try_unwrap(buffer)
                .map(|m| m.into_inner().unwrap())
                .unwrap_or_else(|arc| arc.lock().unwrap().clone());

            Ok(Recording {
                samples,
                sample_rate,
                channels,
            })
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Recorder { stop_tx, handle }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!(t("error.recording.threadEnded"))),
        }
    }

    /// Stop capturing and return the collected audio.
    pub fn stop(self) -> Result<Recording> {
        let _ = self.stop_tx.send(());
        self.handle
            .join()
            .map_err(|_| anyhow!(t("error.recording.threadPanicked")))?
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

/// Encode a [`Recording`] to an in-memory 16-bit PCM mono WAV.
///
/// Multi-channel input is down-mixed to mono by averaging channels, which is all
/// Whisper needs and keeps the upload small.
pub fn encode_wav(rec: &Recording) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rec.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        let ch = rec.channels.max(1) as usize;
        for frame in rec.samples.chunks(ch) {
            let avg = frame.iter().sum::<f32>() / ch as f32;
            let sample = (avg.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_maps_decibels_to_unit_range() {
        assert_eq!(normalize_level(0.0), 0.0);
        assert_eq!(normalize_level(0.001), 0.0); // -60 dBFS: silence
        assert!((normalize_level(0.01) - 0.4).abs() < 1e-4); // -40 dBFS
        assert_eq!(normalize_level(1.0), 1.0); // clipping stays at 1
    }

    #[test]
    fn push_chunk_appends_and_keeps_loudest_rms() {
        let buf = Mutex::new(Vec::new());
        let level = AtomicU32::new(0);
        push_chunk(&buf, &level, [0.5f32, -0.5].into_iter());
        push_chunk(&buf, &level, [0.1f32, -0.1].into_iter());
        assert_eq!(buf.lock().unwrap().len(), 4);
        assert_eq!(f32::from_bits(level.load(Ordering::Relaxed)), 0.5);
    }

    #[test]
    fn wav_is_mono_16_bit_at_the_source_rate() {
        // Two stereo frames: L/R averaged into one mono sample each.
        let rec = Recording {
            samples: vec![1.0, 0.0, -0.5, -0.5],
            sample_rate: 48_000,
            channels: 2,
        };
        let wav = encode_wav(&rec).unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 48_000, 16));
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples, vec![i16::MAX / 2, -(i16::MAX / 2)]);
    }

    #[test]
    fn wav_clamps_out_of_range_samples() {
        let rec = Recording {
            samples: vec![3.0, -3.0],
            sample_rate: 16_000,
            channels: 1,
        };
        let wav = encode_wav(&rec).unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples, vec![i16::MAX, -i16::MAX]);
    }
}
