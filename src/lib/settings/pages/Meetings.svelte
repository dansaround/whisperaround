<script lang="ts">
  import { onMount } from "svelte";
  import { fade, slide } from "svelte/transition";
  import { cubicInOut } from "svelte/easing";
  import Icon from "../ui/Icon.svelte";
  import Select from "../ui/Select.svelte";
  import Toggle from "../ui/Toggle.svelte";
  import { store } from "../store.svelte";
  import {
    IS_WINDOWS,
    exportMeeting,
    getMeetingFolder,
    openMeetingFolder,
    pickMeetingFolder,
    startMeeting,
    stopMeeting,
  } from "../../ipc";
  import { clock, longClock, meetingFileName, meetingMarkdown, segmentLine } from "../../meeting";
  import { modelLabel } from "../../models";
  import { t } from "../../i18n/index.svelte";
  import type { MeetingSource } from "../../types";

  const v = $derived(store.view!);
  const m = $derived(store.meeting);
  const recording = $derived(m.active && !m.finishing);
  // System audio can only be captured on Windows.
  const source = $derived<MeetingSource>(IS_WINDOWS ? v.meetingSource : "mic");
  const sourceOptions = $derived([
    { value: "mix" as MeetingSource, label: t("meetings.source.mix") },
    { value: "system" as MeetingSource, label: t("meetings.source.system") },
    { value: "mic" as MeetingSource, label: t("meetings.source.mic") },
  ]);
  /** Where the transcripts go (the default folder when none was chosen). */
  let folder = $state("");
  let copied = $state(false);
  let now = $state(Date.now());

  // Elapsed-time ticker, only while recording.
  onMount(() => {
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  // Follow the setting, which another window may change too.
  $effect(() => {
    void v.meetingFolder;
    getMeetingFolder()
      .then((f) => (folder = f))
      .catch((e) => console.error("getMeetingFolder failed", e));
  });

  const hint = $derived(
    !IS_WINDOWS
      ? t("meetings.hintMicOnly")
      : source === "mix"
        ? t("meetings.hintBoth")
        : source === "system"
          ? t("meetings.hintSystem")
          : t("meetings.hintSuggestMix"),
  );

  async function chooseFolder() {
    try {
      const picked = await pickMeetingFolder();
      if (picked) await store.patch({ meetingFolder: picked });
    } catch (e) {
      store.notify(String(e), "error");
    }
  }

  async function openFolder() {
    try {
      await openMeetingFolder();
    } catch (e) {
      store.notify(String(e), "error");
    }
  }

  function toggle() {
    if (m.active) {
      stopMeeting();
    } else if (!v.meetingConsent) {
      store.consentPrompt = true;
    } else {
      startMeeting();
    }
  }

  async function accept() {
    store.consentPrompt = false;
    if (await store.patch({ meetingConsent: true })) startMeeting();
  }

  async function copyAll() {
    try {
      await navigator.clipboard.writeText(m.segments.map(segmentLine).join("\n"));
      copied = true;
      setTimeout(() => (copied = false), 1400);
    } catch {
      store.notify(t("meetings.copyFailed"), "error");
    }
  }

  async function exportMd() {
    try {
      const path = await exportMeeting(meetingFileName(m.startedTs), meetingMarkdown(m));
      store.notify(t("meetings.exported", { path }));
    } catch (e) {
      store.notify(String(e), "error");
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <h2>{t("meetings.title")}</h2>
  </div>

  {#if store.consentPrompt && !m.active}
    <div class="card consent" transition:slide={{ duration: 220, easing: cubicInOut }}>
      <span class="warn-icon"><Icon name="warning" size={16} /></span>
      <div class="consent-text">
        <strong>{t("meetings.consent.title")}</strong>
        <small>{t("meetings.consent.body")}</small>
        <div class="consent-actions">
          <button class="pill-button primary" onclick={accept}>{t("meetings.consent.accept")}</button>
          <button class="pill-button" onclick={() => (store.consentPrompt = false)}>{t("meetings.consent.cancel")}</button>
        </div>
      </div>
    </div>
  {/if}

  <div class="card">
    <div class="row">
      <div class="row-text">
        <strong class="state">
          {#if m.finishing}
            <span class="spinner" aria-hidden="true"></span> {t("meetings.finishing")}
          {:else if recording}
            <span class="rec-dot" aria-hidden="true"></span> {t("meetings.recording")} · <span class="time">{clock(now - m.startedTs)}</span>
          {:else}
            {t("meetings.idle")}
          {/if}
        </strong>
        <small>{hint}</small>
        {#if m.model}<small>{t("meetings.model", { model: modelLabel(m.model) })}</small>{/if}
      </div>
      <div class="control">
        <button class="pill-button big" class:stop={m.active} disabled={m.finishing} onclick={toggle}>
          <Icon name={m.active ? "x" : "record"} size={14} />
          {m.active ? t("meetings.stop") : t("meetings.start")}
        </button>
      </div>
    </div>
  </div>

  <div class="card">
    {#if IS_WINDOWS}
      <div class="row">
        <div class="row-text">
          <strong>{t("meetings.source.title")}</strong>
          <small>{t("meetings.source.sub")}</small>
        </div>
        <div class="control">
          <Select value={source} options={sourceOptions} align="right" onchange={(meetingSource) => store.patch({ meetingSource })} />
        </div>
      </div>
    {/if}
    <div class="row">
      <div class="row-text">
        <strong>{t("meetings.folder.title")}</strong>
        <small class="path" title={folder}>{folder}</small>
      </div>
      <div class="control buttons">
        {#if v.meetingFolder}
          <button class="pill-button" onclick={() => store.patch({ meetingFolder: "" })}>{t("meetings.folder.reset")}</button>
        {/if}
        <button class="pill-button" onclick={openFolder}><Icon name="external" size={13} /> {t("meetings.folder.open")}</button>
        <button class="pill-button" onclick={chooseFolder}><Icon name="folder" size={13} /> {t("meetings.folder.change")}</button>
      </div>
    </div>
    <div class="row">
      <div class="row-text">
        <strong>{t("meetings.autosave.title")}</strong>
        <small>{t("meetings.autosave.sub")}</small>
      </div>
      <div class="control">
        <Toggle label={t("meetings.autosave.title")} checked={v.meetingAutosave} onchange={(c) => store.patch({ meetingAutosave: c })} />
      </div>
    </div>
  </div>

  <section class="section">
    <div class="page-head">
      <h3 class="section-title">{t("meetings.transcript")}</h3>
      {#if m.segments.length}
        <span class="actions" transition:fade={{ duration: 150 }}>
          <button class="pill-button" onclick={copyAll}>
            <Icon name={copied ? "check" : "copy"} size={13} />
            {copied ? t("meetings.copied") : t("meetings.copyAll")}
          </button>
          <button class="pill-button" onclick={exportMd}><Icon name="share" size={13} /> {t("meetings.exportMd")}</button>
        </span>
      {/if}
    </div>

    {#if m.segments.length}
      <div class="card list">
        {#each m.segments as s (s.startMs + ":" + s.speaker)}
          <div class="seg" in:fade={{ duration: 160 }}>
            <span class="ts">{longClock(s.startMs)}</span>
            {#if s.speaker}
              <span class="badge" class:blue={s.speaker === "mic"}>{s.speaker === "mic" ? t("meetings.me") : t("meetings.others")}</span>
            {/if}
            <span class="seg-text">{s.text}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="card empty">
        <small>{recording ? t("meetings.emptyRecording") : t("meetings.emptyIdle")}</small>
      </div>
    {/if}
  </section>
</div>

<style>
  .consent {
    display: flex;
    gap: 12px;
    padding: 14px 16px;
    border-color: color-mix(in srgb, var(--orange) 45%, var(--line));
  }
  .warn-icon {
    color: var(--orange);
    display: inline-flex;
    padding-top: 1px;
  }
  .consent-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .consent-text small {
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .consent-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .primary {
    background: var(--accent);
    border-color: transparent;
    color: #fff;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .time {
    font-variant-numeric: tabular-nums;
  }
  .rec-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--red);
    animation: blink 1.4s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.35;
    }
  }
  .spinner {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .big {
    padding: 7px 16px;
  }
  .big.stop {
    background: var(--red);
    border-color: transparent;
    color: #fff;
  }
  .big:disabled {
    opacity: 0.5;
    pointer-events: none;
  }
  .actions,
  .buttons {
    display: inline-flex;
    gap: 8px;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list {
    overflow: hidden;
  }
  .seg {
    display: flex;
    align-items: baseline;
    gap: 9px;
    padding: 9px 16px;
    line-height: 1.5;
  }
  .seg + .seg {
    border-top: 1px solid var(--line);
  }
  .ts {
    flex: none;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
  }
  .seg-text {
    min-width: 0;
    user-select: text;
    -webkit-user-select: text;
  }
  .empty {
    padding: 28px 20px;
    text-align: center;
    color: var(--text-2);
  }
</style>
