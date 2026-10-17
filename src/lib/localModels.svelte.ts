/**
 * Which on-request local models (`ModelOption.download`) this machine can run,
 * which are downloaded, and the progress of the download in flight. One
 * instance per window, kept in sync by backend events.
 */
import { localModels, onLocalModelsChanged, onModelDownload } from "./ipc";
import type { ModelOption } from "./models";
import type { LocalModel, ModelDownload } from "./types";
import { intlLocale } from "./i18n/index.svelte";

class LocalModels {
  list = $state<LocalModel[]>([]);
  progress = $state<ModelDownload | null>(null);
  private started = false;

  init() {
    if (this.started) return;
    this.started = true;
    this.refresh();
    onLocalModelsChanged(() => this.refresh());
    onModelDownload((p) => (this.progress = p));
  }

  async refresh() {
    try {
      this.list = await localModels();
      if (!this.list.some((m) => m.downloading)) this.progress = null;
    } catch (e) {
      console.error("localModels failed", e);
    }
  }

  info(id: string): LocalModel | undefined {
    return this.list.find((m) => m.id === id);
  }

  /** Shown in the Models library: on-request models only where they run. */
  available(m: ModelOption): boolean {
    return !m.download || !!this.info(m.id);
  }

  /** Offered in the pickers: on-request models once downloaded. */
  selectable(m: ModelOption): boolean {
    return !m.download || !!this.info(m.id)?.downloaded;
  }
}

export const local = new LocalModels();

/** 1613977880 → "1.6 GB" (decimal separator and unit follow the UI language). */
export function formatBytes(n: number): string {
  const gb = n >= 1e9;
  return new Intl.NumberFormat(intlLocale(), {
    style: "unit",
    unit: gb ? "gigabyte" : "megabyte",
    minimumFractionDigits: gb ? 1 : 0,
    maximumFractionDigits: gb ? 1 : 0,
  }).format(gb ? n / 1e9 : n / 1e6);
}
