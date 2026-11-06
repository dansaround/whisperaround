//! OpenAI transcription over HTTPS.

use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::i18n::{t, tf};

const ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";

/// Default cloud model for file transcription.
pub const DEFAULT_MODEL: &str = "gpt-transcribe";
/// Realtime model. It only works over the WebSocket; file requests use `DEFAULT_MODEL` instead.
pub const LIVE_MODEL: &str = "gpt-live-transcribe";
/// Retired ids (OpenAI shuts them down on 2027-02-26). Settings migrate them to `DEFAULT_MODEL`.
pub const DEPRECATED: &[&str] = &["whisper-1", "gpt-4o-transcribe", "gpt-4o-mini-transcribe"];

/// Optional transcription hints shared by the OpenAI and local backends.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// ISO-639-1 code; empty = auto-detect.
    pub language: String,
    /// Free-form context for the local faster-whisper models. Cloud models use `keywords`.
    pub prompt: Option<String>,
    /// Vocabulary terms (names, jargon) expected in the audio. Sent to gpt-transcribe as `keywords[]`.
    pub keywords: Vec<String>,
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Whether a vocabulary term can be sent as a `keywords[]` field. The API rejects the whole
/// request if a term contains `<`, `>` or a line break, so such terms are dropped.
fn valid_keyword(k: &str) -> bool {
    !k.trim().is_empty() && !k.contains(['<', '>', '\r', '\n'])
}

/// Send a WAV buffer to the OpenAI transcription endpoint and return the text.
///
/// `model` is `DEFAULT_MODEL` or `LIVE_MODEL`; the live model is mapped to `DEFAULT_MODEL`
/// because it has no HTTP endpoint.
pub async fn transcribe(api_key: &str, model: &str, wav: Vec<u8>, opts: &Options) -> Result<String> {
    if api_key.trim().is_empty() {
        return Err(anyhow!(t("error.openai.noKey")));
    }

    // The live model only works over the realtime WebSocket; for a file this is the fallback.
    let model = if model == LIVE_MODEL { DEFAULT_MODEL } else { model };

    let part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = reqwest::multipart::Form::new()
        .text("model", model.to_string())
        .part("file", part);
    // gpt-transcribe takes `languages` (never together with `language`) and `keywords`.
    // The vocabulary goes only as keywords, not as `prompt`.
    if !opts.language.is_empty() {
        form = form.text("languages[]", opts.language.clone());
    }
    for keyword in opts.keywords.iter().map(|k| k.trim()).filter(|k| valid_keyword(k)) {
        form = form.text("keywords[]", keyword.to_string());
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
        let err = tauri::async_runtime::block_on(transcribe("  ", DEFAULT_MODEL, vec![], &Options::default()))
            .unwrap_err();
        assert!(err.to_string().contains("API key"));
    }

    #[test]
    fn valid_keyword_rejects_empty_and_api_breaking_characters() {
        assert!(valid_keyword("Tauri"));
        assert!(valid_keyword("Señor Ñandú"));
        assert!(!valid_keyword(""));
        assert!(!valid_keyword("   "));
        assert!(!valid_keyword("a<b"));
        assert!(!valid_keyword("a>b"));
        assert!(!valid_keyword("a\nb"));
        assert!(!valid_keyword("a\rb"));
    }
}
