/** English strings shared across windows. Source of truth for every other locale. */
export default {
  autoDetect: "Auto-detect",
  modelDesc: {
    "gpt-transcribe": "OpenAI's current transcription model. Accurate with names and jargon, and uses your vocabulary as keywords.",
    "gpt-live-transcribe": "Streams your voice while you speak, so the text is ready the moment you stop. Costs about 4x more.",
    "local:small": "Runs on your computer — no internet, no API key. Light and quick.",
    "local:medium": "Local and private, more accurate at the cost of more memory.",
    "local:turbo": "Whisper large-v3 turbo, accelerated on Apple silicon. Top accuracy, fully offline.",
  },
} as const;
