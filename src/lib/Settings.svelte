<script lang="ts">
  import { onMount } from "svelte";
  import { loadSettings, saveSettings } from "./ipc";
  import type { SettingsView } from "./types";

  let model = $state("whisper-1");
  let shortcut = $state("CmdOrCtrl+Shift+X");
  let autoPaste = $state(true);
  let apiKeyConfigured = $state(false);
  /** Empty means "leave the stored key untouched". */
  let apiKeyInput = $state("");

  let saving = $state(false);
  let savedAt = $state<number | null>(null);

  function apply(view: SettingsView) {
    model = view.model;
    shortcut = view.shortcut;
    autoPaste = view.autoPaste;
    apiKeyConfigured = view.apiKeyConfigured;
    apiKeyInput = "";
  }

  onMount(() => {
    loadSettings().then(apply);
  });

  async function save() {
    saving = true;
    try {
      const view = await saveSettings({
        openaiApiKey: apiKeyInput,
        model,
        shortcut,
        autoPaste,
      });
      apply(view);
      savedAt = Date.now();
    } finally {
      saving = false;
    }
  }
</script>

<form class="settings" onsubmit={(e) => (e.preventDefault(), save())}>
  <label class="field">
    <span>OpenAI API key</span>
    <input
      type="password"
      autocomplete="off"
      placeholder={apiKeyConfigured ? "•••••••••• (configurada)" : "sk-…"}
      bind:value={apiKeyInput}
    />
    <small>
      {apiKeyConfigured
        ? "Hay una key guardada. Déjalo en blanco para conservarla."
        : "Aún no hay key configurada."}
    </small>
  </label>

  <label class="field">
    <span>Modelo</span>
    <select bind:value={model}>
      <option value="whisper-1">whisper-1</option>
      <option value="gpt-4o-transcribe">gpt-4o-transcribe</option>
      <option value="gpt-4o-mini-transcribe">gpt-4o-mini-transcribe</option>
    </select>
  </label>

  <label class="field">
    <span>Atajo global</span>
    <input type="text" bind:value={shortcut} placeholder="CmdOrCtrl+Shift+X" />
    <small>Formato Tauri, p. ej. <code>CmdOrCtrl+Shift+X</code>. Se aplica al guardar (sin reiniciar).</small>
  </label>

  <label class="field checkbox">
    <input type="checkbox" bind:checked={autoPaste} />
    <span>Pegar automáticamente (Ctrl/Cmd+V) tras transcribir</span>
  </label>

  <div class="actions">
    <button type="submit" disabled={saving}>
      {saving ? "Guardando…" : "Guardar"}
    </button>
    {#if savedAt}
      <span class="saved">Guardado ✓</span>
    {/if}
  </div>
</form>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 360px;
    margin: 0 auto;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .field > span {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .field small {
    color: #8a8a92;
    font-size: 0.78rem;
  }

  .field.checkbox {
    flex-direction: row;
    align-items: center;
    gap: 0.6rem;
  }

  .field.checkbox input {
    width: auto;
  }

  .field.checkbox span {
    font-weight: 400;
    font-size: 0.9rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .saved {
    color: #4ade80;
    font-size: 0.85rem;
  }

  code {
    background-color: #232329;
    padding: 0 0.3em;
    border-radius: 4px;
  }
</style>
