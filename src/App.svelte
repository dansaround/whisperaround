<script lang="ts">
  import Status from "./lib/Status.svelte";
  import Settings from "./lib/Settings.svelte";
  import { minimizeWindow, hideWindow } from "./lib/ipc";

  type Tab = "status" | "settings";
  let tab = $state<Tab>("status");
</script>

<main class="app">
  <!-- Single top bar (OS decorations are disabled): tabs on the left, window
       controls on the right. The whole bar is a drag region; the buttons are
       children so they stay clickable and don't start a drag. -->
  <nav class="topbar" data-tauri-drag-region>
    <div class="tabs">
      <button class:active={tab === "status"} onclick={() => (tab = "status")}>
        Estado
      </button>
      <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>
        Ajustes
      </button>
    </div>
    <div class="window-controls">
      <button class="wc" title="Minimizar" onclick={() => minimizeWindow()} aria-label="Minimizar">
        &#x2013;
      </button>
      <button class="wc close" title="Cerrar" onclick={() => hideWindow()} aria-label="Cerrar">
        &#x2715;
      </button>
    </div>
  </nav>

  <section class="content">
    {#if tab === "status"}
      <Status />
    {:else}
      <Settings />
    {/if}
  </section>
</main>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.45rem 0.5rem;
    border-bottom: 1px solid #26262c;
    user-select: none;
    -webkit-user-select: none;
  }

  .tabs {
    display: flex;
    gap: 0.25rem;
  }

  .tabs button {
    background: transparent;
    border: none;
    border-radius: 6px;
    color: #9a9aa2;
    padding: 0.35em 0.8em;
  }

  .tabs button.active {
    background-color: #2a2a30;
    color: #e6e6e6;
  }

  .window-controls {
    display: flex;
    gap: 0.25rem;
  }

  .wc {
    width: 1.7rem;
    height: 1.7rem;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: #9a9aa2;
    font-size: 0.85rem;
    line-height: 1;
  }

  .wc:hover {
    background-color: #2a2a30;
    color: #e6e6e6;
  }

  .wc.close:hover {
    background-color: #e0245e;
    color: #fff;
  }

  .content {
    flex: 1;
    padding: 1.25rem;
    overflow-y: auto;
  }
</style>
