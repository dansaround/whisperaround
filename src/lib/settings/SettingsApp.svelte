<script lang="ts">
  import "./settings.css";
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { cubicInOut } from "svelte/easing";
  import Sidebar from "./Sidebar.svelte";
  import Icon from "./ui/Icon.svelte";
  import Select from "./ui/Select.svelte";
  import Home from "./pages/Home.svelte";
  import Modes from "./pages/Modes.svelte";
  import Vocabulary from "./pages/Vocabulary.svelte";
  import Configuration from "./pages/Configuration.svelte";
  import Sound from "./pages/Sound.svelte";
  import Models from "./pages/Models.svelte";
  import History from "./pages/History.svelte";
  import { store } from "./store.svelte";
  import { IS_MAC, IS_WINDOWS, windowMaterial } from "../ipc";
  import type { WindowMaterial } from "../types";
  import { t } from "../i18n/index.svelte";

  let scroller = $state<HTMLDivElement>();
  /** macOS always has its vibrancy; Windows reports whether Mica is on. */
  let material = $state<WindowMaterial>(IS_MAC ? "vibrancy" : "none");

  const PAGES = { home: Home, modes: Modes, vocabulary: Vocabulary, configuration: Configuration, sound: Sound, models: Models, history: History };
  const Page = $derived(PAGES[store.page]);

  const micOptions = $derived.by(() => {
    const def = store.devices.find((d) => d.isDefault);
    return [
      { value: "", label: t("shell.mic.defaultDevice", { name: def?.name ?? t("shell.mic.systemMicrophone") }), icon: "laptop" },
      ...store.devices.filter((d) => !d.isDefault).map((d) => ({ value: d.name, label: d.name, icon: "mic" })),
      // System audio is captured through WASAPI loopback, which only exists on Windows.
      ...(IS_WINDOWS
        ? [
            { value: "system:", label: t("shell.mic.systemAudio"), icon: "speaker" },
            { value: "mix:", label: t("shell.mic.micAndSystem"), icon: "speaker-low" },
          ]
        : []),
    ];
  });

  onMount(() => {
    windowMaterial().then((m) => (material = m));
    store.init().catch((e) => store.notify(t("shell.toast.loadFailed", { error: String(e) }), "error"));
    // Pick up permissions granted in System Settings / devices plugged in meanwhile.
    const onFocus = () => {
      store.refreshPerms();
      store.refreshDevices();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  $effect(() => {
    store.page;
    scroller?.scrollTo({ top: 0 });
  });
</script>

<div class="app" class:mac={IS_MAC} class:windows={IS_WINDOWS} class:translucent={material !== "none"} class:mica={material === "mica"}>
  <Sidebar />

  <section class="main">
    <header data-tauri-drag-region>
      <div class="spacer" data-tauri-drag-region></div>
      {#if store.view}
        <Select
          variant="ghost"
          align="right"
          icon="laptop"
          minWidth={260}
          value={store.view.inputDevice}
          options={micOptions}
          onchange={(v) => store.patch({ inputDevice: v })}
        />
      {/if}
    </header>

    <div class="scroll" bind:this={scroller}>
      {#if store.view}
        <div class="stack">
          {#key store.page}
            <div class="page-wrap" in:fade={{ duration: 140, easing: cubicInOut }} out:fade={{ duration: 70, easing: cubicInOut }}>
              <Page />
            </div>
          {/key}
        </div>
      {/if}
    </div>
  </section>

  {#if store.toast}
    {#key store.toast.id}
      <div class="toast {store.toast.kind}" in:fly={{ y: 16, duration: 320, easing: cubicInOut }} out:fade={{ duration: 200 }}>
        <Icon name={store.toast.kind === "error" ? "warning" : "check"} size={14} stroke={2} />
        <span>{store.toast.text}</span>
      </div>
    {/key}
  {/if}
</div>

<style>
  .app {
    display: flex;
    height: 100vh;
    background: var(--bg);
    color: var(--text);
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
    transition:
      background-color 0.4s var(--ease),
      color 0.4s var(--ease);
  }
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  /* The window is transparent over a native material (see `windowEffects`
     in tauri.conf.json): the "sidebar" vibrancy on macOS, Mica on Windows 11.
     The sidebar shows it fully; the content area keeps a light tint of the
     page color on top. */
  .app.translucent {
    background: transparent;
  }
  .app.translucent .main {
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    transition: background-color 0.4s var(--ease);
  }
  /* Windows 11: the content sits on a layer with a rounded top-left corner,
     like WinUI's navigation view. */
  .app.mica .main {
    border-top: 1px solid var(--line);
    border-left: 1px solid var(--line);
    border-top-left-radius: 8px;
  }
  /* Windows draws a native title bar, so there are no traffic lights to
     clear and the header can be shorter. */
  .app.windows header {
    height: 46px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    flex: none;
    padding: 0 14px;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
  }
  .stack {
    display: grid;
  }
  .page-wrap {
    grid-area: 1 / 1;
    min-width: 0;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 18px;
    translate: -50% 0;
    z-index: 100;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 70%;
    padding: 8px 14px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    font-size: 12px;
    user-select: text;
    -webkit-user-select: text;
  }
  .toast.error {
    color: var(--red);
  }
  .toast.ok {
    color: var(--green);
  }
  .toast span {
    color: var(--text);
  }
  :global(.scroll::-webkit-scrollbar) {
    width: 10px;
  }
  :global(.scroll::-webkit-scrollbar-thumb) {
    background: var(--line-strong);
    border-radius: 999px;
    border: 3px solid transparent;
    background-clip: content-box;
  }
</style>
