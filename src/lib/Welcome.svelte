<script lang="ts">
  import { onMount } from "svelte";
  import {
    checkPython,
    openPythonInstall,
    finishOnboarding,
    openSettings,
    checkPermissions,
    requestPermission,
    loadSettings,
    updateSettings,
    IS_MAC,
  } from "./ipc";
  import type { Permissions } from "./types";
  import { t, setLocale, LOCALES, i18n, type Locale } from "./i18n/index.svelte";
  import Select from "./settings/ui/Select.svelte";

  /** UI language, seeded from settings; applied immediately and persisted. */
  let uiLanguage = $state<Locale>(i18n.locale);
  const languageOptions = LOCALES.map((l) => ({ value: l.code, label: l.label }));

  function chooseLanguage(code: Locale) {
    uiLanguage = code;
    setLocale(code);
    updateSettings({ uiLanguage: code }).catch((e) => console.error("updateSettings failed", e));
  }

  /** `undefined` while checking, `null` if missing, else the version string. */
  let python = $state<string | null | undefined>(undefined);
  let storeOpened = $state(false);

  async function recheck() {
    python = undefined;
    try {
      python = await checkPython();
    } catch (e) {
      console.error("checkPython failed", e);
      python = null;
    }
  }

  /** macOS only (null elsewhere); polled so toggles made in System Settings show up. */
  let perms = $state<Permissions | null>(null);

  async function refreshPermissions() {
    try {
      perms = await checkPermissions();
    } catch (e) {
      console.error("checkPermissions failed", e);
    }
  }

  async function install() {
    await openPythonInstall();
    storeOpened = true;
  }

  async function configure() {
    await openSettings();
    await finishOnboarding();
  }

  onMount(() => {
    loadSettings()
      .then((v) => {
        const code = LOCALES.find((l) => l.code === v.uiLanguage)?.code;
        if (code) uiLanguage = code;
      })
      .catch((e) => console.error("loadSettings failed", e));
    recheck();
    refreshPermissions();
    const timer = setInterval(refreshPermissions, 1500);
    return () => clearInterval(timer);
  });
</script>

<div class="welcome">
  <header>
    <div class="title-row">
      <h1>{t("welcome.title")}</h1>
      <div class="lang" title={t("welcome.language")}>
        <Select value={uiLanguage} options={languageOptions} onchange={chooseLanguage} variant="ghost" align="right" minWidth={140} />
      </div>
    </div>
    <p>{t("welcome.subtitle")}</p>
  </header>

  {#if perms}
    <section class="check" class:ok={perms.microphone === "granted"} class:missing={perms.microphone === "denied"}>
      <div class="check-head">
        <span class="dot" aria-hidden="true"></span>
        <strong>{t("welcome.microphone.title")}</strong>
        <span class="state">
          {#if perms.microphone === "granted"}{t("welcome.state.granted")}{:else if perms.microphone === "denied"}{t("welcome.state.denied")}{:else}{t("welcome.state.notRequested")}{/if}
        </span>
      </div>
      {#if perms.microphone !== "granted"}
        <p>{t("welcome.microphone.body")}</p>
        <div class="row">
          <button class="primary" onclick={() => requestPermission("microphone")}>
            {perms.microphone === "denied" ? t("welcome.microphone.openSystemSettings") : t("welcome.microphone.allow")}
          </button>
        </div>
      {/if}
    </section>

    <section class="check" class:ok={perms.accessibility} class:missing={!perms.accessibility}>
      <div class="check-head">
        <span class="dot" aria-hidden="true"></span>
        <strong>{t("welcome.accessibility.title")}</strong>
        <span class="state">{perms.accessibility ? t("welcome.state.granted") : t("welcome.state.missing")}</span>
      </div>
      {#if !perms.accessibility}
        <p>
          {t("welcome.accessibility.bodyBefore")} <b>WhisperAround</b>
          {t("welcome.accessibility.bodyAfter")}
        </p>
        <div class="row">
          <button class="primary" onclick={() => requestPermission("accessibility")}>{t("welcome.accessibility.allow")}</button>
        </div>
      {/if}
    </section>
  {/if}

  <section class="check" class:ok={!!python} class:missing={python === null}>
    <div class="check-head">
      <span class="dot" aria-hidden="true"></span>
      <strong>Python 3</strong>
      <span class="state">
        {#if python === undefined}{t("welcome.state.checking")}{:else if python}{python}{:else}{t("welcome.state.notFound")}{/if}
      </span>
    </div>

    {#if python === null}
      <p>
        {t("welcome.python.neededBefore")} <b>{t("welcome.python.neededBold")}</b> {t("welcome.python.neededAfter")}
        {#if IS_MAC}
          {t("welcome.python.installMacBefore")} <code>brew install python</code>{t("welcome.python.installMacAfter")}
        {:else}
          {t("welcome.python.installWindows")}
        {/if}
      </p>
      <div class="row">
        <button class="primary" onclick={install}>{IS_MAC ? t("welcome.python.openPythonOrg") : t("welcome.python.openMicrosoftStore")}</button>
        <button onclick={recheck}>{t("welcome.python.checkAgain")}</button>
      </div>
      {#if storeOpened}
        <small>{t("welcome.python.afterInstall")}</small>
      {/if}
    {:else if python}
      <p>{t("welcome.python.ready")}</p>
    {/if}
  </section>

  <p class="note">
    {t("welcome.cloudNote")}
  </p>

  <footer>
    <button onclick={configure}>{t("welcome.openSettings")}</button>
    <button class="primary" onclick={finishOnboarding}>{t("welcome.continue")}</button>
  </footer>
</div>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    max-width: 380px;
    margin: 0 auto;
  }
  .title-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.6rem;
  }
  .lang {
    flex: none;
    margin: -0.15rem -0.4rem 0 0;
    font-size: 0.85rem;
  }
  h1 {
    margin: 0 0 0.3rem;
    font-size: 1.35rem;
  }
  header p,
  .note {
    margin: 0;
    color: var(--text-2);
    font-size: 0.9rem;
  }
  .check {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    padding: 0.9rem 1rem;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
  }
  .check.ok {
    border-color: rgba(74, 222, 128, 0.45);
  }
  .check.missing {
    border-color: rgba(251, 191, 36, 0.5);
  }
  .check p {
    margin: 0;
    font-size: 0.88rem;
    color: var(--text);
  }
  .check small {
    color: var(--text-2);
    font-size: 0.78rem;
  }
  .check-head {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }
  .state {
    margin-left: auto;
    color: var(--text-2);
    font-size: 0.85rem;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .ok .dot {
    background: #4ade80;
  }
  .missing .dot {
    background: #fbbf24;
  }
  .row,
  footer {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  footer {
    justify-content: flex-end;
    margin-top: 0.4rem;
  }
</style>
