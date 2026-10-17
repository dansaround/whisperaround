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
}

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
