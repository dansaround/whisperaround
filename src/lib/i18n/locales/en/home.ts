/** English strings: home. Source of truth for every other locale. */
export default {
  range: {
    all: "All time",
    week: "Last 7 days",
    today: "Today",
  },
  /** Label under the "time saved" stat, one per range. */
  savedIn: {
    all: "Saved all time",
    week: "Saved last 7 days",
    today: "Saved today",
  },
  copyStats: "Copy stats",
  statsCopied: "Stats copied to the clipboard",
  copyFailed: "Couldn't copy the stats",
  /** Clipboard text from the share button. {range} is a range label, {words} is the "words" plural below. */
  shareText: "WhisperAround · {range}: {wpm} WPM, {words}, {saved} saved.",
  shareWords: { one: "{count} word", other: "{count} words" },
  stats: {
    /** Unit shown after the animated words-per-minute number. */
    wpmUnit: "WPM",
    averageSpeed: "Average speed",
    words: "Words",
    appsUsed: "Apps used",
    savedTooltip: "Compared with typing at {wpm} WPM",
  },
  /** Time-saved durations. */
  time: {
    minutes: { one: "{count} min", other: "{count} min" },
    hours: { one: "{count} h", other: "{count} h" },
    days: { one: "{count} day", other: "{count} days" },
  },
  getStarted: "Get started",
  steps: {
    record: { title: "Start recording", sub: "Turn your voice to text with a single click." },
    shortcuts: { title: "Customize your shortcuts", sub: "Change the keyboard shortcuts for WhisperAround." },
    mode: { title: "Create a mode", sub: "Build the perfect mode for your workflow." },
    vocabulary: { title: "Add vocabulary", sub: "Teach WhisperAround custom words, names, or industry terms." },
  },
  whatsNew: "What's new?",
  viewAllChanges: "View all changes",
  changes: {
    settingsWindow: {
      title: "A brand-new settings window",
      body: "Sidebar navigation, themes, a configurable pill, single-key and push-to-talk shortcuts, and futuristic sound packs.",
    },
    modesVocabulary: {
      title: "Modes & vocabulary",
      /** {keys} is a keyboard shortcut glyph sequence such as ⌥⇧K. */
      body: "Switch model and language presets with {keys}, and teach Whisper the names you use.",
    },
  },
} as const;
