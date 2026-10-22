/**
 * Shared types mirroring the Rust backend payloads.
 * Keep these in sync with `src-tauri/src/state.rs`, `settings.rs`, `history.rs` and `downloads.rs`.
 */

/** Lifecycle states of the dictation pipeline. Serialized lowercase from Rust. */
export type AppStatus = "idle" | "recording" | "processing" | "error";

/** Payload emitted by the backend on the `status` event. */
export interface StatusEvent {
  status: AppStatus;
  /** Present only when `status === "error"`. */
  message?: string | null;
}

export type Theme = "auto" | "light" | "dark";
export type PillStyle = "classic" | "mini" | "none";
export type PillPosition = "top" | "bottom";
/** Pill look: the themed translucent one, or the solid black "Onyx". */
export type PillSkin = "standard" | "onyx";
export type SoundName = "start" | "stop" | "error";
/** Start cue played when the pill starts listening. */
export type StartSound = "orus" | "loto" | "okaso";

/** Where local faster-whisper models run. */
export type LocalDevice = "auto" | "cpu" | "cuda";

/** NVIDIA GPU state (`gpu_info` command). */
export interface GpuInfo {
  /** False where CUDA is never offered (macOS). */
  supported: boolean;
  /** First NVIDIA GPU found, or null. */
  name: string | null;
  /** The CUDA libraries are already installed. */
  cudaReady: boolean;
}

/** A transcription preset. */
export interface Mode {
  id: string;
  name: string;
  icon: string;
  model: string;
  /** ISO-639-1 code, or "" for auto-detect. */
  language: string;
}

/**
 * Settings as exposed to the frontend. The OpenAI API key is NEVER sent back in
 * clear text — the backend only reports whether one is configured via
 * `apiKeyConfigured`.
 *
 * Shortcut bindings: an accelerator (`"Super+Shift+KeyX"`), a lone modifier
 * (`"mod:MetaRight"`, macOS), a mouse button (`"mouse:3"`, macOS) or `""`.
 */
export interface SettingsView {
  apiKeyConfigured: boolean;
  /** Effective model (the active mode's). */
  model: string;
  language: string;
  shortcut: string;
  cancelShortcut: string;
  modeShortcut: string;
  pttShortcut: string;
  mouseShortcut: string;
  meetingShortcut: string;
  /** The recording-consent notice was acknowledged. */
  meetingConsent: boolean;
  /** What a meeting records; the microphone is the dictation one. */
  meetingSource: MeetingSource;
  /** Folder for the transcripts; empty = `<Documents>/WhisperAround`. */
  meetingFolder: string;
  /** Save the transcript there as Markdown when a meeting ends. */
  meetingAutosave: boolean;
  autoPaste: boolean;
  theme: Theme;
  pillStyle: PillStyle;
  pillAlwaysShow: boolean;
  pillPosition: PillPosition;
  pillSkin: PillSkin;
  soundsEnabled: boolean;
  soundVolume: number;
  startSound: StartSound;
  vocabulary: string[];
  modes: Mode[];
  activeMode: string;
  inputDevice: string;
  saveHistory: boolean;
  /** Interface language (see `Locale` in `i18n/index.svelte.ts`). */
  uiLanguage: string;
  localDevice: LocalDevice;
}

/** `mix` = microphone + system audio (Windows only, like `system`). */
export type MeetingSource = "mix" | "system" | "mic";

/** Partial update; `openaiApiKey` blank = keep the stored key. */
export type SettingsPatch = Partial<Omit<SettingsView, "apiKeyConfigured" | "model" | "language">> & {
  openaiApiKey?: string;
};

export interface HistoryEntry {
  id: number;
  ts: number;
  text: string;
  words: number;
  durationMs: number;
  model: string;
  mode: string;
  app: string | null;
  /** A meeting transcript: multi-line text with `[hh:mm:ss] Me: …` lines. */
  meeting: boolean;
}

/** One transcribed stretch of a meeting. `speaker` is only set when mic and system audio were recorded separately. */
export interface MeetingSegment {
  startMs: number;
  endMs: number;
  speaker: "mic" | "system" | null;
  text: string;
}

/** The meeting being recorded, or the last finished one (`meeting_state`, `meeting` event). */
export interface MeetingView {
  active: boolean;
  /** Recording is over; the last chunks are being transcribed. */
  finishing: boolean;
  /** Unix time in ms. */
  startedTs: number;
  model: string;
  segments: MeetingSegment[];
}

export interface HistoryStats {
  words: number;
  audioMs: number;
  sessions: number;
  apps: string[];
}

export interface History {
  /** Oldest first. */
  entries: HistoryEntry[];
  stats: HistoryStats;
}

export interface InputDevice {
  name: string;
  isDefault: boolean;
}

/** macOS privacy permissions (`check_permissions`); null on other platforms. */
export interface Permissions {
  microphone: "undetermined" | "denied" | "granted";
  accessibility: boolean;
}

/** A local model downloaded on request (`local_models`). */
export interface LocalModel {
  id: string;
  /** Download size in bytes. */
  size: number;
  downloaded: boolean;
  downloading: boolean;
}

/** `model-download` progress: installing the engine, then the weights. */
export interface ModelDownload {
  id: string;
  phase: "engine" | "model";
  received: number;
  total: number;
}

/** Native backdrop behind the settings window (`window_material`). */
export type WindowMaterial = "vibrancy" | "mica" | "none";
