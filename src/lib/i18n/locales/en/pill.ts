/** English strings: pill. Source of truth for every other locale. */
export default {
  /** Spoken/visible status of the pill. */
  status: {
    idle: "Ready to dictate",
    recording: "Recording…",
    processing: "Transcribing…",
    error: "Something went wrong",
  },
  /** Badge while a meeting is being recorded; {time} is e.g. "12:34". */
  meeting: "Meeting {time}",
  /** Fallback when the backend reports an error without a message. */
  unknownError: "Unknown error",
  /** Text tooltips shown after a sustained hover on a compact-pill icon. */
  tooltip: {
    error: "Error",
    startRecording: "Start recording",
    cancel: "Cancel",
    model: "Model",
    expand: "Expand",
  },
  /** Accessible labels / native titles of the pill's controls. */
  aria: {
    chooseModel: "Choose model",
    record: "Record",
    expand: "Expand",
    cancelRecording: "Cancel recording",
    stop: "Stop",
    collapse: "Collapse",
    collapseEsc: "Collapse (Esc)",
    settings: "Settings",
  },
  /** Keyboard hints in the expanded panel's bottom bar (followed by key chips). */
  hint: {
    record: "Record",
    stop: "Stop",
    cancel: "Cancel",
    close: "Close",
  },
  /** Model menu group headings ("OpenAI" is a brand and stays untranslated). */
  menu: {
    local: "Local",
  },
} as const;
