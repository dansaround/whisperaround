<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { store } from "../store.svelte";
  import { t, tn } from "../../i18n/index.svelte";

  const v = $derived(store.view!);
</script>

<div class="page">
  <div class="page-head">
    <h2>
      {t("vocabulary.title")}
      <span class="badge">{t("vocabulary.soon.badge")}</span>
    </h2>
    {#if v.vocabulary.length}
      <span class="muted count">{tn("vocabulary.count", v.vocabulary.length)}</span>
    {/if}
  </div>

  <div class="card soon">
    <span class="soon-icon"><Icon name="lock" size={18} /></span>
    <div class="row-text">
      <strong>{t("vocabulary.soon.title")}</strong>
      <small>{t("vocabulary.soon.body")}</small>
    </div>
  </div>

  {#if v.vocabulary.length}
    <p class="section-sub kept">{t("vocabulary.soon.kept")}</p>
    <div class="chips">
      {#each v.vocabulary as w (w)}
        <span class="chip">{w}</span>
      {/each}
    </div>
  {/if}

  <p class="section-sub">
    {t("vocabulary.soon.footnote")}
  </p>
</div>

<style>
  .count {
    font-size: 12px;
  }
  .soon {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 16px 18px;
  }
  .soon-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 10px;
    background: var(--selected);
    color: var(--text-2);
  }
  .soon .row-text {
    gap: 4px;
    padding-top: 2px;
  }
  .soon small {
    line-height: 1.45;
  }
  .kept {
    margin-bottom: -12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    padding: 5px 11px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    font-weight: 500;
    color: var(--text-2);
  }
</style>
