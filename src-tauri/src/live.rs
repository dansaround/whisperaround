//! Live dictation over OpenAI's realtime transcription WebSocket
//! (`gpt-live-transcribe`). While the user records, the audio is streamed in
//! 100 ms chunks; on stop the server commits the buffer and returns the final
//! text almost at once. Any failure yields `None`, and the caller falls back to
//! the file model with the WAV.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::state::AppState;
use crate::whisper::{self, Options};

/// Realtime endpoint. The model goes in `session.update`, not in the URL.
const URL: &str = "wss://api.openai.com/v1/realtime?intent=transcription";
/// Interval between streamed audio chunks.
const CHUNK: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the server may take to return the text after the commit.
const TRANSCRIPT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long `finish` waits for the session before it gives up.
const FINISH_TIMEOUT: Duration = Duration::from_secs(20);

/// Messages for the session task.
enum Msg {
    /// Stop streaming. Carries the final mixed recording; the task sends the
    /// part it has not streamed yet.
    Finish(Vec<f32>),
}

/// The handle `start` keeps while a session runs.
struct Session {
    /// Dropping it ends the task (its receiver closes), which is how `cancel` stops it.
    tx: mpsc::UnboundedSender<Msg>,
    /// The task's answer: the text, or `None` if the session failed.
    done: oneshot::Receiver<Option<String>>,
}

/// The session of the recording in progress. Only one exists at a time.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn slot() -> MutexGuard<'static, Option<Session>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

/// Start streaming the recording to gpt-live-transcribe. Call right after the
/// recorder started. A session left over from an earlier recording is dropped.
pub fn start(app: &AppHandle, api_key: String, opts: Options) {
    let (tx, rx) = mpsc::unbounded_channel();
    let (done_tx, done) = oneshot::channel();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run(&app, &api_key, &opts, rx).await;
        if let Err(e) = &result {
            eprintln!("[whisperaround] live transcription failed: {e:#}");
        }
        // Nobody waits for the answer after a cancel; the send error is then fine.
        let _ = done_tx.send(result.ok());
    });
    *slot() = Some(Session { tx, done });
}

/// Finish the live session. `all` is the final `Recording::mixed()`: the
/// session sends the audio after the last streamed frame, commits, and waits
/// for the text. `None` if there is no session or it failed or timed out; the
/// caller then uses the file model.
pub async fn finish(all: &[f32]) -> Option<String> {
    let session = slot().take();
    let Some(Session { tx, done }) = session else {
        return None;
    };
    let _ = tx.send(Msg::Finish(all.to_vec()));
    match timeout(FINISH_TIMEOUT, done).await {
        Ok(Ok(text)) => text,
        _ => None,
    }
}

/// Drop the session without a result (the recording was cancelled).
pub fn cancel() {
    drop(slot().take());
}

/// One session from connect to the final text. Any error ends it.
async fn run(
    app: &AppHandle,
    api_key: &str,
    opts: &Options,
    mut rx: mpsc::UnboundedReceiver<Msg>,
) -> Result<String> {
    // Install the TLS crypto provider once; an error only means it is already set.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut req = URL.into_client_request()?;
    let auth = HeaderValue::from_str(&format!("Bearer {api_key}"))?;
    req.headers_mut().insert("Authorization", auth);
    let (ws, _) = timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(req))
        .await
        .map_err(|_| anyhow!("connect timed out"))??;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::text(session_update(opts).to_string())).await?;

    // Frames already streamed. Everything after them goes out at the finish.
    let mut sent: usize = 0;
    let mut tick = interval(CHUNK);
    let final_audio = loop {
        tokio::select! {
            _ = tick.tick() => {
                let new = new_audio(app, sent);
                if !new.is_empty() {
                    sent += new.len();
                    sink.send(append(&new)).await?;
                }
            }
            msg = rx.recv() => {
                match msg {
                    Some(Msg::Finish(all)) => break all,
                    // The sender is gone: the recording was cancelled.
                    None => bail!("cancelled"),
                }
            }
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if let Server::Failed(msg) = server_event(text.as_str()) {
                            bail!("OpenAI: {msg}");
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => bail!("connection closed while recording"),
                }
            }
        }
    };

    // Send the rest, then commit: the server transcribes the whole buffer once.
    let rest = &final_audio[sent.min(final_audio.len())..];
    if !rest.is_empty() {
        sink.send(append(rest)).await?;
    }
    sink.send(Message::text(json!({"type": "input_audio_buffer.commit"}).to_string())).await?;
    let text = timeout(TRANSCRIPT_TIMEOUT, read_transcript(&mut stream))
        .await
        .map_err(|_| anyhow!("no transcript from OpenAI"))??;
    let _ = sink.close().await;
    Ok(text)
}

/// Read server events until the transcript of the commit arrives.
async fn read_transcript<S>(stream: &mut S) -> Result<String>
where
    S: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => match server_event(text.as_str()) {
                Server::Completed(transcript) => return Ok(transcript),
                Server::Failed(msg) => bail!("OpenAI: {msg}"),
                Server::Ignore => {}
            },
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.into()),
            None => bail!("connection closed before the transcript"),
        }
    }
}

/// What a server event means for the session.
enum Server {
    Completed(String),
    Failed(String),
    Ignore,
}

/// Classify a server event by its `type`. Unknown and malformed events are ignored.
fn server_event(text: &str) -> Server {
    let Ok(event) = serde_json::from_str::<Value>(text) else {
        return Server::Ignore;
    };
    match event["type"].as_str() {
        Some("conversation.item.input_audio_transcription.completed") => {
            Server::Completed(event["transcript"].as_str().unwrap_or_default().to_string())
        }
        Some("conversation.item.input_audio_transcription.failed") | Some("error") => {
            Server::Failed(event["error"]["message"].as_str().unwrap_or("unknown error").to_string())
        }
        _ => Server::Ignore,
    }
}

/// Mixed audio of the running recording from frame `from` on. Empty when no
/// recording runs (it was stopped or cancelled).
fn new_audio(app: &AppHandle, from: usize) -> Vec<f32> {
    let state = app.state::<AppState>();
    let Ok(recorder) = state.recorder.lock() else {
        return Vec::new();
    };
    let samples = match recorder.as_ref() {
        Some(rec) => rec.mixed_since(from),
        None => Vec::new(),
    };
    samples
}

/// An `input_audio_buffer.append` event carrying mono 16 kHz samples.
fn append(samples: &[f32]) -> Message {
    let audio = STANDARD.encode(to_pcm16_24k(samples));
    Message::text(json!({"type": "input_audio_buffer.append", "audio": audio}).to_string())
}

/// Resample mono 16 kHz samples to 24 kHz and encode them as little-endian PCM16.
/// Each chunk is resampled on its own; the error at the seams is inaudible.
fn to_pcm16_24k(samples_16k: &[f32]) -> Vec<u8> {
    let n = samples_16k.len();
    if n == 0 {
        return Vec::new();
    }
    let out_len = (n as f64 * 1.5).round() as usize;
    let last = n - 1;
    let mut out = Vec::with_capacity(out_len * 2);
    for i in 0..out_len {
        // Input position of output frame `i`: 16 kHz / 24 kHz = 2/3.
        let pos = i as f64 * 2.0 / 3.0;
        let idx = (pos as usize).min(last);
        let next = (idx + 1).min(last);
        let frac = (pos - idx as f64) as f32;
        let s = samples_16k[idx] + (samples_16k[next] - samples_16k[idx]) * frac;
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// The `session.update` that opens the transcription session. `languages` is
/// left out for auto-detect, and `keywords` when no valid term is left; the
/// keyword filter is the one the file model uses.
fn session_update(opts: &Options) -> Value {
    let mut transcription = json!({"model": whisper::LIVE_MODEL, "delay": "low"});
    if !opts.language.is_empty() {
        transcription["languages"] = json!([opts.language]);
    }
    let keywords: Vec<&str> = opts
        .keywords
        .iter()
        .map(|k| k.trim())
        .filter(|k| whisper::valid_keyword(k))
        .collect();
    if !keywords.is_empty() {
        transcription["keywords"] = json!(keywords);
    }
    json!({
        "type": "session.update",
        "session": {
            "type": "transcription",
            "audio": {
                "input": {
                    "format": {"type": "audio/pcm", "rate": 24000},
                    "transcription": transcription,
                    // The live model has no server VAD; the stop key ends the turn.
                    "turn_detection": null
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_is_24k_mono_pcm16() {
        // 160 frames at 16 kHz are 240 frames at 24 kHz, two bytes each.
        assert_eq!(to_pcm16_24k(&[0.0; 160]).len(), 480);
        assert!(to_pcm16_24k(&[]).is_empty());
    }

    #[test]
    fn full_scale_maps_to_i16_range() {
        let bytes = to_pcm16_24k(&[1.0, -1.0]);
        // Output frame 0 is input 0, frame 2 (position 1.33) is input 1.
        assert_eq!(&bytes[..2], &32767i16.to_le_bytes());
        assert_eq!(&bytes[4..6], &(-32767i16).to_le_bytes());
    }

    #[test]
    fn session_update_omits_empty_language_and_invalid_keywords() {
        let opts = Options {
            language: String::new(),
            prompt: None,
            keywords: vec![" Tauri ".into(), "a<b".into(), "  ".into()],
        };
        let v = session_update(&opts);
        let input = &v["session"]["audio"]["input"];
        let t = &input["transcription"];
        assert_eq!(t["model"], "gpt-live-transcribe");
        assert!(t.get("languages").is_none());
        assert_eq!(t["keywords"], json!(["Tauri"]));
        assert_eq!(input["turn_detection"], Value::Null);
    }

    #[test]
    fn session_update_sends_language_and_no_keywords_when_none_are_valid() {
        let opts = Options {
            language: "es".into(),
            ..Default::default()
        };
        let v = session_update(&opts);
        let t = &v["session"]["audio"]["input"]["transcription"];
        assert_eq!(t["languages"], json!(["es"]));
        assert!(t.get("keywords").is_none());
    }
}
