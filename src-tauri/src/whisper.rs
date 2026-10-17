//! OpenAI Whisper transcription over HTTPS.

use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::i18n::{t, tf};

const ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";

/// Optional transcription hints shared by the OpenAI and local backends.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// ISO-639-1 code; empty = auto-detect.
    pub language: String,
    /// Vocabulary hint (names, jargon) to steer spelling.
    pub prompt: Option<String>,
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Send a WAV buffer to the OpenAI transcription endpoint and return the text.
///
/// `model` is typically `whisper-1` (also accepts `gpt-4o-transcribe` /
/// `gpt-4o-mini-transcribe`).
pub async fn transcribe(api_key: &str, model: &str, wav: Vec<u8>, opts: &Options) -> Result<String> {
    if api_key.trim().is_empty() {
        return Err(anyhow!(t("error.openai.noKey")));
    }

    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .part("file", part);
    if !opts.language.is_empty() {
        form = form.text("language", opts.language.clone());
    }
    if let Some(prompt) = &opts.prompt {
        form = form.text("prompt", prompt.clone());
    }

    let resp = reqwest::Client::new()
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| anyhow!(tf("error.openai.network", &[("error", &e)])))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(tf("error.openai.status", &[("status", &status), ("body", &body)])));
    }

    let parsed: TranscriptionResponse = resp
        .json()
        .await
        .map_err(|e| anyhow!(tf("error.openai.badResponse", &[("error", &e)])))?;
    Ok(parsed.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The API key is only ever sent here: OpenAI, over HTTPS.
    #[test]
    fn key_goes_only_to_openai_over_https() {
        assert!(ENDPOINT.starts_with("https://api.openai.com/"));
    }

    #[test]
    fn missing_key_fails_before_any_request() {
        let err = tauri::async_runtime::block_on(transcribe("  ", "whisper-1", vec![], &Options::default()))
            .unwrap_err();
        assert!(err.to_string().contains("API key"));
    }
}
