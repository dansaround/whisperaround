<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import Keys from "../ui/Keys.svelte";
  import Select from "../ui/Select.svelte";
  import CountUp from "../ui/CountUp.svelte";
  import { store, type Page } from "../store.svelte";
  import { openReleases, toggleRecording } from "../../ipc";
  import { shortcutText } from "../../shortcutFormat";
  import { t, tn, intlLocale, type Key } from "../../i18n/index.svelte";

  type Range = "all" | "week" | "today";
  let range = $state<Range>("all");

  /** Typing speed assumed for "time saved" (average keyboard typist). */
  const TYPING_WPM = 40;

  const stats = $derived.by(() => {
    const h = store.history;
    if (range === "all") {
      return { words: h.stats.words, audioMs: h.stats.audioMs, apps: h.stats.apps.length, sessions: h.stats.sessions };
    }
    const since = range === "today" ? new Date().setHours(0, 0, 0, 0) : Date.now() - 7 * 86_400_000;
    const list = h.entries.filter((e) => e.ts >= since);
    return {
      words: list.reduce((n, e) => n + e.words, 0),
      audioMs: list.reduce((n, e) => n + e.durationMs, 0),
      apps: new Set(list.map((e) => e.app).filter(Boolean)).size,
      sessions: list.length,
    };
  });
  const wpm = $derived(stats.audioMs > 0 ? stats.words / (stats.audioMs / 60_000) : 0);
  const savedMin = $derived(Math.max(0, stats.words / TYPING_WPM - stats.audioMs / 60_000));

  const formatInt = (n: number) => Math.round(n).toLocaleString(intlLocale());

  function formatSaved(min: number): string {
    if (min < 1) return tn("home.time.minutes", 0);
    if (min < 60) return tn("home.time.minutes", Math.round(min));
    if (min < 60 * 24) return tn("home.time.hours", Math.round(min / 60));
    return tn("home.time.days", Math.round(min / 60 / 24));
  }

  const RANGE_LABEL: Record<Range, Key> = { all: "home.range.all", week: "home.range.week", today: "home.range.today" };
  const SAVED_LABEL: Record<Range, Key> = { all: "home.savedIn.all", week: "home.savedIn.week", today: "home.savedIn.today" };

  function share() {
    const text = t("home.shareText", {
      range: t(RANGE_LABEL[range]),
      wpm: formatInt(wpm),
      words: tn("home.shareWords", stats.words),
      saved: formatSaved(savedMin),
    });
    navigator.clipboard.writeText(text).then(
      () => store.notify(t("home.statsCopied")),
      () => store.notify(t("home.copyFailed"), "error"),
    );
  }

  const STEPS: { id: string; title: Key; sub: Key; icon: string; go?: Page; action?: () => void; keys?: boolean }[] = [
    { id: "record", title: "home.steps.record.title", sub: "home.steps.record.sub", icon: "record", action: () => toggleRecording(), keys: true },
    { id: "shortcuts", title: "home.steps.shortcuts.title", sub: "home.steps.shortcuts.sub", icon: "pointer", go: "configuration" },
    { id: "mode", title: "home.steps.mode.title", sub: "home.steps.mode.sub", icon: "sparkle", go: "modes" },
    { id: "vocabulary", title: "home.steps.vocabulary.title", sub: "home.steps.vocabulary.sub", icon: "book", go: "vocabulary" },
  ];

  /** Release date of the entries below (month is 0-based). */
  const CHANGES_DATE = new Date(2026, 9, 16);
  const CHANGES: { id: string; title: Key; body: Key; params?: Record<string, string> }[] = [
    { id: "settingsWindow", title: "home.changes.settingsWindow.title", body: "home.changes.settingsWindow.body" },
    { id: "modesVocabulary", title: "home.changes.modesVocabulary.title", body: "home.changes.modesVocabulary.body", params: { keys: shortcutText("Alt+Shift+KeyK") } },
  ];
  const changesDate = $derived(CHANGES_DATE.toLocaleDateString(intlLocale(), { month: "short", day: "numeric" }));
</script>

<div class="page">
  <div class="page-head">
    <Select
      variant="ghost"
      value={range}
      options={[
        { value: "all", label: t("home.range.all") },
        { value: "week", label: t("home.range.week") },
        { value: "today", label: t("home.range.today") },
      ]}
      minWidth={150}
      onchange={(r) => (range = r)}
    />
    <button class="icon-button" title={t("home.copyStats")} onclick={share}><Icon name="share" size={15} /></button>
  </div>

  <div class="card stats">
    <div class="stat">
      <strong><CountUp value={wpm} format={formatInt} /> {t("home.stats.wpmUnit")}</strong>
      <span>{t("home.stats.averageSpeed")}</span>
    </div>
    <div class="stat">
      <strong><CountUp value={stats.words} format={formatInt} /></strong>
      <span>{t("home.stats.words")}</span>
    </div>
    <div class="stat">
      <strong><CountUp value={stats.apps} format={formatInt} /></strong>
      <span>{t("home.stats.appsUsed")}</span>
    </div>
    <div class="stat">
      <strong><CountUp value={savedMin} format={formatSaved} /></strong>
      <span class="saved" title={t("home.stats.savedTooltip", { wpm: TYPING_WPM })}>
        {t(SAVED_LABEL[range])} <Icon name="gear" size={11} />
      </span>
    </div>
  </div>

  <section class="section">
    <h3 class="section-title">{t("home.getStarted")}</h3>
    <div class="steps">
      {#each STEPS as s (s.id)}
        <button class="step" onclick={() => (s.go ? (store.page = s.go) : s.action?.())}>
          <span class="step-icon"><Icon name={s.icon} size={15} /></span>
          <span class="row-text">
            <strong>{t(s.title)}</strong>
            <small>{t(s.sub)}</small>
          </span>
          {#if s.keys && store.view}
            <Keys binding={store.view.shortcut} />
          {:else}
            <span class="chev"><Icon name="chevron-right" size={14} /></span>
          {/if}
        </button>
      {/each}
    </div>
  </section>

  <section class="section">
    <div class="page-head">
      <h3 class="section-title">{t("home.whatsNew")}</h3>
      <button class="link" onclick={() => openReleases()}>{t("home.viewAllChanges")}</button>
    </div>
    <div class="card news">
      {#each CHANGES as c (c.id)}
        <div class="news-item">
          <span class="date">{changesDate}</span>
          <div class="row-text">
            <strong>{t(c.title)}</strong>
            <small>{t(c.body, c.params)}</small>
          </div>
        </div>
      {/each}
    </div>
  </section>
</div>

<style>
  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    padding: 16px 18px;
    gap: 12px;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .stat strong {
    font-size: 14px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .stat span {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .saved {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .steps {
    display: flex;
    flex-direction: column;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 9px 10px;
    margin: 0 -10px;
    border: none;
    border-radius: 10px;
    background: transparent;
    text-align: left;
    font-weight: 400;
  }
  .step:hover {
    background: var(--selected);
  }
  .step .row-text {
    flex: 1;
  }
  .step-icon {
    display: inline-flex;
    width: 22px;
    justify-content: center;
    color: var(--text-2);
  }
  .chev {
    color: var(--text-3);
    opacity: 0;
    transform: translateX(-4px);
    transition:
      opacity 0.25s var(--ease),
      transform 0.25s var(--ease);
  }
  .step:hover .chev {
    opacity: 1;
    transform: none;
  }
  .link {
    padding: 2px 4px;
    border: none;
    background: transparent;
    color: var(--text);
    font-weight: 500;
    font-size: 12px;
  }
  .link:hover {
    background: transparent;
    color: var(--accent);
  }
  .news {
    display: flex;
    flex-direction: column;
  }
  .news-item {
    display: flex;
    gap: 22px;
    padding: 14px 18px;
  }
  .news-item + .news-item {
    border-top: 1px solid var(--line);
  }
  .date {
    flex: none;
    min-width: 48px;
    white-space: nowrap;
    color: var(--text-2);
    font-size: 12px;
  }
  .news .row-text strong {
    font-weight: 600;
  }
</style>
