/**
 * State shared by the settings pages: the settings view (auto-saved field by
 * field), history, devices, permissions and app version. One instance per
 * settings window.
 */
import {
  appVersion,
  checkPermissions,
  checkUpdate,
  getHistory,
  listInputDevices,
  loadSettings,
  onHistory,
  onSettingsChanged,
  updateSettings,
} from "../ipc";
import { local } from "../localModels.svelte";
import { applyTheme } from "../theme";
import { setLocale } from "../i18n/index.svelte";
import type { History, InputDevice, Permissions, SettingsPatch, SettingsView } from "../types";

export type Page = "home" | "modes" | "vocabulary" | "configuration" | "sound" | "models" | "history";

export interface Toast {
  id: number;
  text: string;
  kind: "error" | "ok";
}

class SettingsStore {
  view = $state<SettingsView | null>(null);
  history = $state<History>({ entries: [], stats: { words: 0, audioMs: 0, sessions: 0, apps: [] } });
  devices = $state<InputDevice[]>([]);
  perms = $state<Permissions | null>(null);
  version = $state("");
  update = $state<string | null>(null);
  page = $state<Page>("home");
  toast = $state<Toast | null>(null);
  private toastTimer: ReturnType<typeof setTimeout> | undefined;

  async init() {
    const [view] = await Promise.all([
      loadSettings(),
      this.refreshHistory(),
      this.refreshDevices(),
      this.refreshPerms(),
    ]);
    this.setView(view);
    appVersion().then((v) => (this.version = v));
    checkUpdate()
      .then((v) => (this.update = v))
      .catch((e) => console.error("checkUpdate failed", e));
    onSettingsChanged((v) => this.setView(v));
    onHistory(() => this.refreshHistory());
    local.init();
  }

  private setView(v: SettingsView) {
    this.view = v;
    applyTheme(v.theme);
    setLocale(v.uiLanguage);
  }

  async refreshHistory() {
    try {
      this.history = await getHistory();
    } catch (e) {
      console.error("getHistory failed", e);
    }
  }

  async refreshDevices() {
    try {
      this.devices = await listInputDevices();
    } catch (e) {
      console.error("listInputDevices failed", e);
    }
  }

  async refreshPerms() {
    try {
      this.perms = await checkPermissions();
    } catch (e) {
      console.error("checkPermissions failed", e);
    }
  }

  /** Optimistically apply `patch`, persist it, roll back + toast on failure. */
  async patch(patch: SettingsPatch): Promise<boolean> {
    const before = this.view;
    if (before) {
      const { openaiApiKey: _key, ...rest } = patch;
      this.setView({ ...before, ...rest });
    }
    try {
      this.setView(await updateSettings(patch));
      return true;
    } catch (e) {
      if (before) this.setView(before);
      this.notify(String(e), "error");
      return false;
    }
  }

  notify(text: string, kind: Toast["kind"] = "ok") {
    clearTimeout(this.toastTimer);
    this.toast = { id: Date.now(), text, kind };
    this.toastTimer = setTimeout(() => (this.toast = null), kind === "error" ? 5200 : 2400);
  }
}

export const store = new SettingsStore();
