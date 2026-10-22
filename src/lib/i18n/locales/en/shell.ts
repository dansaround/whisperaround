/** English strings: shell. Source of truth for every other locale. */
export default {
  nav: {
    home: "Home",
    modes: "Modes",
    vocabulary: "Vocabulary",
    configuration: "Configuration",
    sound: "Sound",
    models: "Models library",
    history: "History",
  },
  mic: {
    /** Header microphone picker entry for the system default input. */
    defaultDevice: "{name} (Default)",
    /** Fallback name when the default input device has no name. */
    systemMicrophone: "System microphone",
    /** Header picker entry (Windows): capture what the speakers play. */
    systemAudio: "System audio (speakers)",
    /** Header picker entry (Windows): microphone and system audio together. */
    micAndSystem: "Microphone + system audio",
  },
  update: {
    button: "Update",
    updating: "Updating…",
    /** Tooltip; {version} is e.g. "0.1.7". */
    install: "Install {version}",
  },
  toast: {
    loadFailed: "Couldn't load settings: {error}",
  },
} as const;
