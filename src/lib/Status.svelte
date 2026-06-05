<script lang="ts">
  import { onMount } from "svelte";
  import { getStatus, onStatus, toggleRecording } from "./ipc";
  import type { AppStatus } from "./types";

  let status = $state<AppStatus>("idle");
  let errorMessage = $state<string | null>(null);

  const LABELS: Record<AppStatus, string> = {
    idle: "Listo",
    recording: "Grabando…",
    processing: "Procesando…",
    error: "Error",
  };

  onMount(() => {
    getStatus().then((s) => (status = s));
    const unlisten = onStatus((e) => {
      status = e.status;
      errorMessage = e.status === "error" ? e.message ?? "Error desconocido" : null;
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="status">
  <div class="indicator {status}" aria-live="polite">
    <span class="dot"></span>
    <span class="label">{LABELS[status]}</span>
  </div>

  {#if status === "error" && errorMessage}
    <p class="error-msg">{errorMessage}</p>
  {/if}

  <button class="toggle" onclick={() => toggleRecording()} disabled={status === "processing"}>
    {status === "recording" ? "Detener y transcribir" : "Empezar a grabar"}
  </button>

  <p class="hint">
    Atajo global: empieza/detiene la grabación desde cualquier app. El texto se
    copia al portapapeles y se pega automáticamente.
  </p>
</div>

<style>
  .status {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.25rem;
    text-align: center;
  }

  .indicator {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 1.2rem;
    border-radius: 999px;
    background-color: #232329;
    font-weight: 600;
    font-size: 1.1rem;
  }

  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background-color: #6b6b73;
  }

  .indicator.idle .dot {
    background-color: #4ade80;
  }
  .indicator.recording .dot {
    background-color: #f87171;
    animation: pulse 1s infinite;
  }
  .indicator.processing .dot {
    background-color: #fbbf24;
    animation: pulse 1s infinite;
  }
  .indicator.error .dot {
    background-color: #f87171;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.3;
    }
  }

  .toggle {
    min-width: 220px;
  }

  .error-msg {
    color: #f87171;
    margin: 0;
    font-size: 0.9rem;
    max-width: 320px;
  }

  .hint {
    color: #8a8a92;
    font-size: 0.85rem;
    max-width: 320px;
    margin: 0;
  }
</style>
