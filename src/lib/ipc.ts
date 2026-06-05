/**
 * Typed wrappers around the Tauri IPC bridge.
 * Centralizes every `invoke`/`listen` call so the rest of the UI stays decoupled
 * from the command names exposed by the Rust backend (`src-tauri/src/commands.rs`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppStatus, SettingsView, SettingsUpdate, StatusEvent } from "./types";

/**
 * Whether the Tauri runtime is present. When running the Vite dev server in a
 * plain browser (UI preview) it isn't, so the IPC helpers below fall back to
 * in-memory defaults instead of throwing.
 */
const IN_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Mock settings used only for the browser UI preview. */
const previewSettings: SettingsView = {
  apiKeyConfigured: false,
  model: "whisper-1",
  shortcut: "CmdOrCtrl+Shift+X",
  autoPaste: true,
};

/** Read the current pipeline status (used to seed the UI on mount). */
export function getStatus(): Promise<AppStatus> {
  if (!IN_TAURI) return Promise.resolve("idle");
  return invoke<AppStatus>("get_status");
}

/** Manually toggle recording (same effect as pressing the global shortcut). */
export function toggleRecording(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("toggle_recording");
}

/** Load the current settings (API key returned only as a boolean flag). */
export function loadSettings(): Promise<SettingsView> {
  if (!IN_TAURI) return Promise.resolve(previewSettings);
  return invoke<SettingsView>("load_settings");
}

/** Persist settings. Returns the refreshed view. */
export function saveSettings(update: SettingsUpdate): Promise<SettingsView> {
  if (!IN_TAURI) {
    return Promise.resolve({
      apiKeyConfigured: update.openaiApiKey.trim().length > 0 || previewSettings.apiKeyConfigured,
      model: update.model,
      shortcut: update.shortcut,
      autoPaste: update.autoPaste,
    });
  }
  return invoke<SettingsView>("save_settings", { update });
}

/** Subscribe to backend status changes. Returns an unlisten function. */
export function onStatus(handler: (e: StatusEvent) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) return Promise.resolve(() => {});
  return listen<StatusEvent>("status", (event) => handler(event.payload));
}
