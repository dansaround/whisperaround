<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./ui/Icon.svelte";
  import { store, type Page } from "./store.svelte";
  import { installUpdate } from "../ipc";
  import { t, type Key } from "../i18n/index.svelte";

  type Item = { id: Page; label: Key; icon: string; color: string };
  const GROUPS: Item[][] = [
    [
      { id: "home", label: "shell.nav.home", icon: "home", color: "linear-gradient(180deg,#ffa53d,#ff7a1a)" },
      { id: "modes", label: "shell.nav.modes", icon: "sparkle", color: "linear-gradient(180deg,#4f8fff,#2f6bf0)" },
      { id: "vocabulary", label: "shell.nav.vocabulary", icon: "book", color: "linear-gradient(180deg,#4f8fff,#2f6bf0)" },
    ],
    [
      { id: "configuration", label: "shell.nav.configuration", icon: "gear", color: "linear-gradient(180deg,#8e8e95,#6b6b72)" },
      { id: "sound", label: "shell.nav.sound", icon: "speaker", color: "linear-gradient(180deg,#8e8e95,#6b6b72)" },
      { id: "models", label: "shell.nav.models", icon: "library", color: "linear-gradient(180deg,#8e8e95,#6b6b72)" },
    ],
    [{ id: "history", label: "shell.nav.history", icon: "clock", color: "linear-gradient(180deg,#b36bff,#8b4cf0)" }],
  ];

  let nav: HTMLElement;
  let hl = $state({ y: 0, h: 0, ready: false });
  let updating = $state(false);

  // The selection highlight is a single element that glides between items.
  async function place() {
    await tick();
    const el = nav?.querySelector<HTMLElement>(`[data-page="${store.page}"]`);
    if (!el) return;
    hl = { y: el.offsetTop, h: el.offsetHeight, ready: true };
  }
  $effect(() => {
    store.page;
    place();
  });

  async function update() {
    updating = true;
    try {
      await installUpdate();
    } catch (e) {
      store.notify(String(e), "error");
      updating = false;
    }
  }
</script>

<aside class="sidebar">
  <div class="drag" data-tauri-drag-region></div>
  <nav bind:this={nav}>
    <div class="highlight" class:ready={hl.ready} style="transform:translateY({hl.y}px);height:{hl.h}px"></div>
    {#each GROUPS as group, g (g)}
      <div class="group">
        {#each group as item (item.id)}
          <button
            class="item"
            class:active={store.page === item.id}
            data-page={item.id}
            onclick={() => (store.page = item.id)}
          >
            <span class="squircle" style="background:{item.color}"><Icon name={item.icon} size={12} stroke={2.2} /></span>
            <span class="label">{t(item.label)}</span>
          </button>
        {/each}
      </div>
    {/each}
  </nav>

  <div class="foot">
    {#if store.update}
      <button class="brand update" disabled={updating} onclick={update} title={t("shell.update.install", { version: store.update })}>
        <span>{updating ? t("shell.update.updating") : t("shell.update.button")}</span>
        <span class="tag">{store.update}</span>
      </button>
    {:else}
      <div class="brand">
        <span>WhisperAround</span>
        {#if store.version}<span class="tag">{store.version}</span>{/if}
      </div>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 196px;
    flex: none;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
    overflow: hidden;
  }
  /* Let the native material behind the window (vibrancy / Mica) show through. */
  :global(.app.translucent) .sidebar {
    background: transparent;
  }
  :global(.app.mica) .sidebar {
    border-right: none;
  }
  /* Windows: the native title bar sits above, nothing to clear for. */
  :global(.app.windows) .drag {
    height: 8px;
  }
  /* Windows 11: an accent pill marks the selected item. */
  :global(.app.windows) .highlight::before {
    content: "";
    position: absolute;
    left: 0;
    top: 50%;
    width: 3px;
    height: 16px;
    margin-top: -8px;
    border-radius: 2px;
    background: var(--accent);
  }
  .drag {
    height: 52px;
    flex: none;
  }
  nav {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 0 10px;
    width: 196px;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .highlight {
    position: absolute;
    left: 10px;
    right: 10px;
    top: 0;
    border-radius: 8px;
    background: var(--selected);
    opacity: 0;
    pointer-events: none;
  }
  .highlight.ready {
    opacity: 1;
    will-change: transform;
    transition:
      transform 0.18s var(--ease),
      height 0.18s var(--ease);
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    height: 32px;
    padding: 0 8px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    font-weight: 500;
    text-align: left;
    white-space: nowrap;
  }
  .item:hover:not(.active) {
    background: color-mix(in srgb, var(--selected) 50%, transparent);
  }
  .item:active {
    transform: scale(0.98);
  }
  .squircle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    color: #fff;
    box-shadow:
      inset 0 0.5px 0 rgba(255, 255, 255, 0.35),
      0 1px 2px rgba(0, 0, 0, 0.25);
    transition: transform 0.3s var(--spring);
  }
  .item:hover .squircle {
    transform: scale(1.08);
  }
  .foot {
    margin-top: auto;
    padding: 12px;
    width: 196px;
  }
  .brand {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    width: 100%;
    height: 34px;
    border-radius: 999px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text-2);
    font-weight: 600;
    font-size: 12px;
  }
  .tag {
    padding: 0 5px;
    border-radius: 4px;
    background: var(--chip);
    color: var(--text-2);
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .brand.update {
    color: #fff;
    background: var(--accent);
    border-color: transparent;
    animation: breathe 2.4s var(--ease) infinite;
  }
  .brand.update .tag {
    background: rgba(255, 255, 255, 0.22);
    color: #fff;
  }
  @keyframes breathe {
    50% {
      box-shadow: 0 0 0 5px var(--accent-soft);
    }
  }
</style>
