/**
 * Typed wrappers around the Tauri IPC bridge.
 * Centralizes every `invoke`/`listen` call so the rest of the UI stays decoupled
 * from the command names exposed by the Rust backend (`src-tauri/src/commands.rs`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import type {
  AppStatus,
  GpuInfo,
  History,
  HistoryEntry,
  InputDevice,
  LocalModel,
  MeetingView,
  ModelDownload,
  Permissions,
  SettingsPatch,
  SettingsView,
  SoundName,
  StartSound,
  StatusEvent,
  WindowMaterial,
} from "./types";

/**
 * Whether the Tauri runtime is present. When running the Vite dev server in a
 * plain browser (UI preview) it isn't, so the IPC helpers below fall back to
 * in-memory defaults instead of throwing.
 */
const IN_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * The browser UI preview can pose as another platform with
 * `?platform=windows` / `?platform=mac`.
 */
const PREVIEW_PLATFORM = IN_TAURI || typeof location === "undefined" ? null : new URLSearchParams(location.search).get("platform");

/** Running on macOS (WKWebView reports "Macintosh"). */
export const IS_MAC = PREVIEW_PLATFORM
  ? PREVIEW_PLATFORM === "mac"
  : typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);
/** Running on Windows (WebView2 reports "Windows NT"). */
export const IS_WINDOWS = PREVIEW_PLATFORM
  ? PREVIEW_PLATFORM === "windows"
  : typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
/**
 * Single-modifier and mouse-button shortcuts (`mod:` / `mouse:` bindings)
 * have a native backend here (`native_input` in the Rust crate).
 */
export const NATIVE_SHORTCUTS = IS_MAC || IS_WINDOWS;

// --- Browser preview mocks ----------------------------------------------------

/** Mock settings used only for the browser UI preview. */
let previewSettings: SettingsView = {
  apiKeyConfigured: true,
  model: "gpt-4o-mini-transcribe",
  language: "",
  shortcut: "mod:MetaRight",
  cancelShortcut: "Escape",
  modeShortcut: "Alt+Shift+KeyK",
  pttShortcut: "",
  mouseShortcut: "",
  meetingShortcut: "",
  meetingConsent: false,
  meetingSource: "mix",
  meetingFolder: "",
  meetingAutosave: true,
  meetingDateFormat: "ymd",
  meetingName: "",
  autoPaste: true,
  theme: "auto",
  pillStyle: "mini",
  pillAlwaysShow: true,
  pillPosition: "top",
  pillSkin: "standard",
  soundsEnabled: true,
  soundVolume: 0.6,
  startSound: "orus",
  vocabulary: ["WhisperAround", "Tauri", "Svelte"],
  modes: [
    { id: "default", name: "Default", icon: "sparkle", model: "gpt-4o-mini-transcribe", language: "" },
    { id: "message", name: "Message", icon: "message", model: "gpt-4o-transcribe", language: "es" },
    { id: "local", name: "Local", icon: "laptop", model: "local:small", language: "" },
  ],
  activeMode: "default",
  inputDevice: "",
  saveHistory: true,
  uiLanguage: new URLSearchParams(location.search).get("lang") ?? "en",
  localDevice: "auto",
};

function previewHistory(): History {
  const now = Date.now();
  const samples = [
    "Recuérdame revisar el pull request de la pill antes de la reunión de mañana.",
    "Hey! I just pushed the new settings window, let me know what you think about the animations.",
    "La idea es que el panel de configuración se sienta nativo, con transiciones suaves y sonidos sutiles.",
    "Send the invoice to Marta and copy finance on the thread.",
    "Necesitamos probar el atajo de push to talk con la tecla comando derecha.",
  ];
  const apps = IS_WINDOWS
    ? ["Slack", "Notepad", "Outlook", "Cursor", "WhatsApp"]
    : ["Slack", "Notes", "Mail", "Cursor", "Messages"];
  const entries: HistoryEntry[] = samples.map((text, i) => ({
    id: now - i * 5_400_000,
    ts: now - i * 5_400_000 - (i > 2 ? 86_400_000 : 0),
    text,
    words: text.split(/\s+/).length,
    durationMs: 3_000 + i * 1_700,
    model: i % 2 ? "gpt-4o-transcribe" : "gpt-4o-mini-transcribe",
    mode: i % 2 ? "Message" : "Default",
    app: apps[i],
    meeting: false,
  }));
  entries.reverse();
  return { entries, stats: { words: 102_865, audioMs: 49_400_000, sessions: 4_210, apps } };
}
let previewHist = previewHistory();
const previewSettingsListeners = new Set<(v: SettingsView) => void>();
const previewHistoryListeners = new Set<() => void>();

/** Browser preview only: a fake pipeline so the pill's states can be seen. */
let previewStatus: AppStatus = "idle";
const previewListeners = new Set<(e: StatusEvent) => void>();
function previewEmit(status: AppStatus) {
  previewStatus = status;
  previewListeners.forEach((fn) => fn({ status }));
  // Fake a speech-like mic level while "recording".
  clearInterval(previewLevelTimer);
  if (status === "recording") {
    previewLevelTimer = setInterval(() => {
      const t = Date.now() / 1000;
      const level = Math.max(0, Math.sin(t * 3) * 0.5 + Math.random() * 0.5);
      previewLevelListeners.forEach((fn) => fn(level));
    }, 50);
  }
}
let previewLevelTimer: ReturnType<typeof setInterval> | undefined;
const previewLevelListeners = new Set<(level: number) => void>();

// --- Pipeline -----------------------------------------------------------------

/** Read the current pipeline status (used to seed the UI on mount). */
export function getStatus(): Promise<AppStatus> {
  if (!IN_TAURI) return Promise.resolve(previewStatus);
  return invoke<AppStatus>("get_status");
}

/** Manually toggle recording (same effect as pressing the global shortcut). */
export function toggleRecording(): Promise<void> {
  if (!IN_TAURI) {
    if (previewStatus === "recording") {
      previewEmit("processing");
      setTimeout(() => previewEmit("idle"), 1500);
    } else if (previewStatus !== "processing") {
      previewEmit("recording");
    }
    return Promise.resolve();
  }
  return invoke("toggle_recording");
}

/** Discard the current recording. */
export function cancelRecording(): Promise<void> {
  if (!IN_TAURI) {
    if (previewStatus === "recording") previewEmit("idle");
    return Promise.resolve();
  }
  return invoke("cancel_recording");
}

/** Subscribe to backend status changes. Returns an unlisten function. */
export function onStatus(handler: (e: StatusEvent) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewListeners.add(handler);
    return Promise.resolve(() => previewListeners.delete(handler));
  }
  return listen<StatusEvent>("status", (event) => handler(event.payload));
}

/** Subscribe to the live mic level (0..1, ~20 Hz) while recording. */
export function onLevel(handler: (level: number) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewLevelListeners.add(handler);
    return Promise.resolve(() => previewLevelListeners.delete(handler));
  }
  return listen<number>("level", (event) => handler(event.payload));
}

// --- Settings -----------------------------------------------------------------

/** Load the current settings (API key returned only as a boolean flag). */
export function loadSettings(): Promise<SettingsView> {
  if (!IN_TAURI) return Promise.resolve(structuredClone(previewSettings));
  return invoke<SettingsView>("load_settings");
}

/** Apply a partial update and persist it. Returns the refreshed view. */
export function updateSettings(patch: SettingsPatch): Promise<SettingsView> {
  if (!IN_TAURI) {
    const { openaiApiKey, ...rest } = patch;
    previewSettings = { ...previewSettings, ...rest };
    if (openaiApiKey?.trim()) previewSettings.apiKeyConfigured = true;
    const active = previewSettings.modes.find((m) => m.id === previewSettings.activeMode) ?? previewSettings.modes[0];
    previewSettings.activeMode = active.id;
    previewSettings.model = active.model;
    previewSettings.language = active.language;
    const view = structuredClone(previewSettings);
    previewSettingsListeners.forEach((fn) => fn(view));
    return Promise.resolve(view);
  }
  return invoke<SettingsView>("update_settings", { patch });
}

/** Settings saved from any window (or a mode switched by shortcut). */
export function onSettingsChanged(handler: (v: SettingsView) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewSettingsListeners.add(handler);
    return Promise.resolve(() => previewSettingsListeners.delete(handler));
  }
  return listen<SettingsView>("settings-changed", (event) => handler(event.payload));
}

/** The "Change mode" shortcut switched modes (payload: new mode name). */
export function onModeChanged(handler: (name: string) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) return Promise.resolve(() => {});
  return listen<string>("mode-changed", (event) => handler(event.payload));
}

/** A message from the backend worth showing briefly (e.g. the GPU fell back to the CPU). */
export function onNotice(handler: (text: string) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) return Promise.resolve(() => {});
  return listen<string>("notice", (event) => handler(event.payload));
}

/** Update only the active mode's transcription model (pill model dropdown). */
export function setModel(model: string): Promise<void> {
  if (!IN_TAURI) {
    const modes = previewSettings.modes.map((m) => (m.id === previewSettings.activeMode ? { ...m, model } : m));
    return updateSettings({ modes }).then(() => {});
  }
  return invoke("set_model", { model });
}

/** Mute every binding while the settings window records a new shortcut. */
export function beginShortcutCapture(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("begin_shortcut_capture");
}

export function endShortcutCapture(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("end_shortcut_capture");
}

/** Available microphones (default first). */
export function listInputDevices(): Promise<InputDevice[]> {
  if (!IN_TAURI)
    return Promise.resolve([
      { name: IS_WINDOWS ? "Microphone Array (Realtek(R) Audio)" : "MacBook Pro Microphone", isDefault: true },
      { name: "AirPods Pro", isDefault: false },
      { name: "Shure MV7", isDefault: false },
    ]);
  return invoke<InputDevice[]>("list_input_devices");
}

/** Audition a sound effect (`startSound` picks the start cue). */
export function previewSound(startSound: StartSound, sound: SoundName, volume: number): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("preview_sound", { startSound, sound, volume });
}

// --- History ------------------------------------------------------------------

export function getHistory(): Promise<History> {
  if (!IN_TAURI) return Promise.resolve(structuredClone(previewHist));
  return invoke<History>("get_history");
}

export function deleteHistoryEntry(id: number): Promise<void> {
  if (!IN_TAURI) {
    previewHist.entries = previewHist.entries.filter((e) => e.id !== id);
    previewHistoryListeners.forEach((fn) => fn());
    return Promise.resolve();
  }
  return invoke("delete_history_entry", { id });
}

export function clearHistory(): Promise<void> {
  if (!IN_TAURI) {
    previewHist.entries = [];
    previewHistoryListeners.forEach((fn) => fn());
    return Promise.resolve();
  }
  return invoke("clear_history");
}

/** A transcription finished or the history was edited. */
export function onHistory(handler: () => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewHistoryListeners.add(handler);
    return Promise.resolve(() => previewHistoryListeners.delete(handler));
  }
  return listen("history", () => handler());
}

// --- Meetings -----------------------------------------------------------------

/** Browser preview only: a fake meeting that "transcribes" a line every few seconds. */
let previewMeeting: MeetingView = { active: false, finishing: false, startedTs: 0, model: "", segments: [] };
let previewMeetingTimer: ReturnType<typeof setInterval> | undefined;
const previewMeetingListeners = new Set<(v: MeetingView) => void>();
function setPreviewMeeting(change: Partial<MeetingView>) {
  previewMeeting = { ...previewMeeting, ...change };
  previewMeetingListeners.forEach((fn) => fn(structuredClone(previewMeeting)));
}

/** The current meeting (live, or the last finished one). */
export function getMeeting(): Promise<MeetingView> {
  if (!IN_TAURI) return Promise.resolve(structuredClone(previewMeeting));
  return invoke<MeetingView>("meeting_state");
}

/** Start recording a meeting (asks for consent first if it was never given). */
export function startMeeting(): Promise<void> {
  if (!IN_TAURI) {
    if (previewMeeting.active) return Promise.resolve();
    if (!previewSettings.meetingConsent) {
      previewConsentListeners.forEach((fn) => fn());
      return Promise.resolve();
    }
    const startedTs = Date.now();
    setPreviewMeeting({ active: true, finishing: false, startedTs, model: previewSettings.model, segments: [] });
    previewMeetingTimer = setInterval(() => {
      const startMs = Date.now() - startedTs;
      const both = IS_WINDOWS && previewSettings.meetingSource === "mix";
      const n = previewMeeting.segments.length;
      const segment = {
        startMs,
        endMs: startMs + 4_000,
        speaker: both ? (n % 2 ? ("system" as const) : ("mic" as const)) : null,
        text: n % 2 ? "Sounds good, let's ship it on Thursday." : "Can everyone see my screen?",
      };
      setPreviewMeeting({ segments: [...previewMeeting.segments, segment] });
    }, 4_000);
    return Promise.resolve();
  }
  return invoke("start_meeting");
}

/** Stop the meeting; the rest is transcribed and saved to the history. */
export function stopMeeting(): Promise<void> {
  if (!IN_TAURI) {
    clearInterval(previewMeetingTimer);
    setPreviewMeeting({ active: false, finishing: false });
    return Promise.resolve();
  }
  return invoke("stop_meeting");
}

/** The folder meeting transcripts are saved to. */
export function getMeetingFolder(): Promise<string> {
  if (!IN_TAURI) return Promise.resolve(previewSettings.meetingFolder || "~/Documents/WhisperAround");
  return invoke<string>("meeting_folder");
}

/** Ask the user for the meetings folder; `null` when the dialog is closed. */
export function pickMeetingFolder(): Promise<string | null> {
  if (!IN_TAURI) return Promise.resolve("~/Notes/Meetings");
  return invoke<string | null>("pick_meeting_folder");
}

/** Open the meetings folder in the file manager. */
export function openMeetingFolder(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("open_meeting_folder");
}

/** Save `content` as Markdown in the meetings folder and open it. Returns the path. */
export function exportMeeting(fileName: string, content: string): Promise<string> {
  if (!IN_TAURI) {
    const url = URL.createObjectURL(new Blob([content], { type: "text/markdown" }));
    Object.assign(document.createElement("a"), { href: url, download: fileName }).click();
    URL.revokeObjectURL(url);
    return Promise.resolve(`~/Documents/WhisperAround/${fileName}`);
  }
  return invoke<string>("export_meeting", { fileName, content });
}

/** The meeting changed: a segment arrived, it started, stopped or finished. */
export function onMeeting(handler: (v: MeetingView) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewMeetingListeners.add(handler);
    return Promise.resolve(() => previewMeetingListeners.delete(handler));
  }
  return listen<MeetingView>("meeting", (event) => handler(event.payload));
}

/** Meeting recording was requested without the user's consent: show the notice. */
export function onMeetingConsent(handler: () => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewConsentListeners.add(handler);
    return Promise.resolve(() => previewConsentListeners.delete(handler));
  }
  return listen("meeting-consent", () => handler());
}
const previewConsentListeners = new Set<() => void>();

// --- Downloadable local models ----------------------------------------------

/** Browser preview only: a fake 1.6 GB download. */
let previewLocal: LocalModel[] = [{ id: "local:turbo", size: 1_613_977_880, downloaded: false, downloading: false }];
let previewDownloadTimer: ReturnType<typeof setInterval> | undefined;
const previewLocalListeners = new Set<() => void>();
const previewDownloadListeners = new Set<(p: ModelDownload) => void>();
function setPreviewLocal(id: string, change: Partial<LocalModel>) {
  previewLocal = previewLocal.map((m) => (m.id === id ? { ...m, ...change } : m));
  previewLocalListeners.forEach((fn) => fn());
}

/** Models downloaded on request that this machine can run, and their state. */
export function localModels(): Promise<LocalModel[]> {
  if (!IN_TAURI) return Promise.resolve(structuredClone(previewLocal));
  return invoke<LocalModel[]>("local_models");
}

/** Install the engine and download a model (progress on `onModelDownload`). */
export function downloadModel(model: string): Promise<void> {
  if (!IN_TAURI) {
    const total = previewLocal.find((m) => m.id === model)?.size ?? 0;
    let received = 0;
    setPreviewLocal(model, { downloading: true });
    previewDownloadListeners.forEach((fn) => fn({ id: model, phase: "engine", received, total }));
    return new Promise((resolve, reject) => {
      setTimeout(() => {
        previewDownloadTimer = setInterval(() => {
          received = Math.min(total, received + total / 60);
          previewDownloadListeners.forEach((fn) => fn({ id: model, phase: "model", received, total }));
          if (received >= total) {
            clearInterval(previewDownloadTimer);
            setPreviewLocal(model, { downloading: false, downloaded: true });
            resolve();
          }
        }, 100);
      }, 1500);
      previewCancel = () => {
        clearInterval(previewDownloadTimer);
        setPreviewLocal(model, { downloading: false });
        reject(DOWNLOAD_CANCELLED);
      };
    });
  }
  return invoke("download_model", { model });
}
let previewCancel = () => {};

/** Error text of a download cancelled on purpose (`downloads::CANCELLED` in Rust). Not user-facing. */
export const DOWNLOAD_CANCELLED = "download-cancelled";

export function cancelModelDownload(): Promise<void> {
  if (!IN_TAURI) {
    previewCancel();
    return Promise.resolve();
  }
  return invoke("cancel_model_download");
}

/** A download started or ended (in any window). */
export function onLocalModelsChanged(handler: () => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewLocalListeners.add(handler);
    return Promise.resolve(() => previewLocalListeners.delete(handler));
  }
  return listen("local-models-changed", () => handler());
}

export function onModelDownload(handler: (p: ModelDownload) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewDownloadListeners.add(handler);
    return Promise.resolve(() => previewDownloadListeners.delete(handler));
  }
  return listen<ModelDownload>("model-download", (event) => handler(event.payload));
}

// --- Platform / app -----------------------------------------------------------

/** Version of the system Python 3 (e.g. "Python 3.12.7"), or null if missing. */
export function checkPython(): Promise<string | null> {
  if (!IN_TAURI) return Promise.resolve(null);
  return invoke<string | null>("check_python");
}

/** NVIDIA GPU detected for local models, and whether its CUDA libraries are installed. */
export function gpuInfo(): Promise<GpuInfo> {
  if (!IN_TAURI) return Promise.resolve({ supported: !IS_MAC, name: IS_MAC ? null : "NVIDIA GeForce RTX 4070", cudaReady: false });
  return invoke<GpuInfo>("gpu_info");
}

/** Open the Python install page (Microsoft Store on Windows, python.org elsewhere). */
export function openPythonInstall(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("open_python_install");
}

/** macOS microphone / Accessibility permissions, or null where none are needed. */
export function checkPermissions(): Promise<Permissions | null> {
  if (!IN_TAURI) return Promise.resolve(IS_MAC ? { microphone: "granted", accessibility: true } : null);
  return invoke<Permissions | null>("check_permissions");
}

/**
 * Native backdrop behind the settings window: macOS vibrancy, Windows 11
 * Mica, or none (the page paints its own opaque background).
 */
export function windowMaterial(): Promise<WindowMaterial> {
  if (!IN_TAURI) return Promise.resolve(IS_MAC ? "vibrancy" : "none");
  return invoke<WindowMaterial>("window_material");
}

/** Show the macOS prompt for a permission, or its System Settings pane. */
export function requestPermission(kind: "microphone" | "accessibility"): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("request_permission", { kind });
}

/** Installed app version (e.g. "0.1.5"). */
export function appVersion(): Promise<string> {
  if (!IN_TAURI) return Promise.resolve("0.2.0");
  return getVersion();
}

/** Version of a newer published release, or null if up to date. */
export function checkUpdate(): Promise<string | null> {
  if (!IN_TAURI) return Promise.resolve(null);
  return invoke<string | null>("check_update");
}

/** Download + install the newer release, then restart the app. */
export function installUpdate(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("install_update");
}

/** Open the release notes on GitHub. */
export function openReleases(): Promise<void> {
  if (!IN_TAURI) {
    window.open("https://github.com/dansaround/whisperaround/releases", "_blank");
    return Promise.resolve();
  }
  return invoke("open_releases");
}

/** Dismiss the first-launch welcome window for good. */
export function finishOnboarding(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("finish_onboarding");
}

// --- Pill window --------------------------------------------------------------

/**
 * Resize the floating window (logical px), keeping its top- or bottom-center
 * fixed. The pill sequences these around its morph animations.
 */
export function fitWindow(width: number, height: number): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("fit_window", { width, height });
}

/** Let clicks pass through the pill window while the pill is hidden. */
export function setPillInteractive(interactive: boolean): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("set_pill_interactive", { interactive });
}

/** The tray asked to reveal the pill (it may be auto-hidden). */
export function onRevealPill(handler: () => void): Promise<UnlistenFn> {
  if (!IN_TAURI) return Promise.resolve(() => {});
  return listen("reveal-pill", () => handler());
}

/** Open the standalone settings window. */
export function openSettings(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("open_settings");
}

/** Start an OS-level drag of the current window (call while the mouse is down). */
export function startDragging(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return getCurrentWindow().startDragging();
}

/** Hide the current window to the tray. */
export function hideWindow(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return getCurrentWindow().hide();
}

/** Label of the current Tauri window ("main" pill, "settings" or "welcome"). */
export function currentWindowLabel(): string {
  // Browser preview: `?window=welcome` previews another window.
  if (!IN_TAURI) return new URLSearchParams(location.search).get("window") ?? "main";
  return getCurrentWindow().label;
}
