/** English strings: meetings. Source of truth for every other locale. */
export default {
  title: "Meetings",
  start: "Start meeting",
  stop: "Stop meeting",
  finishing: "Finishing…",
  idle: "Not recording",
  recording: "Recording a meeting",
  /** {model} is a model name, e.g. "gpt-4o-mini-transcribe". */
  model: "Model: {model}",
  /** Windows, input is not "Microphone + system audio". */
  hintSuggestMix: "Only your microphone is recorded. To capture both sides of a call and label who spoke, choose “Microphone + system audio” as the source below.",
  /** Windows, "Microphone + system audio" is selected. */
  hintBoth: "Recording your microphone and the system audio. Your lines are labeled “Me”, everyone else “Others”.",
  /** macOS and other systems. */
  hintMicOnly: "Only the microphone is recorded.",
  /** Windows, the source is "System audio only". */
  hintSystem: "Recording only the system audio: what the others in the call say.",
  me: "Me",
  others: "Others",
  transcript: "Transcript",
  emptyIdle: "The transcript appears here while the meeting is recorded, and is saved to your history when you stop.",
  emptyRecording: "Listening… the first lines show up after a few seconds of speech.",
  copyAll: "Copy all",
  copied: "Copied",
  copyFailed: "Couldn't copy to the clipboard",
  exportMd: "Export .md",
  /** {path} is the saved file's full path. */
  exported: "Saved to {path}",
  source: {
    title: "Source",
    sub: "What a meeting records. Dictation keeps the microphone chosen at the top.",
    mix: "Microphone + system audio",
    system: "System audio only",
    mic: "Microphone only",
  },
  /** The meetings folder; its path is shown below the title. */
  folder: {
    title: "Save to",
    change: "Change…",
    open: "Open",
    /** Go back to Documents/WhisperAround. */
    reset: "Default",
  },
  autosave: {
    title: "Save automatically",
    sub: "Save the transcript as .md in this folder when the meeting ends.",
  },
  consent: {
    title: "Before you record",
    body: "Let everyone in the call know you're recording and transcribing it. Recording people without their consent may be illegal where you live.",
    accept: "I understand, start",
    cancel: "Cancel",
  },
  /** Markdown export. */
  md: {
    /** Used in the file name and the heading. */
    name: "Meeting",
    /** {time} is e.g. "12:34". */
    duration: "Duration: {time}",
    model: "Model: {model}",
  },
} as const;
