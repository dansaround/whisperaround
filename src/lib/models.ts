import { t, td } from "./i18n/index.svelte";

/**
 * Transcription models offered in the pill dropdown and the settings window.
 * `local:*` ids run on-device with faster-whisper (see `src-tauri/src/local.rs`)
 * or, once downloaded, with MLX on Apple Silicon (`src-tauri/src/downloads.rs`);
 * the rest go to the OpenAI API.
 */
export interface ModelOption {
  id: string;
  label: string;
  local: boolean;
  /** 1–5 ratings for the library cards. */
  speed: number;
  accuracy: number;
  /** Downloaded on request from the Models page (and only where it runs). */
  download?: boolean;
}

export const MODELS: ModelOption[] = [
  {
    id: "gpt-4o-mini-transcribe",
    label: "gpt-4o-mini-transcribe",
    local: false,
    speed: 5,
    accuracy: 4,
  },
  {
    id: "gpt-4o-transcribe",
    label: "gpt-4o-transcribe",
    local: false,
    speed: 4,
    accuracy: 5,
  },
  {
    id: "whisper-1",
    label: "whisper-1",
    local: false,
    speed: 4,
    accuracy: 3,
  },
  {
    id: "local:small",
    label: "faster-whisper small",
    local: true,
    speed: 4,
    accuracy: 3,
  },
  {
    id: "local:medium",
    label: "faster-whisper medium",
    local: true,
    speed: 2,
    accuracy: 4,
  },
  {
    id: "local:turbo",
    label: "whisper large-v3 turbo",
    local: true,
    speed: 5,
    accuracy: 5,
    download: true,
  },
];

/** One-line pitch for the Models library, in the UI language. */
export function modelDescription(id: string): string {
  return td(`common.modelDesc.${id}`);
}

export function modelLabel(id: string): string {
  return MODELS.find((m) => m.id === id)?.label ?? id;
}

export function isLocal(id: string): boolean {
  return id.startsWith("local:");
}

/** Transcription languages ("" = auto-detect); call it so the label follows the UI language. */
export function transcriptionLanguages(): { code: string; label: string }[] {
  return [{ code: "", label: t("common.autoDetect") }, ...LANGUAGES];
}

/** Transcription languages, named in their own language. */
const LANGUAGES: { code: string; label: string }[] = [
  { code: "es", label: "Español" },
  { code: "en", label: "English" },
  { code: "pt", label: "Português" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "it", label: "Italiano" },
  { code: "ca", label: "Català" },
  { code: "nl", label: "Nederlands" },
  { code: "ja", label: "日本語" },
  { code: "ko", label: "한국어" },
  { code: "zh", label: "中文" },
];
