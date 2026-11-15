//! Estimated OpenAI spend from published list prices.
//!
//! The meter is not a bill: it counts audio we sent and tokens the Responses
//! API reported. Local models add nothing.

/// gpt-transcribe: USD per minute of audio.
const TRANSCRIBE_USD_PER_MIN: f64 = 0.0045;
/// gpt-live-transcribe: USD per minute of audio.
const LIVE_USD_PER_MIN: f64 = 0.017;
/// gpt-6-luna, standard short-context, USD per million tokens.
const LUNA_INPUT_USD_PER_M: f64 = 0.10;
const LUNA_CACHED_USD_PER_M: f64 = 0.01;
const LUNA_OUTPUT_USD_PER_M: f64 = 0.50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Transcribe,
    Cleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Charge {
    pub usd_micros: u64,
    pub kind: Kind,
}

fn usd_to_micros(usd: f64) -> u64 {
    (usd * 1_000_000.0).round().max(0.0) as u64
}

fn audio(usd_per_min: f64, duration_ms: u64) -> u64 {
    usd_to_micros(usd_per_min * duration_ms as f64 / 60_000.0)
}

/// File transcription (`gpt-transcribe`, including the live model's HTTP fallback).
pub fn transcribe(duration_ms: u64) -> Charge {
    Charge {
        usd_micros: audio(TRANSCRIBE_USD_PER_MIN, duration_ms),
        kind: Kind::Transcribe,
    }
}

/// Streaming transcription (`gpt-live-transcribe`).
pub fn live(duration_ms: u64) -> Charge {
    Charge {
        usd_micros: audio(LIVE_USD_PER_MIN, duration_ms),
        kind: Kind::Transcribe,
    }
}

/// Dictation cleanup (`gpt-6-luna`). `input` is the API's total input tokens
/// (cached included); `cached` is billed at the cached-input rate.
pub fn luna(input: u64, cached: u64, output: u64) -> Charge {
    let cached = cached.min(input);
    let uncached = input - cached;
    let usd = uncached as f64 * LUNA_INPUT_USD_PER_M / 1_000_000.0
        + cached as f64 * LUNA_CACHED_USD_PER_M / 1_000_000.0
        + output as f64 * LUNA_OUTPUT_USD_PER_M / 1_000_000.0;
    Charge {
        usd_micros: usd_to_micros(usd),
        kind: Kind::Cleanup,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_minute_of_file_transcription() {
        assert_eq!(transcribe(60_000).usd_micros, 4_500);
        assert_eq!(transcribe(60_000).kind, Kind::Transcribe);
    }

    #[test]
    fn one_minute_of_live_transcription() {
        assert_eq!(live(60_000).usd_micros, 17_000);
    }

    #[test]
    fn silence_is_free() {
        assert_eq!(transcribe(0).usd_micros, 0);
        assert_eq!(luna(0, 0, 0).usd_micros, 0);
    }

    #[test]
    fn luna_standard_million() {
        // $0.10 input + $0.50 output
        assert_eq!(luna(1_000_000, 0, 1_000_000).usd_micros, 600_000);
        assert_eq!(luna(1_000_000, 0, 1_000_000).kind, Kind::Cleanup);
    }

    #[test]
    fn luna_cached_input_is_cheaper() {
        assert_eq!(luna(1_000_000, 1_000_000, 0).usd_micros, 10_000);
    }

    #[test]
    fn luna_cached_cannot_exceed_input() {
        assert_eq!(luna(10, 99, 0), luna(10, 10, 0));
    }
}
