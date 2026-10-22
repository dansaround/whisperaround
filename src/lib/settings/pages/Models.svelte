<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import Icon from "../ui/Icon.svelte";
  import { store } from "../store.svelte";
  import { MODELS, modelDescription, type ModelOption } from "../../models";
  import { local, formatBytes } from "../../localModels.svelte";
  import Select from "../ui/Select.svelte";
  import { cancelModelDownload, checkPython, downloadModel, gpuInfo, openPythonInstall, setModel, IS_MAC, IS_WINDOWS, DOWNLOAD_CANCELLED } from "../../ipc";
  import type { GpuInfo, LocalDevice } from "../../types";
  import { t, type Key } from "../../i18n/index.svelte";

  const v = $derived(store.view!);
  const activeMode = $derived(v.modes.find((m) => m.id === v.activeMode));
  let key = $state("");
  let savingKey = $state(false);
  /** undefined = checking, null = missing. */
  let python = $state<string | null | undefined>(undefined);

  let gpu = $state<GpuInfo | null>(null);
  // The GPU option is omitted (Select has no disabled items) unless a GPU exists or is already chosen.
  const deviceOptions = $derived([
    { value: "auto" as LocalDevice, label: t("models.device.auto") },
    { value: "cpu" as LocalDevice, label: t("models.device.cpu") },
    ...(gpu?.name || v.localDevice === "cuda" ? [{ value: "cuda" as LocalDevice, label: t("models.device.cuda") }] : []),
  ]);
  const deviceNote = $derived(
    !gpu ? "" : !gpu.name ? t("models.device.noGpu") : v.localDevice === "auto" ? t("models.device.autoSub") : !gpu.cudaReady ? t("models.device.download") : t("models.device.ready"),
  );

  onMount(() => {
    if (!IS_MAC) gpuInfo().then((g) => (gpu = g)).catch(() => {});
    checkPython()
      .then((p) => (python = p))
      .catch(() => (python = null));
  });

  async function saveKey() {
    if (!key.trim()) return;
    savingKey = true;
    if (await store.patch({ openaiApiKey: key })) {
      key = "";
      store.notify(t(IS_WINDOWS ? "models.openai.keySavedWindows" : "models.openai.keySaved"));
    }
    savingKey = false;
  }

  async function use(m: ModelOption) {
    try {
      await setModel(m.id);
    } catch (e) {
      store.notify(String(e), "error");
    }
  }

  async function download(m: ModelOption) {
    try {
      await downloadModel(m.id);
      store.notify(t("models.card.ready", { model: m.label }));
    } catch (e) {
      if (!String(e).includes(DOWNLOAD_CANCELLED)) store.notify(String(e), "error");
    }
  }

  // Dictionary keys; t() runs in the template so they follow the UI language.
  const groups: { title: Key; sub: Key; local: boolean }[] = [
    { title: "models.groups.cloud", sub: "models.groups.cloudSub", local: false },
    { title: "models.groups.local", sub: "models.groups.localSub", local: true },
  ];
</script>

<div class="page">
  <section class="section">
    <h3 class="section-title">OpenAI</h3>
    <div class="card">
      <div class="row tall">
        <div class="row-text">
          <strong class="help">
            {t("models.openai.apiKey")}
            {#if v.apiKeyConfigured}
              <span class="badge green" in:fade><Icon name="check" size={10} stroke={3} /> {t("models.openai.configured")}</span>
            {:else}
              <span class="badge orange" in:fade>{t("models.openai.missing")}</span>
            {/if}
          </strong>
          <small>{t(IS_WINDOWS ? "models.openai.keySubWindows" : "models.openai.keySub")}</small>
        </div>
        <form class="control key" onsubmit={(e) => (e.preventDefault(), saveKey())}>
          <span class="key-input">
            <Icon name="key" size={13} />
            <input type="password" autocomplete="off" placeholder={v.apiKeyConfigured ? t("models.openai.replacePlaceholder") : "sk-…"} bind:value={key} />
          </span>
          <button class="primary" type="submit" disabled={!key.trim() || savingKey}>{savingKey ? t("models.openai.saving") : t("models.openai.save")}</button>
        </form>
      </div>
    </div>
  </section>

  <p class="section-sub active-note">
    {t("models.activeNote")}
    {#if activeMode}<span class="badge blue"><Icon name={activeMode.icon} size={10} /> {activeMode.name}</span>{/if}
  </p>

  {#each groups as g (g.title)}
    <section class="section">
      <div class="group-head">
        <h3 class="section-title">{t(g.title)}</h3>
        <span class="muted">{t(g.sub)}</span>
        {#if g.local}
          <span class="spacer"></span>
          {#if python === undefined}
            <span class="badge">{t("models.python.checking")}</span>
          {:else if python}
            <span class="badge green">{python}</span>
          {:else}
            <span class="badge orange">{t("models.python.notFound")}</span>
            <button class="pill-button small" onclick={() => openPythonInstall()}>{IS_WINDOWS ? t("models.python.microsoftStore") : t("models.python.getPython")}</button>
          {/if}
        {/if}
      </div>
      {#if g.local && !IS_MAC}
        <div class="card">
          <div class="row tall">
            <div class="row-text">
              <strong class="help">
                {t("models.device.title")}
                {#if gpu?.name}<span class="badge green">{gpu.name}</span>{/if}
              </strong>
              <small>{t("models.device.sub")}{deviceNote ? ` · ${deviceNote}` : ""}</small>
            </div>
            <div class="control">
              <Select value={v.localDevice} options={deviceOptions} align="right" onchange={(localDevice) => store.patch({ localDevice })} />
            </div>
          </div>
        </div>
      {/if}
      <div class="grid">
        {#each MODELS.filter((m) => m.local === g.local && local.available(m)) as m (m.id)}
          {@const active = m.id === v.model}
          {@const dl = m.download ? local.info(m.id) : undefined}
          {@const progress = dl?.downloading && local.progress?.id === m.id ? local.progress : null}
          <div class="model card" class:active>
            <div class="model-head">
              <span class="logo" class:local={m.local}><Icon name={m.local ? "chip" : "cloud"} size={14} stroke={2} /></span>
              <strong>{m.label}</strong>
            </div>
            <p>{modelDescription(m.id)}</p>
            <div class="meters">
              <span class="meter"><em>{t("models.card.speed")}</em>{#each [1, 2, 3, 4, 5] as i (i)}<i class:on={i <= m.speed}></i>{/each}</span>
              <span class="meter"><em>{t("models.card.accuracy")}</em>{#each [1, 2, 3, 4, 5] as i (i)}<i class:on={i <= m.accuracy}></i>{/each}</span>
            </div>
            {#if dl?.downloading}
              {@const engine = !progress || progress.phase === "engine"}
              <div class="download" in:fade={{ duration: 150 }}>
                <div class="bar" class:indeterminate={engine}>
                  <i style:width={engine ? null : `${(progress.received / progress.total) * 100}%`}></i>
                </div>
                <div class="download-meta">
                  <span>{engine ? t("models.card.preparingEngine") : t("models.card.progress", { received: formatBytes(progress.received), total: formatBytes(progress.total) })}</span>
                  <button class="link" onclick={() => cancelModelDownload()}>{t("models.card.cancel")}</button>
                </div>
              </div>
            {:else}
              <div class="model-foot">
                {#if dl && !dl.downloaded}<span class="badge">{formatBytes(dl.size)}</span>{:else if m.local}<span class="badge"><Icon name="lock" size={10} /> {t("models.card.offline")}</span>{:else}<span class="badge"><Icon name="globe" size={10} /> {t("models.card.cloud")}</span>{/if}
                {#if dl && !dl.downloaded}
                  <button class="pill-button small" disabled={local.list.some((x) => x.downloading)} onclick={() => download(m)}><Icon name="download" size={11} /> {t("models.card.download")}</button>
                {:else if active}
                  <span class="badge green"><Icon name="check" size={10} stroke={3} /> {t("models.card.active")}</span>
                {:else}
                  <button class="pill-button small" onclick={() => use(m)}>{t("models.card.use")}</button>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .key {
    gap: 8px;
  }
  .key-input {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
    color: var(--text-2);
    transition:
      border-color 0.2s var(--ease),
      box-shadow 0.2s var(--ease);
  }
  .key-input:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .key-input input {
    width: 190px;
    border: none;
    background: transparent;
    padding: 6px 0;
    box-shadow: none;
    user-select: text;
    -webkit-user-select: text;
  }
  .active-note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: -8px 0 -8px 2px;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .group-head .muted {
    font-size: 11.5px;
  }
  .spacer {
    flex: 1;
  }
  .small {
    padding: 3px 10px;
    font-size: 11.5px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 12px;
  }
  .model {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    transition:
      border-color 0.3s var(--ease),
      box-shadow 0.3s var(--ease),
      transform 0.3s var(--ease);
  }
  .model:hover {
    transform: translateY(-2px);
    box-shadow: var(--shadow);
  }
  .model.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .model-head {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .model-head strong {
    font-weight: 600;
  }
  .logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: #111;
    color: #fff;
  }
  .logo.local {
    background: linear-gradient(135deg, #f06292, #7c4dff);
  }
  .model p {
    margin: 0;
    flex: 1;
    color: var(--text-2);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .meters {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .meter em {
    width: 60px;
    font-style: normal;
    font-size: 10.5px;
    color: var(--text-3);
  }
  .meter i {
    width: 14px;
    height: 4px;
    border-radius: 2px;
    background: var(--track);
  }
  .meter i.on {
    background: var(--accent);
  }
  .model-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 2px;
  }
  .model-foot .small {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .download {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 4px;
  }
  .bar {
    position: relative;
    height: 4px;
    border-radius: 2px;
    background: var(--track);
    overflow: hidden;
  }
  .bar i {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.2s linear;
  }
  .bar.indeterminate i {
    width: 30%;
    animation: slide 1.2s var(--ease) infinite;
  }
  @keyframes slide {
    from {
      left: -30%;
    }
    to {
      left: 100%;
    }
  }
  .download-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    font-size: 10.5px;
    color: var(--text-2);
    cursor: pointer;
  }
  .link:hover {
    color: var(--text);
  }
</style>
