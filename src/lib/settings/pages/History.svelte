<script lang="ts">
  import { slide, fade } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicInOut } from "svelte/easing";
  import Icon from "../ui/Icon.svelte";
  import Toggle from "../ui/Toggle.svelte";
  import { store } from "../store.svelte";
  import { clearHistory, deleteHistoryEntry, exportMeeting } from "../../ipc";
  import { meetingFileName, meetingMarkdown } from "../../meeting";
  import { modelLabel } from "../../models";
  import type { HistoryEntry } from "../../types";
  import { t, tn, intlLocale } from "../../i18n/index.svelte";

  const v = $derived(store.view!);
  let query = $state("");
  let copied = $state<number | null>(null);
  let expanded = $state<number | null>(null);
  let confirmClear = $state(false);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = [...store.history.entries].reverse();
    return q ? list.filter((e) => e.text.toLowerCase().includes(q) || e.app?.toLowerCase().includes(q)) : list;
  });

  function dayLabel(ts: number): string {
    const d = new Date(ts);
    const today = new Date().setHours(0, 0, 0, 0);
    const day = new Date(ts).setHours(0, 0, 0, 0);
    if (day === today) return t("history.today");
    if (day === today - 86_400_000) return t("history.yesterday");
    return d.toLocaleDateString(intlLocale(), { weekday: "long", month: "short", day: "numeric" });
  }

  const groups = $derived.by(() => {
    const out: { label: string; items: HistoryEntry[] }[] = [];
    for (const e of filtered) {
      const label = dayLabel(e.ts);
      if (out.at(-1)?.label === label) out.at(-1)!.items.push(e);
      else out.push({ label, items: [e] });
    }
    return out;
  });

  function time(ts: number) {
    return new Date(ts).toLocaleTimeString(intlLocale(), { hour: "2-digit", minute: "2-digit" });
  }

  function seconds(ms: number) {
    const s = Math.round(ms / 1000);
    return s < 60 ? t("history.seconds", { s }) : t("history.minutesSeconds", { m: Math.floor(s / 60), s: s % 60 });
  }

  async function copy(e: HistoryEntry) {
    try {
      await navigator.clipboard.writeText(e.text);
      copied = e.id;
      setTimeout(() => copied === e.id && (copied = null), 1400);
    } catch {
      store.notify(t("history.copyFailed"), "error");
    }
  }

  async function exportMd(e: HistoryEntry) {
    try {
      const path = await exportMeeting(meetingFileName(e.ts, v.meetingDateFormat, v.meetingName), meetingMarkdown(e, v.meetingDateFormat, v.meetingName));
      store.notify(t("meetings.exported", { path }));
    } catch (err) {
      store.notify(String(err), "error");
    }
  }

  async function remove(e: HistoryEntry) {
    try {
      await deleteHistoryEntry(e.id);
      await store.refreshHistory();
    } catch (err) {
      store.notify(String(err), "error");
    }
  }

  async function clearAll() {
    confirmClear = false;
    try {
      await clearHistory();
      await store.refreshHistory();
    } catch (err) {
      store.notify(String(err), "error");
    }
  }
</script>

<div class="page">
  <div class="page-head">
    <h2>{t("history.title")}</h2>
    <div class="head-actions">
      <span class="muted">{t("history.saveHistory")}</span>
      <Toggle label={t("history.saveHistory")} checked={v.saveHistory} onchange={(c) => store.patch({ saveHistory: c })} />
      {#if store.history.entries.length}
        {#if confirmClear}
          <span class="confirm" transition:fade={{ duration: 150 }}>
            <button class="danger-solid" onclick={clearAll}>{t("history.deleteAll")}</button>
            <button onclick={() => (confirmClear = false)}>{t("history.cancel")}</button>
          </span>
        {:else}
          <button class="pill-button" onclick={() => (confirmClear = true)}><Icon name="trash" size={13} /> {t("history.clear")}</button>
        {/if}
      {/if}
    </div>
  </div>

  <label class="search card">
    <Icon name="search" size={14} />
    <input placeholder={t("history.searchPlaceholder")} bind:value={query} spellcheck="false" />
    {#if query}<button class="icon-button" aria-label={t("history.clearSearch")} onclick={() => (query = "")}><Icon name="x" size={12} /></button>{/if}
  </label>

  {#if groups.length}
    <div class="groups">
      {#each groups as g (g.label)}
        <section class="section" transition:slide={{ duration: 260, easing: cubicInOut }}>
          <h3 class="day">{g.label}</h3>
          <div class="card list">
            {#each g.items as e (e.id)}
              <div class="entry" animate:flip={{ duration: 280, easing: cubicInOut }} out:slide={{ duration: 220, easing: cubicInOut }}>
                <button class="text" class:clamp={expanded !== e.id} class:lines={e.meeting} onclick={() => (expanded = expanded === e.id ? null : e.id)}>
                  {e.text}
                </button>
                <div class="meta">
                  {#if e.meeting}<span class="badge blue">{t("history.meeting")}</span>{/if}
                  <span>{time(e.ts)}</span>
                  {#if e.app}<span class="dot">·</span><span>{e.app}</span>{/if}
                  <span class="dot">·</span><span>{seconds(e.durationMs)}</span>
                  <span class="dot">·</span><span>{tn("history.words", e.words)}</span>
                  <span class="dot">·</span><span>{modelLabel(e.model)}</span>
                  <span class="spacer"></span>
                  <span class="tools">
                    {#if e.meeting}
                      <button class="icon-button" title={t("history.exportMd")} onclick={() => exportMd(e)}><Icon name="share" size={13} /></button>
                    {/if}
                    <button class="icon-button" title={t("history.copy")} onclick={() => copy(e)}>
                      {#if copied === e.id}<span class="ok" in:fade={{ duration: 120 }}><Icon name="check" size={13} stroke={2.4} /></span>{:else}<Icon name="copy" size={13} />{/if}
                    </button>
                    <button class="icon-button del" title={t("history.delete")} onclick={() => remove(e)}><Icon name="trash" size={13} /></button>
                  </span>
                </div>
              </div>
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {:else}
    <div class="empty card" in:fade={{ duration: 200 }}>
      <span class="empty-icon"><Icon name="clock" size={22} /></span>
      {#if query}
        <strong>{t("history.empty.noMatches")}</strong>
        <small>{t("history.empty.noMatchesSub", { query })}</small>
      {:else if !v.saveHistory}
        <strong>{t("history.empty.off")}</strong>
        <small>{t("history.empty.offSub")}</small>
      {:else}
        <strong>{t("history.empty.none")}</strong>
        <small>{t("history.empty.noneSub")}</small>
      {/if}
    </div>
  {/if}
</div>

<style>
  .head-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }
  .confirm {
    display: inline-flex;
    gap: 6px;
  }
  .confirm button {
    padding: 4px 10px;
    font-size: 12px;
  }
  .danger-solid {
    background: var(--red);
    border-color: transparent;
    color: #fff;
  }
  .danger-solid:hover {
    background: #e0352b;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 2px 6px 2px 13px;
    color: var(--text-2);
    transition:
      border-color 0.25s var(--ease),
      box-shadow 0.25s var(--ease);
  }
  .search:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 8px 0;
    box-shadow: none;
    user-select: text;
    -webkit-user-select: text;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .day {
    margin: 0 0 0 2px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-2);
    text-transform: capitalize;
  }
  .list {
    overflow: hidden;
  }
  .entry {
    padding: 12px 14px 8px 16px;
    background: var(--surface);
  }
  .entry + .entry {
    border-top: 1px solid var(--line);
  }
  .text {
    display: block;
    width: 100%;
    padding: 0;
    border: none;
    background: transparent;
    text-align: left;
    font-weight: 400;
    line-height: 1.5;
    color: var(--text);
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }
  .text:hover {
    background: transparent;
  }
  .text:active {
    transform: none;
  }
  /* Meeting transcripts keep their line breaks. */
  .lines {
    white-space: pre-wrap;
  }
  .clamp {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    cursor: pointer;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 4px;
    font-size: 11px;
    color: var(--text-3);
  }
  .dot {
    opacity: 0.6;
  }
  .spacer {
    flex: 1;
  }
  .tools {
    display: inline-flex;
    gap: 2px;
    opacity: 0;
    transform: translateX(4px);
    transition:
      opacity 0.2s var(--ease),
      transform 0.2s var(--ease);
  }
  .entry:hover .tools {
    opacity: 1;
    transform: none;
  }
  .tools .icon-button {
    width: 24px;
    height: 24px;
  }
  .del:hover {
    color: var(--red);
  }
  .ok {
    display: inline-flex;
    color: var(--green);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 36px 20px;
    text-align: center;
  }
  .empty small {
    color: var(--text-2);
  }
  .empty-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    margin-bottom: 4px;
    border-radius: 12px;
    background: linear-gradient(180deg, #b36bff, #8b4cf0);
    color: #fff;
  }
</style>
