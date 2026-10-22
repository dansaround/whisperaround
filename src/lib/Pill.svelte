<script lang="ts">
  import { onMount } from "svelte";
  import type { TransitionConfig } from "svelte/transition";
  import {
    getStatus,
    onStatus,
    onLevel,
    toggleRecording,
    cancelRecording,
    loadSettings,
    onSettingsChanged,
    onModeChanged,
    onNotice,
    onRevealPill,
    setModel,
    fitWindow,
    setPillInteractive,
    openSettings,
    startDragging,
  } from "./ipc";
  import { MODELS } from "./models";
  import { local } from "./localModels.svelte";
  import { chips } from "./shortcutFormat";
  import { t, td } from "./i18n/index.svelte";
  import type { AppStatus, PillPosition, PillSkin, PillStyle, SettingsView } from "./types";

  // --- Backend-driven state -------------------------------------------------
  let status = $state<AppStatus>("idle");
  let errorMessage = $state<string | null>(null);
  /** The last error arrived without a message (shown as `pill.unknownError`). */
  let errorUnknown = $state(false);
  let shortcut = $state("CmdOrCtrl+Shift+X");
  let cancelShortcut = $state("Escape");
  let model = $state("gpt-4o-mini-transcribe");
  let pillStyle = $state<PillStyle>("mini");
  let alwaysShow = $state(true);
  let position = $state<PillPosition>("top");
  let skin = $state<PillSkin>("standard");

  // --- Local UI state -------------------------------------------------------
  let expanded = $state(false);
  let hovered = $state(false); // mouse anywhere over the window
  /** Button whose text tooltip is shown (only after a sustained hover). */
  let tooltipBtn = $state<null | "model" | "record" | "expand" | "cancel">(null);
  let menuOpen = $state(false);
  /** Mode name flashed after the "Change mode" shortcut. */
  let modeToast = $state<string | null>(null);
  /** Backend notice (e.g. GPU fallback) flashed in the same toast. */
  let noticeToast = $state<string | null>(null);
  /** Tray "show pill" while auto-hidden: stay visible for a moment. */
  let revealed = $state(false);
  /** A fresh error stays visible for a while even with "always show" off. */
  let errorFresh = $state(false);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let revealTimer: ReturnType<typeof setTimeout> | undefined;
  let errorTimer: ReturnType<typeof setTimeout> | undefined;

  // Live mic level → waveform. Each bar shows one recent level sample; newest
  // in the middle, older ones spreading outward, so speech ripples from the
  // center.
  /** Bar order: index into `levels` (0 = newest) for each bar left→right. */
  const BAR_AGE = [2, 1, 0, 1, 2];
  /** Onyx: six bars, each with its own reach so speech looks organic. */
  const ONYX_BARS = [
    { age: 2, w: 0.55 },
    { age: 1, w: 0.85 },
    { age: 0, w: 1 },
    { age: 0, w: 0.9 },
    { age: 1, w: 0.75 },
    { age: 2, w: 0.5 },
  ];
  const HISTORY = 3;
  let levels = $state<number[]>(Array(HISTORY).fill(0));
  let lastShift = 0;

  // Show the icon row immediately on hover, but only reveal the *text* tooltip
  // after a sustained hover, and delay collapsing back to the empty pill so a
  // grow/shrink loop at the window edge can't make it flicker.
  const TOOLTIP_DELAY = 2000;
  const LEAVE_DELAY = 280;
  /** Length of the shell morph; the window only shrinks once it's done. */
  const MORPH_MS = 520;
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let tipTimer: ReturnType<typeof setTimeout> | undefined;
  let shrinkTimer: ReturnType<typeof setTimeout> | undefined;

  type Size = { w: number; h: number };

  /**
   * Scale of the compact pill (idle / icons / recording / transcribing). The
   * shapes below are designed at full size and shrunk by this factor; their
   * content is scaled to match. Tooltips, menus and the expanded panel keep
   * their full size so text stays readable.
   */
  const Z = 0.6;
  /** The hover icon row is scaled less, so its icons stay legible. */
  const ICONS_Z = 1.05;
  /** The resting dots, likewise. */
  const IDLE_Z = 0.85;

  // Window sizes (logical px). Every non-idle compact mode shares one width so
  // switching between them never moves the window sideways. `IDLE` must match
  // `PILL_SIZE` in `src-tauri/src/window.rs`.
  const IDLE: Size = { w: 52, h: 34 };
  const COMPACT_W = 240;
  /** Extra vertical padding on the compact pill (content keeps its size). */
  const PAD_Y = 11;
  const PAD_Y_IDLE = 4;
  /** The hover icon row is a little taller than recording / transcribing. */
  const PAD_Y_ICONS = 16;
  /**
   * Gap between the compact pill and the window edge it hangs from. The
   * window is transparent but still clips, so this (and the room left on the
   * other sides) must fit the pill's drop shadow, or its cut-off edges draw a
   * faint box around the pill.
   */
  const INSET = 7;
  /** Window height that fits the compact pill (and its shadow) alone. */
  const COMPACT_H = 58;
  /** Where tooltips / the compact menu start (below the pill), CSS px. */
  const POP_OFFSET = 56;
  const PANEL: Size = { w: 500, h: 158 };
  const MENU_H = 216;

  /** The morphing shell's shape for each visual state (CSS px). */
  type Visual = "idle" | "icons" | "rec" | "proc" | "panel";
  type Shape = { w: number; h: number; r: number; top: number };
  const SHELL: Record<Visual, Shape> = {
    idle: { w: 50, h: 22, r: 11, top: 4 },
    icons: { w: 110, h: 40, r: 20, top: 4 },
    rec: { w: 104, h: 40, r: 20, top: 4 },
    proc: { w: 56, h: 40, r: 20, top: 4 },
    panel: { w: PANEL.w, h: PANEL.h, r: 16, top: 0 },
  };
  /** Onyx is a touch larger and rounder: a solid black capsule. */
  const ONYX_SHELL: Record<Visual, Shape> = {
    idle: { w: 58, h: 24, r: 12, top: 4 },
    icons: { w: 116, h: 44, r: 22, top: 4 },
    rec: { w: 110, h: 44, r: 22, top: 4 },
    proc: { w: 124, h: 44, r: 22, top: 4 },
    panel: { w: PANEL.w, h: PANEL.h, r: 18, top: 0 },
  };

  /** Status text, looked up on every render so it follows the UI language. */
  const statusText = (s: AppStatus) => td(`pill.status.${s}`);

  /** Error text, resolved on render so the fallback follows the UI language. */
  const errorText = $derived(errorUnknown ? t("pill.unknownError") : errorMessage);

  const busy = $derived(status === "recording" || status === "processing");
  const showIcons = $derived(!busy && (hovered || menuOpen));
  const modelLabel = $derived(MODELS.find((m) => m.id === model)?.label ?? model);
  const bottom = $derived(position === "bottom");
  const onyx = $derived(skin === "onyx");
  /** Classic style: the big panel takes over while recording / transcribing. */
  const autoPanel = $derived(pillStyle === "classic" && busy);
  /** Auto-hidden: "always show" off and nothing worth showing. */
  const hidden = $derived(
    !alwaysShow && !busy && !expanded && !menuOpen && !revealed && !modeToast && !noticeToast && !(status === "error" && errorFresh),
  );

  // Hidden pill: let clicks fall through to whatever is underneath.
  $effect(() => {
    setPillInteractive(!hidden);
  });

  const visual = $derived.by<Visual>(() => {
    if (expanded || autoPanel) return "panel";
    if (status === "recording") return "rec";
    if (status === "processing") return "proc";
    return showIcons ? "icons" : "idle";
  });
  const shell = $derived.by<Shape>(() => {
    const s = (onyx ? ONYX_SHELL : SHELL)[visual];
    if (visual === "panel") return s;
    // Taller than the scaled design, and still a full capsule.
    const h = s.h * Z + (visual === "idle" ? PAD_Y_IDLE : visual === "icons" ? PAD_Y_ICONS : PAD_Y);
    return { w: s.w * (visual === "icons" ? ICONS_Z : Z), h, r: h / 2, top: INSET };
  });

  // Text tooltip shown only after a sustained hover on a specific icon.
  const tooltip = $derived.by(() => {
    if (busy || menuOpen || !tooltipBtn) return null;
    if (tooltipBtn === "record")
      return status === "error"
        ? { text: errorText ?? t("pill.tooltip.error"), keys: [] as string[] }
        : { text: t("pill.tooltip.startRecording"), keys: keyParts(shortcut) };
    if (tooltipBtn === "cancel") return { text: t("pill.tooltip.cancel"), keys: keyParts(cancelShortcut) };
    if (tooltipBtn === "model") return { text: t("pill.tooltip.model"), keys: [] };
    return { text: t("pill.tooltip.expand"), keys: [] };
  });

  /** Window size that fits everything currently shown. */
  const target = $derived.by<Size>(() => {
    if (expanded || autoPanel) return { w: PANEL.w, h: menuOpen ? PANEL.h + 8 + MENU_H + 16 : PANEL.h };
    const tipH = POP_OFFSET + 44;
    if (modeToast || noticeToast) return { w: COMPACT_W, h: tipH };
    if (hidden) return IDLE;
    if (busy) return { w: COMPACT_W, h: tooltipBtn === "cancel" ? tipH : COMPACT_H };
    if (menuOpen) return { w: COMPACT_W, h: POP_OFFSET + MENU_H + 16 };
    if (tooltipBtn) return { w: COMPACT_W, h: tipH };
    if (hovered) return { w: COMPACT_W, h: COMPACT_H };
    return IDLE;
  });

  // Grow the window *before* the shell morphs (so nothing gets clipped) and
  // shrink it only *after* the morph settles (so the shape can animate down
  // instead of being cut off). Mixed changes grow to the union first.
  let applied: Size = { ...IDLE };
  function applyWindow(s: Size) {
    applied = s;
    fitWindow(s.w, s.h);
  }
  $effect(() => {
    const t = target;
    clearTimeout(shrinkTimer);
    const grown = { w: Math.max(t.w, applied.w), h: Math.max(t.h, applied.h) };
    if (grown.w !== applied.w || grown.h !== applied.h) applyWindow(grown);
    if (grown.w !== t.w || grown.h !== t.h) shrinkTimer = setTimeout(() => applyWindow(t), MORPH_MS);
  });

  /** Pretty-print a binding into key chips (⌘ᴿ, ⌥, K…). */
  function keyParts(s: string): string[] {
    return chips(s).map((c) => c.label + (c.side ? (c.side === "R" ? "ᴿ" : "ᴸ") : ""));
  }

  function applySettings(v: SettingsView) {
    shortcut = v.shortcut;
    cancelShortcut = v.cancelShortcut;
    model = v.model;
    pillStyle = v.pillStyle;
    alwaysShow = v.pillAlwaysShow;
    position = v.pillPosition;
    skin = v.pillSkin;
  }

  onMount(() => {
    local.init();
    getStatus()
      .then((s) => (status = s))
      .catch((e) => console.error("getStatus failed", e));
    loadSettings()
      .then(applySettings)
      .catch((e) => console.error("loadSettings failed", e));
    const unlistenSettings = onSettingsChanged(applySettings);
    const unlistenMode = onModeChanged((name) => {
      modeToast = name;
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => (modeToast = null), 1600);
    });
    const unlistenNotice = onNotice((text) => {
      noticeToast = text;
      clearTimeout(noticeTimer);
      noticeTimer = setTimeout(() => (noticeToast = null), 4000);
    });
    const unlistenReveal = onRevealPill(() => {
      revealed = true;
      clearTimeout(revealTimer);
      revealTimer = setTimeout(() => (revealed = false), 4000);
    });

    const unlisten = onStatus((e) => {
      status = e.status;
      errorMessage = e.status === "error" ? e.message ?? null : null;
      errorUnknown = e.status === "error" && e.message == null;
      if (e.status !== "recording") levels = Array(HISTORY).fill(0);
      if (e.status !== "recording" && tooltipBtn === "cancel") tooltipBtn = null;
      clearTimeout(errorTimer);
      errorFresh = e.status === "error";
      if (errorFresh) errorTimer = setTimeout(() => (errorFresh = false), 6000);
    });
    const unlistenLevel = onLevel((l) => {
      // Rise instantly, decay softly so syllables don't flicker.
      // Levels arrive at ~60 Hz: the newest bar follows every sample, while
      // the history only steps every ~50 ms so the ripple keeps its pace.
      const now = performance.now();
      const next = Math.max(l, levels[0] * 0.86);
      if (now - lastShift >= 48) {
        lastShift = now;
        levels = [next, ...levels.slice(0, HISTORY - 1)];
      } else {
        levels = [next, ...levels.slice(1)];
      }
    });

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (menuOpen) menuOpen = false;
        else if (expanded) collapse();
      }
    };
    window.addEventListener("keydown", onKey);

    return () => {
      unlisten.then((fn) => fn());
      unlistenLevel.then((fn) => fn());
      unlistenSettings.then((fn) => fn());
      unlistenMode.then((fn) => fn());
      unlistenNotice.then((fn) => fn());
      unlistenReveal.then((fn) => fn());
      clearTimeout(toastTimer);
      clearTimeout(revealTimer);
      clearTimeout(errorTimer);
      window.removeEventListener("keydown", onKey);
      clearTimeout(leaveTimer);
      clearTimeout(tipTimer);
      clearTimeout(shrinkTimer);
    };
  });

  // Drag the window from anywhere on the pill that isn't a control. The drag
  // only starts once the pointer moves a few px, so a plain click (e.g. on the
  // recording pill to stop) still registers as a click.
  const DRAG_THRESHOLD = 4;
  let dragFrom: { x: number; y: number } | null = null;
  /** Set when a drag starts so its release isn't taken as a click; any new
   *  press clears it (the OS may or may not deliver that release). */
  let suppressClick = false;

  function pressStage(e: MouseEvent) {
    suppressClick = false;
    const target = e.target as Element;
    if (e.button !== 0 || !target.closest(".shell, .tooltip")) return;
    if (target.closest("button, input, select, [role=menu]")) return;
    dragFrom = { x: e.screenX, y: e.screenY };
  }

  function moveStage(e: MouseEvent) {
    enterPill();
    if (!dragFrom) return;
    if (!(e.buttons & 1)) {
      dragFrom = null;
      return;
    }
    if (Math.hypot(e.screenX - dragFrom.x, e.screenY - dragFrom.y) < DRAG_THRESHOLD) return;
    dragFrom = null;
    suppressClick = true;
    startDragging();
  }

  /** Click on the recording pill stops it, unless that click ended a drag. */
  function clickShell() {
    if (suppressClick) suppressClick = false;
    else if (visual === "rec") record();
  }

  /** Also bound to mousemove: after collapsing the panel the cursor may already
   *  be inside the window, where no new mouseenter would ever fire. */
  function enterPill() {
    if (expanded) return;
    clearTimeout(leaveTimer);
    hovered = true;
  }

  /** Delay the collapse so the window's grow/shrink can't oscillate at its edge. */
  function leavePill() {
    clearTimeout(tipTimer);
    tooltipBtn = null;
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => {
      hovered = false;
      if (!expanded) menuOpen = false;
    }, LEAVE_DELAY);
  }

  function enterBtn(which: "model" | "record" | "expand" | "cancel") {
    clearTimeout(tipTimer);
    tipTimer = setTimeout(() => (tooltipBtn = which), TOOLTIP_DELAY);
  }

  function leaveBtn() {
    clearTimeout(tipTimer);
    tooltipBtn = null;
  }

  function record() {
    menuOpen = false;
    toggleRecording();
  }

  function chooseModel(id: string) {
    model = id;
    menuOpen = false;
    setModel(id);
  }

  function expand() {
    clearTimeout(leaveTimer);
    clearTimeout(tipTimer);
    tooltipBtn = null;
    menuOpen = false;
    hovered = false;
    expanded = true;
  }

  function collapse() {
    menuOpen = false;
    expanded = false; // the shell morphs back, then the window shrinks
  }

  /** Popover entrance: unfold from the pill with a soft blur (Apple-like). */
  function pop(_node: Element, { duration = 260 } = {}): TransitionConfig {
    const dir = bottom ? 1 : -1;
    return {
      duration,
      easing: (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2),
      css: (t, u) =>
        `opacity:${t};transform:translateY(${6 * dir * u}px) scale(${0.94 + 0.06 * t});filter:blur(${4 * u}px)`,
    };
  }

  function cancel(e: MouseEvent) {
    // Don't let the click reach the shell, which stops (and transcribes).
    e.stopPropagation();
    tooltipBtn = null;
    cancelRecording();
  }
</script>

{#snippet waveform()}
  {#if onyx}
    <!-- Dots at rest, rounded bars that stretch with the voice. -->
    <span class="waveform onyx-wave" aria-hidden="true">
      {#each ONYX_BARS as b, i (i)}<span style="--l:{Math.min(1, levels[b.age] * b.w * 1.25)}"></span>{/each}
    </span>
  {:else}
    <span class="waveform" aria-hidden="true">
      {#each BAR_AGE as age, i (i)}<span style="--l:{levels[age] * (1 - age * 0.18)}"></span>{/each}
    </span>
  {/if}
{/snippet}

{#snippet spinner()}
  {#if onyx}
    <span class="onyx-wave thinking" aria-hidden="true">{#each ONYX_BARS as _, i (i)}<span style="--i:{i}"></span>{/each}</span>
  {:else}
    <span class="spinner" aria-hidden="true"></span>
  {/if}
{/snippet}

{#snippet modelMenu(placement: "compact" | "panel")}
  <div class="menu {placement}" role="menu" transition:pop>
    {#each [false, true] as isLocal (isLocal)}
      <div class="menu-group">{isLocal ? t("pill.menu.local") : "OpenAI"}</div>
      {#each MODELS.filter((m) => m.local === isLocal && local.selectable(m)) as m (m.id)}
        <button class="menu-item" class:active={m.id === model} role="menuitemradio" aria-checked={m.id === model} onclick={() => chooseModel(m.id)}>
          <span class="mic" aria-hidden="true">{#if m.local}{@render chipIcon()}{:else}{@render micIcon()}{/if}</span>
          <span class="menu-label">{m.label}</span>
          {#if m.id === model}<span class="check" aria-hidden="true">✓</span>{/if}
        </button>
      {/each}
    {/each}
  </div>
{/snippet}

<!-- Inline icons (kept tiny and dependency-free). -->
{#snippet sparkleIcon()}
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor"><path d="M12 2l1.8 5.6a4 4 0 0 0 2.6 2.6L22 12l-5.6 1.8a4 4 0 0 0-2.6 2.6L12 22l-1.8-5.6a4 4 0 0 0-2.6-2.6L2 12l5.6-1.8a4 4 0 0 0 2.6-2.6L12 2z"/></svg>
{/snippet}
{#snippet recordIcon()}
  <svg viewBox="0 0 24 24" width="16" height="16"><rect x="8.5" y="2" width="7" height="12.5" rx="3.5" fill="currentColor"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3.5M8.5 21.5h7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
{/snippet}
{#snippet expandIcon()}
  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H3v-6M21 9V3h-6M3 21l7-7M21 3l-7 7"/></svg>
{/snippet}
{#snippet collapseIcon()}
  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10h6V4M20 14h-6v6M14 10l7-7M10 14l-7 7"/></svg>
{/snippet}
{#snippet gearIcon()}
  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
{/snippet}
{#snippet micIcon()}
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4"/></svg>
{/snippet}
{#snippet chipIcon()}
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/></svg>
{/snippet}

<!-- Mouse handlers here only implement window dragging; every control inside is a real button. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="stage"
  class:bottom
  class:onyx
  style="--z:{Z};--pop:{POP_OFFSET}px"
  role="group"
  onmouseenter={enterPill}
  onmousemove={moveStage}
  onmousedown={pressStage}
  onmouseup={() => (dragFrom = null)}
  onmouseleave={leavePill}
>
  <!--
    One persistent shell for every state. Its size/radius morph on a spring
    while each state's content cross-fades in its own layer, so changes read
    as the same object transforming instead of elements being swapped.
  -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="shell {status} {visual}"
    class:hidden
    style="width:{shell.w}px;height:{shell.h}px;{bottom ? 'bottom' : 'top'}:{shell.top}px;border-radius:{shell.r}px"
    title={visual === "rec" ? t("pill.aria.stop") : undefined}
    aria-label={busy ? statusText(status) : undefined}
    onclick={clickShell}
  >
    <!-- Onyx at rest: a quiet row of dots in the black nub. -->
    {#if onyx}
      <div class="layer idle-dots" class:on={visual === "idle"} aria-hidden="true" style="--z:{IDLE_Z}"><i></i><i></i><i></i></div>
    {/if}

    <!-- Hover: model · record · expand -->
    <div class="layer" class:on={visual === "icons"} inert={visual !== "icons"} style="--z:{ICONS_Z}">
      <button
        class="icon-btn side"
        class:active={menuOpen}
        aria-label={t("pill.aria.chooseModel")}
        onmouseenter={() => enterBtn("model")}
        onmouseleave={leaveBtn}
        onclick={() => (menuOpen = !menuOpen)}
      >
        {@render sparkleIcon()}
      </button>
      <button
        class="icon-btn record"
        aria-label={t("pill.aria.record")}
        onmouseenter={() => enterBtn("record")}
        onmouseleave={leaveBtn}
        onclick={(e) => {
          // Don't let the click reach the shell, which becomes "stop" once recording.
          e.stopPropagation();
          record();
        }}
      >
        {@render recordIcon()}
      </button>
      <button
        class="icon-btn side"
        aria-label={t("pill.aria.expand")}
        onmouseenter={() => enterBtn("expand")}
        onmouseleave={leaveBtn}
        onclick={expand}
      >
        {@render expandIcon()}
      </button>
    </div>

    <!-- Recording: cancel · waveform · live dot (click anywhere else to stop) -->
    <div class="layer rec-layer" class:on={visual === "rec"} inert={visual !== "rec"}>
      <button
        class="icon-btn cancel"
        aria-label={t("pill.aria.cancelRecording")}
        onmouseenter={() => enterBtn("cancel")}
        onmouseleave={leaveBtn}
        onclick={cancel}
      >
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
      {@render waveform()}
      <span class="rec-dot" aria-hidden="true"></span>
    </div>

    <!-- Processing -->
    <div class="layer" class:on={visual === "proc"}>
      {@render spinner()}
    </div>

    <!-- Expanded panel -->
    <div class="layer panel-layer" class:on={visual === "panel"} inert={visual !== "panel"}>
      <button class="icon-btn collapse" class:gone={!expanded} title={t("pill.aria.collapseEsc")} aria-label={t("pill.aria.collapse")} onclick={collapse}>
        {@render collapseIcon()}
      </button>

      <div class="panel-body">
        <div class="pb-layer" class:on={status === "idle"}>
          <p class="panel-status">{t("pill.status.idle")}</p>
        </div>
        <div class="pb-layer" class:on={status === "recording"}>
          <p class="panel-status">{t("pill.status.recording")}</p>
          <div class="wave-big">{@render waveform()}</div>
        </div>
        <div class="pb-layer" class:on={status === "processing"}>
          <p class="panel-status">{t("pill.status.processing")}</p>
          {@render spinner()}
        </div>
        <div class="pb-layer" class:on={status === "error"}>
          <p class="panel-status err">{errorText ?? t("pill.status.error")}</p>
        </div>
      </div>

      <div class="panel-bar">
        <button class="model-btn" class:active={menuOpen} onclick={() => (menuOpen = !menuOpen)}>
          <span class="mic" aria-hidden="true">{@render micIcon()}</span>
          <span>{modelLabel}</span>
        </button>

        <div class="hints">
          <button class="hint rec-hint" onclick={record}>
            {status === "recording" ? t("pill.hint.stop") : t("pill.hint.record")}
            {#each keyParts(shortcut) as k (k)}<kbd>{k}</kbd>{/each}
          </button>
          {#if status === "recording"}
            <button class="hint rec-hint" onclick={(e) => cancel(e)}>
              {t("pill.hint.cancel")} {#each keyParts(cancelShortcut) as k (k)}<kbd>{k}</kbd>{/each}
            </button>
          {:else}
            <span class="hint">{t("pill.hint.close")} <kbd>Esc</kbd></span>
          {/if}
          <button class="icon-btn" title={t("pill.aria.settings")} aria-label={t("pill.aria.settings")} onclick={openSettings}>
            {@render gearIcon()}
          </button>
        </div>
      </div>
    </div>
  </div>

  {#if noticeToast && !expanded && !autoPanel}
    <div class="tooltip mode-toast notice-toast" transition:pop={{ duration: 220 }}>
      <span class="mode-dot warn" aria-hidden="true"></span>
      <span class="tt-text">{noticeToast}</span>
    </div>
  {:else if modeToast && !expanded && !autoPanel}
    <div class="tooltip mode-toast" transition:pop={{ duration: 220 }}>
      <span class="mode-dot" aria-hidden="true"></span>
      <span class="tt-text">{modeToast}</span>
    </div>
  {:else if tooltip && !expanded && !autoPanel}
    <div class="tooltip" transition:pop={{ duration: 200 }}>
      <span class="tt-text" class:err={status === "error" && tooltipBtn === "record"}>{tooltip.text}</span>
      {#if tooltip.keys.length}
        <span class="tt-keys">{#each tooltip.keys as k (k)}<kbd>{k}</kbd>{/each}</span>
      {/if}
    </div>
  {/if}

  {#if menuOpen}{@render modelMenu(expanded || autoPanel ? "panel" : "compact")}{/if}
</div>

<style>
  :global(#app) {
    /* Pill UI uses fixed px sizing, independent of the settings window scale. */
    font-size: 13px;
  }

  .stage {
    /* Critically-damped-ish spring (~4% overshoot), with a bezier fallback. */
    --spring: cubic-bezier(0.32, 0.72, 0, 1);
    --morph: 520ms;
    position: relative;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
  }
  @supports (transition-timing-function: linear(0, 1)) {
    .stage {
      --spring: linear(0, 0.0402, 0.1381, 0.2657, 0.4029, 0.536, 0.6565, 0.7599, 0.8445, 0.9106, 0.9599, 0.9946, 1.0174, 1.0308, 1.0371, 1.0384, 1.0363, 1.0323, 1.0272, 1.022, 1.0169, 1.0124, 1.0086, 1.0055, 1.003, 1.0013, 1, 0.9992, 0.9988, 0.9986, 0.9985, 0.9986, 0.9988, 0.999, 0.9992, 0.9994, 1);
    }
  }

  /* ---------- Morphing shell ---------- */
  .shell {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    overflow: hidden;
    color: var(--pill-text);
    background: var(--pill-bg);
    border: 1px solid var(--pill-line);
    /* Small enough to fit inside the window (see INSET), so it fades out
       smoothly instead of being clipped into a visible box. */
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.25);
    backdrop-filter: blur(18px) saturate(1.4);
    -webkit-backdrop-filter: blur(18px) saturate(1.4);
    transition:
      width var(--morph) var(--spring),
      height var(--morph) var(--spring),
      top var(--morph) var(--spring),
      bottom var(--morph) var(--spring),
      border-radius var(--morph) var(--spring),
      transform 0.42s cubic-bezier(0.65, 0, 0.35, 1),
      opacity 0.34s cubic-bezier(0.65, 0, 0.35, 1),
      filter 0.34s cubic-bezier(0.65, 0, 0.35, 1),
      background-color 0.35s ease,
      border-color 0.35s ease,
      box-shadow 0.35s ease;
    will-change: width, height, transform, opacity;
    contain: layout style;
  }
  /* Auto-hidden ("always show" off): melt away instead of popping out. */
  .shell.hidden {
    transform: translateX(-50%) scale(0.5);
    opacity: 0;
    filter: blur(6px);
    pointer-events: none;
  }
  .shell {
    cursor: grab;
  }
  .shell.rec {
    cursor: pointer;
  }
  .shell.recording {
    background: color-mix(in srgb, var(--pill-bg) 88%, #f43f5e);
    border-color: rgba(248, 113, 113, 0.45);
  }
  .shell.error {
    border-color: rgba(248, 113, 113, 0.55);
  }
  .shell.panel {
    background: var(--pill-panel);
    border-color: var(--pill-line);
    box-shadow: var(--pill-shadow);
  }
  .shell.panel.recording {
    border-color: rgba(248, 113, 113, 0.4);
  }

  /*
   * Content layers: stacked and centered on the shell. Leaving content fades
   * out fast; entering content follows the shape with a small delay, scaling
   * up from a soft blur — the "same object transforming" feel.
   */
  .layer {
    will-change: transform, opacity, filter;
    position: absolute;
    left: 50%;
    top: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    opacity: 0;
    transform: translate(-50%, -50%) scale(calc(var(--z) * 0.6));
    filter: blur(6px);
    pointer-events: none;
    transition:
      opacity 0.14s ease,
      transform 0.3s ease,
      filter 0.2s ease;
  }
  .layer.on {
    opacity: 1;
    transform: translate(-50%, -50%) scale(var(--z));
    filter: blur(0);
    pointer-events: auto;
    transition:
      opacity 0.26s ease 0.07s,
      transform var(--morph) var(--spring) 0.03s,
      filter 0.3s ease 0.07s;
  }

  .rec-dot {
    width: 8px;
    height: 8px;
    margin-right: 10px;
    border-radius: 50%;
    background: #f87171;
    box-shadow: 0 0 0 0 rgba(248, 113, 113, 0.6);
    animation: pulse 1.4s ease-out infinite;
  }
  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(248, 113, 113, 0.55);
    }
    100% {
      box-shadow: 0 0 0 7px rgba(248, 113, 113, 0);
    }
  }

  /* ---------- Buttons ---------- */
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--pill-text-2);
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s,
      transform 0.25s var(--spring);
  }
  .icon-btn:hover {
    background: var(--pill-hover);
    color: var(--pill-text);
  }
  .icon-btn:active {
    transform: scale(0.9);
  }
  .icon-btn.side.active {
    background: var(--pill-hover);
    color: var(--pill-text);
  }
  .icon-btn.record {
    background: var(--pill-hover);
    color: var(--pill-text);
  }
  .icon-btn.record:hover {
    background: color-mix(in srgb, var(--pill-hover) 100%, var(--pill-text) 12%);
  }
  .rec-layer {
    gap: 9px;
  }
  .icon-btn.cancel {
    width: 22px;
    height: 22px;
    background: var(--pill-hover);
    color: var(--pill-text-2);
  }
  .icon-btn.cancel:hover {
    background: rgba(248, 113, 113, 0.9);
    color: #fff;
  }
  .rec-layer .rec-dot {
    margin: 0 0 0 1px;
  }

  /* ---------- Tooltip / menu (below the compact pill) ---------- */
  .tooltip,
  .menu.compact {
    position: absolute;
    top: var(--pop);
    left: 50%;
    translate: -50% 0;
    transform-origin: top center;
  }
  .bottom .tooltip,
  .bottom .menu.compact {
    top: auto;
    bottom: var(--pop);
    transform-origin: bottom center;
  }
  .tooltip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--pill-panel);
    border: 1px solid var(--pill-line);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.25);
    white-space: nowrap;
    color: var(--pill-text);
  }
  .mode-toast {
    padding: 6px 12px 6px 10px;
    border-radius: 999px;
  }
  .mode-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #7ee36b;
    box-shadow: 0 0 8px rgba(126, 227, 107, 0.8);
  }
  .mode-dot.warn {
    background: #f59e0b;
    box-shadow: 0 0 8px rgba(245, 158, 11, 0.8);
  }
  .notice-toast {
    max-width: 280px;
    white-space: normal;
    border-radius: 12px;
  }
  .tt-text {
    font-weight: 500;
  }
  .tt-text.err {
    color: #f87171;
  }
  .tt-keys {
    display: inline-flex;
    gap: 3px;
  }

  .menu {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    border-radius: 12px;
    background: var(--pill-panel);
    border: 1px solid var(--pill-line);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.35);
    min-width: 214px;
  }
  /* Expanded: drop below the panel, aligned with the model button. */
  .menu.panel {
    position: absolute;
    top: 166px;
    left: 12px;
    transform-origin: top left;
  }
  .bottom .menu.panel {
    top: auto;
    bottom: 166px;
    transform-origin: bottom left;
  }
  .menu-group {
    padding: 6px 9px 3px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--pill-text-2);
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 9px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--pill-text);
    cursor: pointer;
    text-align: left;
    font-size: 12px;
    transition: background 0.12s;
  }
  .menu-item:hover {
    background: var(--pill-hover);
  }
  .menu-item.active {
    font-weight: 600;
  }
  .menu-label {
    flex: 1;
  }
  .check {
    color: #8ab4ff;
  }
  .mic {
    display: inline-flex;
    color: var(--pill-text-2);
  }

  /* ---------- Waveform / spinner ---------- */
  .waveform {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 18px;
  }
  .waveform span {
    width: 3px;
    height: 100%;
    border-radius: 2px;
    background: var(--pill-text);
    transform-origin: center;
    /* Silence keeps a small dot-like bar; speech stretches it to full height. */
    transform: scaleY(calc(0.17 + 0.83 * var(--l, 0)));
    transition: transform 60ms linear;
    will-change: transform;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--pill-text) 20%, transparent);
    border-top-color: var(--pill-text);
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ---------- Expanded panel ---------- */
  .panel-layer {
    /* Fixed panel-sized content, pinned to the shell's top so it is revealed
       by the growing shape rather than squeezed by it. */
    top: 0;
    width: 500px;
    height: 158px;
    flex-direction: column;
    align-items: stretch;
    justify-content: flex-start;
    gap: 0;
    transform: translate(-50%, 0) scale(0.9);
    transform-origin: top center;
  }
  .panel-layer.on {
    transform: translate(-50%, 0) scale(1);
  }
  /* At the bottom of the screen the panel grows upward: pin to the bottom. */
  .bottom .panel-layer {
    top: auto;
    bottom: 0;
    transform-origin: bottom center;
  }
  .collapse {
    position: absolute;
    top: 10px;
    right: 10px;
    z-index: 1;
  }
  .collapse.gone {
    visibility: hidden;
  }
  .panel-body {
    position: relative;
    flex: 1;
  }
  .pb-layer {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    opacity: 0;
    transform: scale(0.94);
    filter: blur(4px);
    transition:
      opacity 0.14s ease,
      transform 0.3s ease,
      filter 0.2s ease;
  }
  .pb-layer.on {
    opacity: 1;
    transform: none;
    filter: none;
    transition:
      opacity 0.26s ease 0.06s,
      transform var(--morph) var(--spring),
      filter 0.3s ease 0.06s;
  }
  .panel-status {
    margin: 0;
    font-size: 15px;
    color: var(--pill-text);
    opacity: 0.85;
  }
  .panel-status.err {
    color: #f87171;
    max-width: 80%;
    text-align: center;
  }
  .wave-big {
    transform: scale(1.6);
  }

  .panel-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--pill-text) 5%, transparent);
    border-top: 1px solid var(--pill-line);
  }
  .model-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    border: none;
    border-radius: 999px;
    background: var(--pill-hover);
    color: var(--pill-text);
    cursor: pointer;
    font-size: 12px;
    transition: background 0.15s;
  }
  .model-btn:hover,
  .model-btn.active {
    background: color-mix(in srgb, var(--pill-hover) 100%, var(--pill-text) 10%);
  }
  .hints {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--pill-text-2);
  }
  .rec-hint {
    padding: 4px 6px;
    margin: -4px -6px;
    border: none;
    border-radius: 8px;
    background: transparent;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .rec-hint:hover {
    background: var(--pill-hover);
    color: var(--pill-text);
  }

  kbd {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--pill-hover);
    border: 1px solid var(--pill-line);
    font-family: inherit;
    font-size: 11px;
    color: var(--pill-text);
    line-height: 1.4;
  }

  /* ---------- Onyx skin: solid black capsule, dots that become bars ---------- */
  .stage.onyx {
    --pill-bg: #050506;
    --pill-line: rgba(255, 255, 255, 0.17);
    --pill-text: #ffffff;
    --pill-text-2: rgba(255, 255, 255, 0.62);
    --pill-hover: rgba(255, 255, 255, 0.1);
    --pill-panel: rgba(9, 9, 11, 0.97);
    --pill-shadow: 0 10px 28px rgba(0, 0, 0, 0.55);
  }
  .onyx .shell {
    background: linear-gradient(180deg, #111113 0%, #040405 60%);
    border-width: 1.5px;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.07),
      inset 0 -1px 0 rgba(0, 0, 0, 0.6),
      0 1px 6px rgba(0, 0, 0, 0.4);
    backdrop-filter: none;
    -webkit-backdrop-filter: none;
  }
  .onyx .shell.recording {
    background: linear-gradient(180deg, #111113 0%, #040405 60%);
    border-color: var(--pill-line);
  }
  .onyx .shell.panel {
    background: var(--pill-panel);
  }
  .onyx .shell.error {
    border-color: rgba(248, 113, 113, 0.6);
  }

  .onyx-wave {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 26px;
  }
  .onyx-wave span {
    width: 4px;
    height: calc(4px + 20px * var(--l, 0));
    border-radius: 999px;
    background: #fff;
    transform: none;
    transition: height 70ms linear;
  }
  /* Compact pill: speech stretches the bars nearly edge to edge. */
  .rec-layer .onyx-wave {
    height: 52px;
  }
  .rec-layer .onyx-wave span {
    height: calc(4px + 46px * var(--l, 0));
  }
  /* Transcribing: the dots ripple. */
  .onyx-wave.thinking span {
    height: 4px;
    animation: onyx-think 1.1s cubic-bezier(0.65, 0, 0.35, 1) infinite;
    animation-delay: calc(var(--i) * 0.11s);
  }
  @keyframes onyx-think {
    0%,
    60%,
    100% {
      height: 4px;
      opacity: 0.55;
    }
    30% {
      height: 12px;
      opacity: 1;
    }
  }

  /* Recording: just the waveform; the cancel button slides in on hover. */
  .onyx .rec-layer {
    gap: 0;
  }
  .onyx .rec-layer .rec-dot {
    display: none;
  }
  .onyx .icon-btn.cancel {
    width: 0;
    margin-right: 0;
    opacity: 0;
    overflow: hidden;
    transition:
      width 0.32s cubic-bezier(0.65, 0, 0.35, 1),
      margin 0.32s cubic-bezier(0.65, 0, 0.35, 1),
      opacity 0.22s ease,
      background 0.15s;
  }
  .onyx .shell:hover .icon-btn.cancel {
    width: 22px;
    margin-right: 10px;
    opacity: 1;
  }

  .idle-dots {
    gap: 4px;
  }
  .idle-dots i {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.5);
  }

  @media (prefers-reduced-motion: reduce) {
    .stage {
      --morph: 1ms;
    }
  }
</style>
