<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { cubicInOut } from "svelte/easing";
  import Icon from "./Icon.svelte";
  import Keys from "./Keys.svelte";
  import { beginShortcutCapture, endShortcutCapture, NATIVE_SHORTCUTS, requestPermission } from "../../ipc";
  import {
    acceleratorFromEvent,
    chips,
    isModifierCode,
    mouseBinding,
    normalizeModifierCode,
  } from "../../shortcutFormat";
  import { store } from "../store.svelte";
  import { t } from "../../i18n/index.svelte";

  let {
    value,
    defaultValue,
    kind = "keyboard",
    allowBareEscape = false,
    allowModifierOnly = true,
    onchange,
  }: {
    value: string;
    defaultValue: string;
    kind?: "keyboard" | "mouse";
    allowBareEscape?: boolean;
    allowModifierOnly?: boolean;
    onchange: (binding: string) => void;
  } = $props();

  let recording = $state(false);
  /** Modifier codes currently held (live preview). */
  let held = $state<string[]>([]);
  let error = $state<string | null>(null);
  let shake = $state(0);
  let sawKey = false;
  let root: HTMLDivElement;

  const needsAx = $derived(value.startsWith("mod:") && store.perms !== null && !store.perms.accessibility);
  const heldPreview = $derived(
    held.length ? held.map((c) => chips(`mod:${c}`)[0]).filter(Boolean) : [],
  );

  async function start() {
    if (recording) return;
    recording = true;
    held = [];
    error = null;
    sawKey = false;
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("mousedown", onMouseDown, true);
    window.addEventListener("blur", cancel);
    await beginShortcutCapture();
  }

  async function finish(binding?: string) {
    if (!recording) return;
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("keyup", onKeyUp, true);
    window.removeEventListener("mousedown", onMouseDown, true);
    window.removeEventListener("blur", cancel);
    recording = false;
    held = [];
    await endShortcutCapture();
    if (binding !== undefined && binding !== value) onchange(binding);
  }

  function cancel() {
    finish();
  }

  function reject(msg: string) {
    error = msg;
    shake++;
  }

  function onKeyDown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    // No code: the no-op key Windows' input hook injects so a lone Win tap
    // doesn't open the Start menu.
    if (e.repeat || !e.code) return;
    const bare = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    if (kind === "mouse") {
      if (e.code === "Escape") cancel();
      else if (e.code === "Backspace" || e.code === "Delete") finish("");
      else reject(t("configuration.recorder.pressMouseButton"));
      return;
    }
    if (isModifierCode(e.code)) {
      const code = normalizeModifierCode(e.code);
      if (!held.includes(code)) held = [...held, code];
      return;
    }
    sawKey = true;
    if (bare && e.code === "Escape" && !allowBareEscape) return cancel();
    if (bare && (e.code === "Backspace" || e.code === "Delete")) return void finish("");
    const res = acceleratorFromEvent(e, allowBareEscape);
    if (!res) return;
    if ("error" in res) reject(res.error);
    else finish(res.binding);
  }

  function onKeyUp(e: KeyboardEvent) {
    if (kind !== "keyboard" || !isModifierCode(e.code)) return;
    e.preventDefault();
    const code = normalizeModifierCode(e.code);
    const lone = !sawKey && held.length === 1 && held[0] === code;
    held = held.filter((c) => c !== code);
    if (lone) {
      if (NATIVE_SHORTCUTS && allowModifierOnly) finish(`mod:${code}`);
      else reject(t("configuration.recorder.combineModifier"));
    }
  }

  function onMouseDown(e: MouseEvent) {
    const inside = root.contains(e.target as Node);
    if (kind === "mouse") {
      const b = mouseBinding(e.button);
      if (b) {
        e.preventDefault();
        e.stopPropagation();
        finish(b);
        return;
      }
    }
    if (e.button === 0 && !inside) cancel();
  }

  $effect(() => () => {
    // Unmounted mid-capture (page switch): restore the bindings.
    if (recording) finish();
  });
</script>

<div class="recorder" bind:this={root}>
  {#if needsAx && !recording}
    <button class="ax" transition:fade={{ duration: 160 }} onclick={() => requestPermission("accessibility").then(() => setTimeout(() => store.refreshPerms(), 1500))}>
      <Icon name="warning" size={12} /> {t("configuration.recorder.needsAccessibility")}
    </button>
  {/if}
  <button
    type="button"
    class="reset"
    class:hidden={value === defaultValue || recording}
    title={t("configuration.recorder.resetTitle")}
    aria-label={t("configuration.recorder.resetLabel")}
    tabindex={value === defaultValue ? -1 : 0}
    onclick={() => onchange(defaultValue)}
  >
    <Icon name="reset" size={13} stroke={2} />
  </button>

  {#key shake}
    <button
      type="button"
      class="field"
      class:recording
      class:empty={!value}
      class:shake={shake > 0 && recording}
      onclick={start}
      title={recording ? "" : t("configuration.recorder.clickToRecord")}
    >
      {#if recording}
        <span class="live" in:fade={{ duration: 140 }}>
          {#if heldPreview.length}
            {#each heldPreview as c, i (i)}<kbd>{c.label}{#if c.side}<sup>{c.side}</sup>{/if}</kbd>{/each}
          {:else}
            <span class="dot"></span>{kind === "mouse" ? t("configuration.recorder.pressButton") : t("configuration.recorder.pressKeys")}
          {/if}
        </span>
      {:else if value}
        <span in:scale={{ duration: 220, start: 0.85, easing: cubicInOut }}><Keys binding={value} /></span>
      {:else}
        <span class="placeholder" in:fade={{ duration: 160 }}>{t("configuration.recorder.placeholder")}</span>
      {/if}
    </button>
  {/key}

  {#if error && recording}
    <div class="error" transition:fade={{ duration: 160 }}>{error}</div>
  {/if}
</div>

<style>
  .recorder {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .reset {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    transition:
      opacity 0.22s var(--ease),
      transform 0.3s var(--ease),
      background-color 0.2s var(--ease);
  }
  .reset:hover {
    background: var(--selected);
    color: var(--text);
    transform: rotate(-40deg);
  }
  .reset.hidden {
    opacity: 0;
    pointer-events: none;
    transform: rotate(60deg) scale(0.6);
  }
  .field {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 26px;
    padding: 3px 4px;
    border: 1px solid transparent;
    border-radius: 7px;
    background: transparent;
    font-weight: 450;
    transition:
      background-color 0.22s var(--ease),
      border-color 0.22s var(--ease),
      box-shadow 0.3s var(--ease);
  }
  .field:hover {
    background: var(--selected);
  }
  .field.empty {
    padding: 3px 8px;
  }
  .placeholder {
    color: var(--text-3);
    font-weight: 500;
  }
  .field.recording {
    min-width: 132px;
    justify-content: center;
    background: var(--accent-soft);
    border-color: var(--accent);
    animation: glow 1.6s var(--ease) infinite;
  }
  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent);
    font-weight: 500;
  }
  .live kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 20px;
    padding: 0 6px;
    border-radius: 5px;
    background: var(--accent);
    color: #fff;
    font-family: inherit;
    font-size: 11.5px;
  }
  .live sup {
    font-size: 7.5px;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    animation: blink 1s var(--ease) infinite;
  }
  .shake {
    animation:
      shake 0.36s var(--ease),
      glow 1.6s var(--ease) infinite;
  }
  .error {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 5;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    color: var(--orange);
    font-size: 11px;
    white-space: nowrap;
  }
  .ax {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border: none;
    border-radius: 999px;
    background: rgba(255, 159, 10, 0.14);
    color: var(--orange);
    font-size: 11px;
  }
  @keyframes glow {
    0%,
    100% {
      box-shadow: 0 0 0 0 var(--accent-soft);
    }
    50% {
      box-shadow: 0 0 0 4px var(--accent-soft);
    }
  }
  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }
  @keyframes shake {
    20%,
    60% {
      transform: translateX(-4px);
    }
    40%,
    80% {
      transform: translateX(4px);
    }
  }
</style>
