<script lang="ts">
  import { onMount } from "svelte";
  import {
    getStatus,
    onStatus,
    toggleRecording,
    loadSettings,
    setModel,
    setPillMode,
    expandPanel,
    openSettings,
    type PillMode,
  } from "./ipc";
  import type { AppStatus } from "./types";

  // --- Backend-driven state -------------------------------------------------
  let status = $state<AppStatus>("idle");
  let errorMessage = $state<string | null>(null);
  let shortcut = $state("CmdOrCtrl+Shift+X");
  let model = $state("whisper-1");

  // --- Local UI state -------------------------------------------------------
  let expanded = $state(false);
  let hovered = $state(false); // mouse anywhere over the pill
  let hoveredBtn = $state<null | "model" | "record" | "expand">(null);
  let menuOpen = $state(false);

  /** Transcription models offered in the dropdown. Local models = future. */
  const MODELS = [
    { id: "whisper-1", label: "whisper-1" },
    { id: "gpt-4o-transcribe", label: "gpt-4o-transcribe" },
    { id: "gpt-4o-mini-transcribe", label: "gpt-4o-mini-transcribe" },
  ];

  const STATUS_TEXT: Record<AppStatus, string> = {
    idle: "Listo para dictar",
    recording: "Grabando…",
    processing: "Transcribiendo…",
    error: "Algo salió mal",
  };

  const busy = $derived(status === "recording" || status === "processing");
  const showIcons = $derived(!busy && (hovered || menuOpen));
  const modelLabel = $derived(MODELS.find((m) => m.id === model)?.label ?? model);

  // Tooltip only while pointing at a specific icon (not just the pill).
  const tooltip = $derived.by(() => {
    if (busy || menuOpen || !hoveredBtn) return null;
    if (hoveredBtn === "record")
      return status === "error"
        ? { text: errorMessage ?? "Error", keys: [] as string[] }
        : { text: "Start recording", keys: keyParts(shortcut) };
    if (hoveredBtn === "model") return { text: "Modelo", keys: [] };
    return { text: "Expandir", keys: [] };
  });

  /** Resize the window to exactly fit what's currently shown. */
  const mode = $derived.by<PillMode>(() => {
    if (busy) return "rec";
    if (menuOpen) return "menu";
    if (hoveredBtn) return "tip";
    if (hovered) return "icons";
    return "idle";
  });

  $effect(() => {
    if (expanded) return;
    setPillMode(mode);
  });

  /** Pretty-print the configured shortcut into key chips. */
  function keyParts(s: string): string[] {
    return s
      .split("+")
      .map((k) => (k === "CmdOrCtrl" || k === "CommandOrControl" ? "Ctrl" : k));
  }

  onMount(() => {
    getStatus().then((s) => (status = s));
    loadSettings().then((v) => {
      shortcut = v.shortcut;
      model = v.model;
    });

    const unlisten = onStatus((e) => {
      status = e.status;
      errorMessage = e.status === "error" ? e.message ?? "Error desconocido" : null;
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
      window.removeEventListener("keydown", onKey);
    };
  });

  function leavePill() {
    hovered = false;
    hoveredBtn = null;
    menuOpen = false;
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
    leavePill();
    expanded = true;
    expandPanel();
  }

  function collapse() {
    expanded = false; // the $effect resizes back to the compact pill in place
  }
</script>

{#snippet waveform()}
  <span class="waveform" aria-hidden="true">
    {#each Array(5) as _, i (i)}<span style="--i:{i}"></span>{/each}
  </span>
{/snippet}

{#snippet spinner()}
  <span class="spinner" aria-hidden="true"></span>
{/snippet}

{#snippet modelMenu()}
  <div class="menu" role="menu">
    {#each MODELS as m (m.id)}
      <button class="menu-item" class:active={m.id === model} role="menuitemradio" aria-checked={m.id === model} onclick={() => chooseModel(m.id)}>
        <span class="mic" aria-hidden="true">{@render micIcon()}</span>
        <span class="menu-label">{m.label}</span>
        {#if m.id === model}<span class="check" aria-hidden="true">✓</span>{/if}
      </button>
    {/each}
  </div>
{/snippet}

<!-- Inline icons (kept tiny and dependency-free). -->
{#snippet sparkleIcon()}
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor"><path d="M12 2l1.8 5.6a4 4 0 0 0 2.6 2.6L22 12l-5.6 1.8a4 4 0 0 0-2.6 2.6L12 22l-1.8-5.6a4 4 0 0 0-2.6-2.6L2 12l5.6-1.8a4 4 0 0 0 2.6-2.6L12 2z"/></svg>
{/snippet}
{#snippet recordIcon()}
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor"><path d="M12 4c1 0 7 6.5 7 11a7 7 0 0 1-14 0c0-4.5 6-11 7-11z"/></svg>
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

{#if expanded}
  <!-- ===== Expanded panel ===== -->
  <div class="panel {status}" data-tauri-drag-region>
    <button class="icon-btn collapse" title="Colapsar (Esc)" aria-label="Colapsar" onclick={collapse}>
      {@render collapseIcon()}
    </button>

    <div class="panel-body" data-tauri-drag-region>
      <p class="panel-status" data-tauri-drag-region>{status === "error" ? errorMessage ?? STATUS_TEXT.error : STATUS_TEXT[status]}</p>
      {#if status === "recording"}
        <div class="wave-big">{@render waveform()}</div>
      {:else if status === "processing"}
        {@render spinner()}
      {/if}
    </div>

    <div class="panel-bar">
      <div class="model-wrap">
        <button class="model-btn" onclick={() => (menuOpen = !menuOpen)}>
          <span class="mic" aria-hidden="true">{@render micIcon()}</span>
          <span>{modelLabel}</span>
        </button>
        {#if menuOpen}{@render modelMenu()}{/if}
      </div>

      <div class="hints">
        <span class="hint">Record {#each keyParts(shortcut) as k (k)}<kbd>{k}</kbd>{/each}</span>
        <span class="hint">Close <kbd>Esc</kbd></span>
        <button class="icon-btn" title="Ajustes" aria-label="Ajustes" onclick={openSettings}>
          {@render gearIcon()}
        </button>
      </div>
    </div>
  </div>
{:else}
  <!-- ===== Compact pill ===== -->
  <div
    class="pill-wrap"
    role="group"
    onmouseenter={() => (hovered = true)}
    onmouseleave={leavePill}
  >
    {#if busy}
      <button
        class="pill {status}"
        data-tauri-drag-region
        title={status === "recording" ? "Detener" : "Procesando…"}
        aria-label={STATUS_TEXT[status]}
        onclick={status === "recording" ? record : undefined}
      >
        <span class="pill-state">
          {#if status === "recording"}{@render waveform()}{:else}{@render spinner()}{/if}
        </span>
      </button>
    {:else if showIcons}
      <div class="pill {status}" data-tauri-drag-region>
        <button
          class="icon-btn side"
          class:active={menuOpen}
          aria-label="Elegir modelo"
          onmouseenter={() => (hoveredBtn = "model")}
          onmouseleave={() => (hoveredBtn = null)}
          onclick={() => (menuOpen = !menuOpen)}
        >
          {@render sparkleIcon()}
        </button>
        <button
          class="icon-btn record"
          aria-label="Grabar"
          onmouseenter={() => (hoveredBtn = "record")}
          onmouseleave={() => (hoveredBtn = null)}
          onclick={record}
        >
          {@render recordIcon()}
        </button>
        <button
          class="icon-btn side"
          aria-label="Expandir"
          onmouseenter={() => (hoveredBtn = "expand")}
          onmouseleave={() => (hoveredBtn = null)}
          onclick={expand}
        >
          {@render expandIcon()}
        </button>
      </div>
    {:else}
      <!-- Resting state: an empty pill (see reference). -->
      <div class="empty-pill {status}" data-tauri-drag-region></div>
    {/if}

    {#if tooltip}
      <div class="tooltip">
        <span class="tt-text" class:err={status === "error" && hoveredBtn === "record"}>{tooltip.text}</span>
        {#if tooltip.keys.length}
          <span class="tt-keys">{#each tooltip.keys as k (k)}<kbd>{k}</kbd>{/each}</span>
        {/if}
      </div>
    {/if}

    {#if menuOpen}{@render modelMenu()}{/if}
  </div>
{/if}

<style>
  :global(#app) {
    /* Pill UI uses fixed px sizing, independent of the settings window scale. */
    font-size: 13px;
  }

  /* ---------- Compact pill ---------- */
  .pill-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding-top: 4px;
  }

  .empty-pill {
    width: 56px;
    height: 18px;
    border-radius: 999px;
    background: rgba(30, 30, 34, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.1);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    cursor: default;
  }
  .empty-pill.recording {
    border-color: rgba(248, 113, 113, 0.5);
  }
  .empty-pill.error {
    border-color: rgba(248, 113, 113, 0.55);
  }

  .pill {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 5px;
    border-radius: 999px;
    background: rgba(28, 28, 32, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    min-height: 38px;
  }
  button.pill {
    cursor: pointer;
  }

  .pill.recording {
    background: rgba(40, 22, 24, 0.95);
    border-color: rgba(248, 113, 113, 0.45);
  }
  .pill.error {
    border-color: rgba(248, 113, 113, 0.55);
  }

  .pill-state {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 14px;
    min-width: 56px;
    height: 28px;
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
    color: #c9c9d1;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, transform 0.1s;
  }
  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }
  .icon-btn:active {
    transform: scale(0.92);
  }
  .icon-btn.side.active {
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
  }
  .icon-btn.record {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }
  .icon-btn.record:hover {
    background: rgba(255, 255, 255, 0.22);
  }

  /* ---------- Tooltip ---------- */
  .tooltip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    background: rgba(28, 28, 32, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.3);
    white-space: nowrap;
    color: #d8d8df;
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

  /* ---------- Model dropdown ---------- */
  .menu {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    border-radius: 12px;
    background: rgba(28, 28, 32, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    min-width: 200px;
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
    color: #d8d8df;
    cursor: pointer;
    text-align: left;
    font-size: 12px;
  }
  .menu-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }
  .menu-item.active {
    color: #fff;
  }
  .menu-label {
    flex: 1;
  }
  .check {
    color: #8ab4ff;
  }
  .mic {
    display: inline-flex;
    color: #9a9aa2;
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
    background: #f1f1f4;
    transform-origin: center;
    animation: bar 0.9s ease-in-out infinite;
    animation-delay: calc(var(--i) * 0.12s);
  }
  @keyframes bar {
    0%,
    100% {
      transform: scaleY(0.3);
    }
    50% {
      transform: scaleY(1);
    }
  }

  .spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: #f1f1f4;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ---------- Expanded panel ---------- */
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    border-radius: 18px;
    background: rgba(26, 26, 30, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
    overflow: hidden;
  }
  .panel.recording {
    border-color: rgba(248, 113, 113, 0.4);
  }
  .collapse {
    position: absolute;
    top: 10px;
    right: 10px;
  }
  .panel-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
  }
  .panel-status {
    margin: 0;
    font-size: 15px;
    color: #cfcfd6;
  }
  .panel.error .panel-status {
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
    background: rgba(0, 0, 0, 0.25);
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .model-wrap {
    position: relative;
  }
  .model-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    border: none;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    color: #d8d8df;
    cursor: pointer;
    font-size: 12px;
  }
  .model-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }
  .model-wrap .menu {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 0;
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
    color: #9a9aa2;
  }

  kbd {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.12);
    font-family: inherit;
    font-size: 11px;
    color: #e6e6e6;
    line-height: 1.4;
  }
</style>
