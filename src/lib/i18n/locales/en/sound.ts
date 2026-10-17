/** English strings: sound. Source of truth for every other locale. */
export default {
  effects: {
    title: "Sound effects",
    play: "Play sounds",
    playSub: "A subtle cue when recording starts and stops, and when something goes wrong.",
    volume: "Volume",
    mute: "Mute",
    max: "Max",
  },
  start: {
    title: "Start sound",
    sub: "Plays the moment the pill starts listening.",
    default: "Default",
    blurbs: {
      orus: "Plucky HUD chime",
      loto: "Soft, low-key blip",
      okaso: "Dreamy, peaceful swell",
    },
  },
  preview: {
    title: "Preview",
    play: "Play {name}",
    events: {
      start: "Start recording",
      startSub: "The pill starts listening",
      stop: "Stop recording",
      stopSub: "Recording ends, transcription begins",
      error: "Error",
      errorSub: "Something went wrong",
    },
  },
} as const;
