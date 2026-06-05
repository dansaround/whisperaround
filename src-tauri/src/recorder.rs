//! Native microphone capture via `cpal`, encoded to 16-bit PCM WAV with `hound`.
//!
//! `cpal::Stream` is `!Send`, so the stream is owned by a dedicated thread. The
//! [`Recorder`] handle communicates with that thread over channels: it waits for
//! the stream to start (surfacing device errors synchronously) and signals it to
//! stop, after which the captured samples are returned.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

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

impl Recorder {
    /// Open the default input device and start capturing. Returns once the
    /// stream is actually running (or with an error if it could not start).
    pub fn start() -> Result<Recorder> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

        let handle = thread::spawn(move || -> Result<Recording> {
            // Build the stream; report any setup error back to `start()`.
            let setup = (|| -> Result<(cpal::Stream, Arc<Mutex<Vec<f32>>>, u32, u16)> {
                let host = cpal::default_host();
                let device = host
                    .default_input_device()
                    .ok_or_else(|| anyhow!("no hay dispositivo de entrada disponible"))?;
                let config = device
                    .default_input_config()
                    .map_err(|e| anyhow!("config de entrada por defecto: {e}"))?;

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
                            buf_cb.lock().unwrap().extend_from_slice(data);
                        },
                        err_fn,
                        None,
                    ),
                    cpal::SampleFormat::I16 => device.build_input_stream(
                        &stream_config,
                        move |data: &[i16], _: &_| {
                            let mut b = buf_cb.lock().unwrap();
                            b.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                        },
                        err_fn,
                        None,
                    ),
                    cpal::SampleFormat::U16 => device.build_input_stream(
                        &stream_config,
                        move |data: &[u16], _: &_| {
                            let mut b = buf_cb.lock().unwrap();
                            b.extend(
                                data.iter()
                                    .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                            );
                        },
                        err_fn,
                        None,
                    ),
                    other => return Err(anyhow!("formato de muestra no soportado: {other:?}")),
                }
                .map_err(|e| anyhow!("construyendo stream: {e}"))?;

                stream.play().map_err(|e| anyhow!("iniciando stream: {e}"))?;
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

            // Capture until stop is signalled.
            let _ = stop_rx.recv();
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
            Err(_) => Err(anyhow!("el hilo de grabación terminó antes de iniciar")),
        }
    }

    /// Stop capturing and return the collected audio.
    pub fn stop(self) -> Result<Recording> {
        let _ = self.stop_tx.send(());
        self.handle
            .join()
            .map_err(|_| anyhow!("el hilo de grabación entró en pánico"))?
    }
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
