/**
 * Shared types mirroring the Rust backend payloads.
 * Keep these in sync with `src-tauri/src/state.rs` and `src-tauri/src/settings.rs`.
 */

/** Lifecycle states of the dictation pipeline. Serialized lowercase from Rust. */
export type AppStatus = "idle" | "recording" | "processing" | "error";

/** Payload emitted by the backend on the `status` event. */
export interface StatusEvent {
  status: AppStatus;
  /** Present only when `status === "error"`. */
  message?: string | null;
}

/**
 * Settings as exposed to the frontend. The OpenAI API key is NEVER sent back in
 * clear text — the backend only reports whether one is configured via
 * `apiKeyConfigured`. Sending a non-empty `openaiApiKey` on save updates it;
 * sending an empty string leaves the stored key untouched.
 */
export interface SettingsView {
  apiKeyConfigured: boolean;
  model: string;
  shortcut: string;
  autoPaste: boolean;
}

/** Payload sent from the frontend when saving settings. */
export interface SettingsUpdate {
  /** Empty string = keep the existing key. */
  openaiApiKey: string;
  model: string;
  shortcut: string;
  autoPaste: boolean;
}
