<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import Toggle from "../ui/Toggle.svelte";
  import ShortcutRecorder from "../ui/ShortcutRecorder.svelte";
  import Select from "../ui/Select.svelte";
  import { store } from "../store.svelte";
  import { IS_MAC, IS_WINDOWS, NATIVE_SHORTCUTS } from "../../ipc";
  import type { PillPosition, PillSkin, PillStyle, SettingsPatch, Theme } from "../../types";
  import { t, LOCALES, type Key, type Locale } from "../../i18n/index.svelte";

  const v = $derived(store.view!);

  // Labels are computed in $derived so they follow the UI language.
  const THEMES: { id: Theme; label: string }[] = $derived([
    { id: "auto", label: t("configuration.appearance.themes.auto") },
    { id: "light", label: t("configuration.appearance.themes.light") },
    { id: "dark", label: t("configuration.appearance.themes.dark") },
  ]);
  const SKINS: { id: PillSkin; label: string }[] = $derived([
    { id: "standard", label: t("configuration.appearance.skins.standard") },
    { id: "onyx", label: "Onyx" },
  ]);
  const STYLES: { id: PillStyle; label: string }[] = $derived([
    { id: "classic", label: t("configuration.appearance.styles.classic") },
    { id: "mini", label: t("configuration.appearance.styles.mini") },
    { id: "none", label: t("configuration.appearance.styles.none") },
  ]);
  const POSITIONS: { id: PillPosition; label: string }[] = $derived([
    { id: "top", label: t("configuration.appearance.positions.top") },
    { id: "bottom", label: t("configuration.appearance.positions.bottom") },
  ]);
  const languageOptions = LOCALES.map((l) => ({ value: l.code, label: l.label }));

  type ShortcutKey = "shortcut" | "cancelShortcut" | "modeShortcut" | "pttShortcut" | "mouseShortcut";
  const SHORTCUTS: {
    key: ShortcutKey;
    title: Key;
    sub: Key;
    def: string;
    kind?: "keyboard" | "mouse";
    bareEsc?: boolean;
    /** Needs a native backend (`NATIVE_SHORTCUTS`). */
    native?: boolean;
  }[] = [
    { key: "shortcut", title: "configuration.shortcuts.toggle", sub: "configuration.shortcuts.toggleSub", def: "CmdOrCtrl+Shift+X" },
    { key: "cancelShortcut", title: "configuration.shortcuts.cancel", sub: "configuration.shortcuts.cancelSub", def: "Escape", bareEsc: true },
    { key: "modeShortcut", title: "configuration.shortcuts.mode", sub: "configuration.shortcuts.modeSub", def: "Alt+Shift+KeyK" },
    { key: "pttShortcut", title: "configuration.shortcuts.ptt", sub: "configuration.shortcuts.pttSub", def: "" },
    { key: "mouseShortcut", title: "configuration.shortcuts.mouse", sub: "configuration.shortcuts.mouseSub", def: "", kind: "mouse", native: true },
  ];

  function set(patch: SettingsPatch) {
    store.patch(patch);
  }

  /** Tip text split around its {cmd}/{delete} glyph placeholders (rendered bold). */
  const tipParts = $derived(t("configuration.shortcuts.tip").split(/(\{cmd\}|\{delete\})/));

  /** Eight bars for the animated waveform mocks. */
  const BARS = [0.35, 0.7, 1, 0.55, 0.85, 0.45, 0.75, 0.4];
</script>

<div class="page">
  <section class="section">
    <h3 class="section-title">{t("configuration.appearance.title")}</h3>
    <div class="card">
      <div class="row tall">
        <div class="row-text"><strong>{t("configuration.appearance.theme")}</strong></div>
        <div class="thumbs">
          {#each THEMES as th (th.id)}
            <button class="thumb" class:selected={v.theme === th.id} onclick={() => set({ theme: th.id })}>
              <span class="frame wallpaper {th.id}" class:pc={IS_WINDOWS}>
                <span class="win"><i></i><i></i><i></i></span>
              </span>
              {th.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="row tall" class:disabled={v.pillStyle === "none"}>
        <div class="row-text">
          <strong>{t("configuration.appearance.pillLook")}</strong>
          <small>{t("configuration.appearance.pillLookSub")}</small>
        </div>
        <div class="thumbs control">
          {#each SKINS as k (k.id)}
            <button class="thumb" class:selected={v.pillSkin === k.id} onclick={() => set({ pillSkin: k.id })}>
              <span class="frame skin-mock {k.id}">
                {#if k.id === "standard"}
                  <span class="std-pill">
                    <span class="std-x">×</span>
                    <span class="bars">{#each BARS.slice(0, 5) as b, i (i)}<i style="--h:{b};--d:{i * 90}ms"></i>{/each}</span>
                    <span class="std-dot"></span>
                  </span>
                {:else}
                  <span class="onyx-pill">
                    <span class="onyx-bars">{#each [0.55, 0.85, 1, 0.9, 0.75, 0.5] as b, i (i)}<i style="--h:{b};--d:{i * 80}ms"></i>{/each}</span>
                  </span>
                {/if}
              </span>
              {k.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="row tall">
        <div class="row-text"><strong>{t("configuration.appearance.recordingWindow")}</strong></div>
        <div class="thumbs">
          {#each STYLES as s (s.id)}
            <button class="thumb" class:selected={v.pillStyle === s.id} onclick={() => set({ pillStyle: s.id })}>
              <span class="frame rec-mock {s.id}">
                {#if s.id === "classic"}
                  <span class="classic-box">
                    <span class="bars big">{#each BARS as b, i (i)}<i style="--h:{b};--d:{i * 90}ms"></i>{/each}</span>
                    <span class="classic-bar"></span>
                  </span>
                {:else if s.id === "mini"}
                  <span class="mini-pill"><span class="bars">{#each BARS.slice(0, 5) as b, i (i)}<i style="--h:{b};--d:{i * 90}ms"></i>{/each}</span></span>
                {:else}
                  <Icon name="eye-off" size={20} stroke={1.6} />
                {/if}
              </span>
              {s.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="row" class:disabled={v.pillStyle === "none"}>
        <div class="row-text">
          <strong class="help">
            {t("configuration.appearance.alwaysShow")}
            <span class="info" title={t("configuration.appearance.alwaysShowInfo")}>
              <Icon name="info" size={13} />
            </span>
          </strong>
        </div>
        <div class="control">
          <Toggle label={t("configuration.appearance.alwaysShow")} checked={v.pillAlwaysShow} onchange={(c) => set({ pillAlwaysShow: c })} />
        </div>
      </div>

      <div class="row tall" class:disabled={v.pillStyle === "none"}>
        <div class="row-text">
          <strong>{t("configuration.appearance.position")}</strong>
          <small>{t("configuration.appearance.positionSub")}</small>
        </div>
        <div class="thumbs control">
          {#each POSITIONS as p (p.id)}
            <button class="thumb" class:selected={v.pillPosition === p.id} onclick={() => set({ pillPosition: p.id })}>
              <span class="frame screen {p.id}" class:pc={IS_WINDOWS}>
                <span class="menubar"></span>
                <span class="mini-dot"></span>
                <span class="dock"></span>
              </span>
              {p.label}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </section>

  <section class="section">
    <h3 class="section-title">{t("configuration.shortcuts.title")}</h3>
    <div class="card">
      {#each SHORTCUTS.filter((s) => !s.native || NATIVE_SHORTCUTS) as s (s.key)}
        <div class="row">
          <div class="row-text">
            <strong>{t(s.title)}</strong>
            <small>{t(s.sub)}</small>
          </div>
          <div class="control">
            <ShortcutRecorder
              value={v[s.key]}
              defaultValue={s.def}
              kind={s.kind ?? "keyboard"}
              allowBareEscape={s.bareEsc ?? false}
              onchange={(b) => set({ [s.key]: b })}
            />
          </div>
        </div>
      {/each}
    </div>
    {#if NATIVE_SHORTCUTS}
      <p class="section-sub hint">
        {#each tipParts as part, i (i)}{#if part === "{cmd}"}<b>{IS_MAC ? "⌘" : "Ctrl"}</b>{:else if part === "{delete}"}<b>{IS_MAC ? "⌫" : "Backspace"}</b>{:else}{part}{/if}{/each}
      </p>
    {/if}
  </section>

  <section class="section">
    <h3 class="section-title">{t("configuration.general.title")}</h3>
    <div class="card">
      <div class="row">
        <div class="row-text">
          <strong>{t("configuration.general.language")}</strong>
          <small>{t("configuration.general.languageSub")}</small>
        </div>
        <div class="control">
          <Select
            value={v.uiLanguage as Locale}
            options={languageOptions}
            align="right"
            onchange={(code) => store.patch({ uiLanguage: code })}
          />
        </div>
      </div>
      <div class="row">
        <div class="row-text">
          <strong>{t("configuration.general.autoPaste")}</strong>
          <small>{t("configuration.general.autoPasteSub", { keys: IS_MAC ? "⌘V" : "Ctrl+V" })}</small>
        </div>
        <div class="control"><Toggle label={t("configuration.general.autoPaste")} checked={v.autoPaste} onchange={(c) => set({ autoPaste: c })} /></div>
      </div>
      <div class="row">
        <div class="row-text">
          <strong>{t("configuration.general.saveHistory")}</strong>
          <small>{t("configuration.general.saveHistorySub")}</small>
        </div>
        <div class="control"><Toggle label={t("configuration.general.saveHistory")} checked={v.saveHistory} onchange={(c) => set({ saveHistory: c })} /></div>
      </div>
    </div>
  </section>
</div>

<style>
  .hint {
    margin-top: 2px;
  }
  .hint b {
    color: var(--text);
    font-weight: 600;
  }

  /* ---- Theme mocks ---- */
  .wallpaper {
    background:
      radial-gradient(120% 90% at 85% 10%, #a86bff 0%, transparent 55%),
      radial-gradient(120% 120% at 10% 100%, #1d4ed8 0%, transparent 60%),
      linear-gradient(135deg, #1e2a78, #3b2a8c 60%, #6d3fd1);
  }
  .wallpaper.light {
    background:
      radial-gradient(120% 90% at 85% 10%, #f3d7a4 0%, transparent 55%),
      radial-gradient(120% 120% at 10% 100%, #3b82f6 0%, transparent 60%),
      linear-gradient(135deg, #5f8df7, #9cc0ff 60%, #e9d8ff);
  }
  .win {
    position: absolute;
    left: 9px;
    top: 9px;
    right: 0;
    bottom: 0;
    border-top-left-radius: 6px;
    background: #2b2b2f;
    border: 0.5px solid rgba(255, 255, 255, 0.18);
    display: flex;
    align-items: flex-end;
    gap: 3px;
    padding: 0 0 7px 7px;
    transition: background 0.3s var(--ease);
  }
  .light .win {
    background: #f4f4f6;
    border-color: rgba(0, 0, 0, 0.1);
  }
  .auto .win {
    background: linear-gradient(90deg, #f4f4f6 0 50%, #2b2b2f 50% 100%);
  }
  .win::before {
    content: "";
    position: absolute;
    left: 7px;
    right: 7px;
    top: 7px;
    height: 12px;
    border-radius: 3px;
    background: rgba(127, 127, 140, 0.35);
  }
  .win i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #ff5f57;
  }
  .win i:nth-child(2) {
    background: #febc2e;
  }
  .win i:nth-child(3) {
    background: #28c840;
  }
  /* Windows: caption buttons (minimize / maximize / close) top-right. */
  .pc .win {
    align-items: flex-start;
    justify-content: flex-end;
    gap: 5px;
    padding: 3px 6px 0 0;
    color: rgba(255, 255, 255, 0.75);
  }
  .pc.light .win {
    color: rgba(0, 0, 0, 0.6);
  }
  .pc .win::before {
    top: 13px;
  }
  .pc .win i {
    width: 5px;
    height: 5px;
    border-radius: 0;
    background: transparent;
  }
  .pc .win i:nth-child(1) {
    height: 3px;
    border-bottom: 1px solid currentColor;
  }
  .pc .win i:nth-child(2) {
    border: 1px solid currentColor;
    border-radius: 1px;
  }
  .pc .win i:nth-child(3) {
    background:
      linear-gradient(45deg, transparent 43%, currentColor 43% 57%, transparent 57%),
      linear-gradient(-45deg, transparent 43%, currentColor 43% 57%, transparent 57%);
  }
  .pc.auto .win i {
    color: rgba(255, 255, 255, 0.75);
  }

  /* ---- Recording window mocks ---- */
  .rec-mock {
    display: flex;
    align-items: center;
    justify-content: center;
    background: #3a3a3e;
    color: rgba(255, 255, 255, 0.7);
  }
  :global([data-theme="light"]) .rec-mock {
    background: #d9d9de;
    color: rgba(0, 0, 0, 0.5);
  }
  .classic-box {
    position: absolute;
    inset: 7px 9px;
    border-radius: 6px;
    background: #0d0d0f;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .classic-box .bars {
    flex: 1;
  }
  .classic-bar {
    height: 9px;
    background: #2a2a2e;
  }
  .mini-pill {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    height: 16px;
    border-radius: 999px;
    background: #0d0d0f;
    box-shadow: 0 0 0 0.5px rgba(255, 255, 255, 0.15);
  }
  .bars {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    height: 100%;
  }
  .bars i {
    width: 2px;
    height: 9px;
    border-radius: 2px;
    background: #fff;
    transform: scaleY(calc(var(--h) * 0.7));
  }
  .bars.big i {
    width: 2.5px;
    height: 20px;
  }
  .thumb.selected .bars i,
  .thumb:hover .bars i {
    animation: bar 0.9s var(--ease) infinite alternate;
    animation-delay: var(--d);
  }
  @keyframes bar {
    from {
      transform: scaleY(0.25);
    }
    to {
      transform: scaleY(var(--h));
    }
  }

  /* ---- Pill look mocks ---- */
  .skin-mock {
    display: flex;
    align-items: center;
    justify-content: center;
    background:
      radial-gradient(120% 120% at 10% 100%, #1d4ed8 0%, transparent 60%),
      linear-gradient(135deg, #1e2a78, #4b2fa0);
  }
  .std-pill {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 6px 0 4px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--pill-bg) 88%, #f43f5e);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: var(--pill-text);
  }
  .std-pill .bars i {
    background: var(--pill-text);
  }
  .std-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--pill-hover);
    font-size: 9px;
    line-height: 1;
  }
  .std-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #f87171;
  }
  .onyx-pill {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 50px;
    height: 22px;
    border-radius: 999px;
    background: linear-gradient(180deg, #111113, #040405 60%);
    border: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: 0 3px 8px rgba(0, 0, 0, 0.45);
  }
  .onyx-bars {
    display: flex;
    align-items: center;
    gap: 2.5px;
    height: 14px;
  }
  .onyx-bars i {
    width: 2.5px;
    height: 2.5px;
    border-radius: 999px;
    background: #fff;
    transition: height 0.3s var(--ease);
  }
  .thumb.selected .onyx-bars i,
  .thumb:hover .onyx-bars i {
    animation: onyx-bar 0.8s var(--ease) infinite alternate;
    animation-delay: var(--d);
  }
  @keyframes onyx-bar {
    from {
      height: 2.5px;
    }
    to {
      height: calc(2.5px + 11px * var(--h));
    }
  }

  /* ---- Position mocks ---- */
  .screen {
    background:
      radial-gradient(120% 120% at 10% 100%, #1d4ed8 0%, transparent 60%),
      linear-gradient(135deg, #1e2a78, #4b2fa0);
  }
  :global([data-theme="light"]) .screen {
    background:
      radial-gradient(120% 120% at 10% 100%, #3b82f6 0%, transparent 60%),
      linear-gradient(135deg, #6d97f8, #c9b6ff);
  }
  .menubar {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 5px;
    background: rgba(255, 255, 255, 0.28);
  }
  .dock {
    position: absolute;
    left: 24px;
    right: 24px;
    bottom: 3px;
    height: 6px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.3);
  }
  .mini-dot {
    position: absolute;
    left: 50%;
    width: 22px;
    height: 8px;
    margin-left: -11px;
    border-radius: 999px;
    background: #0d0d0f;
    box-shadow: 0 0 0 0.5px rgba(255, 255, 255, 0.3);
    transition: top 0.45s var(--ease);
  }
  /* Windows: no menu bar, a full-width taskbar. */
  .pc .menubar {
    display: none;
  }
  .pc .dock {
    left: 0;
    right: 0;
    bottom: 0;
    height: 7px;
    border-radius: 0;
    background: rgba(255, 255, 255, 0.35);
  }
  .top .mini-dot {
    top: 8px;
  }
  .bottom .mini-dot {
    top: 36px;
  }
  .thumb.selected .mini-dot {
    animation: nudge 1.8s var(--ease) infinite;
  }
  @keyframes nudge {
    50% {
      box-shadow:
        0 0 0 0.5px rgba(255, 255, 255, 0.3),
        0 0 0 3px rgba(59, 130, 246, 0.45);
    }
  }
</style>
