<script lang="ts">
  import Status from "./lib/Status.svelte";
  import Settings from "./lib/Settings.svelte";

  type Tab = "status" | "settings";
  let tab = $state<Tab>("status");
</script>

<main class="app">
  <nav class="tabs">
    <button class:active={tab === "status"} onclick={() => (tab = "status")}>
      Estado
    </button>
    <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>
      Ajustes
    </button>
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

  .tabs {
    display: flex;
    gap: 0.25rem;
    padding: 0.5rem;
    border-bottom: 1px solid #2a2a30;
  }

  .tabs button {
    flex: 1;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: #9a9aa2;
  }

  .tabs button.active {
    background-color: #2a2a30;
    color: #e6e6e6;
  }

  .content {
    flex: 1;
    padding: 1.25rem;
    overflow-y: auto;
  }
</style>
