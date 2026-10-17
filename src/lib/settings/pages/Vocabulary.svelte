<script lang="ts">
  import { scale, fade } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicInOut } from "svelte/easing";
  import Icon from "../ui/Icon.svelte";
  import { store } from "../store.svelte";
  import { t, tn } from "../../i18n/index.svelte";

  const v = $derived(store.view!);
  let draft = $state("");
  let input: HTMLInputElement;

  /** Accept one term or a pasted list (commas / new lines). */
  function add() {
    const terms = draft
      .split(/[,\n]/)
      .map((t) => t.trim())
      .filter(Boolean);
    if (!terms.length) return;
    const existing = new Set(v.vocabulary.map((w) => w.toLowerCase()));
    const fresh = terms.filter((t) => !existing.has(t.toLowerCase()));
    draft = "";
    if (fresh.length) store.patch({ vocabulary: [...v.vocabulary, ...fresh] });
    input.focus();
  }

  function remove(word: string) {
    store.patch({ vocabulary: v.vocabulary.filter((w) => w !== word) });
  }

  const SUGGESTIONS = ["WhisperAround", "Kubernetes", "PostgreSQL", "TypeScript", "OKR"];
</script>

<div class="page">
  <div class="page-head">
    <h2>
      {t("vocabulary.title")}
      <span class="info" title={t("vocabulary.info")}>
        <Icon name="info" size={13} />
      </span>
    </h2>
    {#if v.vocabulary.length}
      <span class="muted count">{tn("vocabulary.count", v.vocabulary.length)}</span>
    {/if}
  </div>

  <form class="adder card" onsubmit={(e) => (e.preventDefault(), add())}>
    <Icon name="plus" size={15} />
    <input bind:this={input} bind:value={draft} placeholder={t("vocabulary.placeholder")} autocomplete="off" spellcheck="false" />
    <button type="submit" class="primary" disabled={!draft.trim()}>{t("vocabulary.add")}</button>
  </form>

  {#if v.vocabulary.length}
    <div class="chips">
      {#each v.vocabulary as w (w)}
        <span class="chip" animate:flip={{ duration: 300, easing: cubicInOut }} in:scale={{ duration: 260, start: 0.7, easing: cubicInOut }} out:scale={{ duration: 180, start: 0.7 }}>
          {w}
          <button class="x" aria-label={t("vocabulary.remove", { word: w })} onclick={() => remove(w)}><Icon name="x" size={11} stroke={2.4} /></button>
        </span>
      {/each}
    </div>
  {:else}
    <div class="empty card" in:fade={{ duration: 200 }}>
      <span class="empty-icon"><Icon name="book" size={22} /></span>
      <strong>{t("vocabulary.empty.title")}</strong>
      <small>{t("vocabulary.empty.sub")}</small>
      <div class="suggest">
        {#each SUGGESTIONS as s (s)}
          <button class="pill-button" onclick={() => store.patch({ vocabulary: [...v.vocabulary, s] })}>{s}</button>
        {/each}
      </div>
    </div>
  {/if}

  <p class="section-sub">
    {t("vocabulary.footnote")}
  </p>
</div>

<style>
  .count {
    font-size: 12px;
  }
  .adder {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 14px;
    color: var(--text-2);
    transition:
      border-color 0.25s var(--ease),
      box-shadow 0.25s var(--ease);
  }
  .adder:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .adder input {
    flex: 1;
    border: none;
    background: transparent;
    padding: 6px 0;
    box-shadow: none;
    user-select: text;
    -webkit-user-select: text;
  }
  .adder input:focus {
    box-shadow: none;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 6px 5px 11px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    font-weight: 500;
  }
  .x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: var(--selected);
    color: var(--text-2);
  }
  .x:hover {
    background: var(--red);
    color: #fff;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 28px 20px;
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
    background: linear-gradient(180deg, #4f8fff, #2f6bf0);
    color: #fff;
  }
  .suggest {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
    margin-top: 6px;
  }
</style>
