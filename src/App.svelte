<script lang="ts">
  import { onMount } from "svelte";
  import Pill from "./lib/Pill.svelte";
  import SettingsApp from "./lib/settings/SettingsApp.svelte";
  import Welcome from "./lib/Welcome.svelte";
  import { currentWindowLabel, loadSettings, onSettingsChanged } from "./lib/ipc";
  import { applyTheme } from "./lib/theme";
  import { setLocale } from "./lib/i18n/index.svelte";

  // One bundle, three windows: the floating pill ("main"), the standalone
  // settings window ("settings") and the first-launch welcome ("welcome").
  // Route by the window label.
  const label = currentWindowLabel();

  // The settings window manages its own theme (store); the others follow the
  // saved setting here.
  onMount(() => {
    if (label === "settings") return;
    loadSettings()
      .then((v) => {
        applyTheme(v.theme);
        setLocale(v.uiLanguage);
      })
      .catch(() => applyTheme("auto"));
    const off = onSettingsChanged((v) => {
      applyTheme(v.theme);
      setLocale(v.uiLanguage);
    });
    return () => {
      off.then((fn) => fn());
    };
  });
</script>

{#if label === "settings"}
  <SettingsApp />
{:else if label === "welcome"}
  <main class="welcome-window">
    <Welcome />
  </main>
{:else}
  <Pill />
{/if}

<style>
  .welcome-window {
    height: 100%;
    background-color: var(--bg);
    color: var(--text);
    padding: 1.75rem 1.75rem 2rem;
    overflow-y: auto;
  }
</style>
