<script lang="ts">
  import Pill from "./lib/Pill.svelte";
  import Settings from "./lib/Settings.svelte";
  import { currentWindowLabel } from "./lib/ipc";

  // One bundle, two windows: the floating pill ("main") and the standalone,
  // decorated settings window ("settings"). Route by the window label.
  const label = currentWindowLabel();
</script>

{#if label === "settings"}
  <main class="settings-window">
    <Settings />
  </main>
{:else}
  <Pill />
{/if}

<style>
  .settings-window {
    height: 100%;
    background-color: #1b1b1f;
    padding: 1.5rem 1.5rem 2rem;
    overflow-y: auto;
  }
</style>
