<script lang="ts">
  import { slide, fly, fade } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicInOut } from "svelte/easing";
  import Icon from "../ui/Icon.svelte";
  import Keys from "../ui/Keys.svelte";
  import Select from "../ui/Select.svelte";
  import { store } from "../store.svelte";
  import { MODELS, transcriptionLanguages, isLocal, modelLabel } from "../../models";
  import { local } from "../../localModels.svelte";
  import type { Mode } from "../../types";
  import { t, td } from "../../i18n/index.svelte";

  const v = $derived(store.view!);
  let openId = $state<string | null>(null);
  let tipDismissed = $state(localStorage.getItem("wa.tip.modes") === "1");

  const ICONS = ["sparkle", "message", "laptop", "pen", "code", "globe", "bolt"];

  const modelOptions = $derived(
    MODELS.filter((m) => local.selectable(m)).map((m) => ({
      value: m.id,
      label: m.label,
      group: m.local ? t("modes.localGroup") : "OpenAI",
      icon: m.local ? "chip" : "cloud",
    })),
  );
  const languageOptions = $derived(transcriptionLanguages().map((l) => ({ value: l.code, label: l.label })));

  /** Tip text around the shortcut keycaps: [before, after]. */
  const tipParts = $derived.by(() => {
    const [before, after = ""] = t("modes.tip.body").split("{keys}");
    return [before, after];
  });

  function saveModes(modes: Mode[]) {
    return store.patch({ modes });
  }

  function edit(id: string, change: Partial<Mode>) {
    saveModes(v.modes.map((m) => (m.id === id ? { ...m, ...change } : m)));
  }

  function rename(m: Mode, name: string) {
    const clean = name.trim();
    if (clean && clean !== m.name) edit(m.id, { name: clean });
  }

  async function create() {
    const mode: Mode = {
      id: `mode-${Date.now().toString(36)}`,
      name: t("modes.newModeName"),
      icon: "sparkle",
      model: v.model,
      language: "",
    };
    if (await saveModes([...v.modes, mode])) openId = mode.id;
  }

  function remove(m: Mode) {
    if (v.modes.length < 2) return;
    openId = null;
    saveModes(v.modes.filter((x) => x.id !== m.id));
  }

  function dismissTip() {
    tipDismissed = true;
    localStorage.setItem("wa.tip.modes", "1");
  }
</script>

<div class="page">
  <div class="page-head">
    <h2>
      {t("modes.title")}
      <span class="info" title={t("modes.info")}>
        <Icon name="info" size={13} />
      </span>
    </h2>
    <button class="pill-button" onclick={create}><Icon name="plus" size={13} stroke={2.2} /> {t("modes.create")}</button>
  </div>

  <div class="list">
    {#each v.modes as m (m.id)}
      <div class="mode card" class:open={openId === m.id} animate:flip={{ duration: 320, easing: cubicInOut }} in:fly={{ y: 8, duration: 300, easing: cubicInOut }} out:fade={{ duration: 160 }}>
        <button class="mode-row" onclick={() => (openId = openId === m.id ? null : m.id)}>
          <span class="mode-icon"><Icon name={m.icon} size={14} /></span>
          <span class="name">{m.name}</span>
          {#if m.id === v.activeMode}<span class="active-dot" title={t("modes.activeMode")}></span>{/if}
          <span class="spacer"></span>
          {#if m.language}<span class="badge">{m.language.toUpperCase()}</span>{/if}
          <span class="engine" title={modelLabel(m.model)}>
            <span class="eng {isLocal(m.model) ? 'local' : 'cloud'}"><Icon name={isLocal(m.model) ? "chip" : "cloud"} size={12} stroke={2} /></span>
            <span class="eng-label">{modelLabel(m.model)}</span>
          </span>
          <span class="chev" class:rot={openId === m.id}><Icon name="chevron-down" size={14} /></span>
        </button>

        {#if openId === m.id}
          <div class="editor" transition:slide={{ duration: 300, easing: cubicInOut }}>
            <div class="grid">
              <label class="field">
                <span>{t("modes.field.name")}</span>
                <input
                  value={m.name}
                  maxlength="32"
                  onblur={(e) => rename(m, e.currentTarget.value)}
                  onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                />
              </label>
              <div class="field">
                <span>{t("modes.field.icon")}</span>
                <div class="icons">
                  {#each ICONS as i (i)}
                    <button class="icon-choice" class:selected={m.icon === i} aria-label={td(`modes.icons.${i}`)} onclick={() => edit(m.id, { icon: i })}>
                      <Icon name={i} size={14} />
                    </button>
                  {/each}
                </div>
              </div>
              <div class="field">
                <span>{t("modes.field.voiceModel")}</span>
                <Select value={m.model} options={modelOptions} minWidth={240} onchange={(model) => edit(m.id, { model })} />
              </div>
              <div class="field">
                <span>{t("modes.field.language")}</span>
                <Select value={m.language} options={languageOptions} minWidth={220} onchange={(language) => edit(m.id, { language })} />
              </div>
            </div>
            <div class="actions">
              <button class="danger" disabled={v.modes.length < 2} onclick={() => remove(m)}>
                <Icon name="trash" size={13} /> {t("modes.delete")}
              </button>
              {#if m.id === v.activeMode}
                <span class="badge green"><Icon name="check" size={11} stroke={2.6} /> {t("modes.active")}</span>
              {:else}
                <button class="primary" onclick={() => store.patch({ activeMode: m.id })}>{t("modes.useThisMode")}</button>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  {#if !tipDismissed}
    <div class="tip card" out:fly={{ y: 10, duration: 260, easing: cubicInOut }}>
      <span class="bulb"><Icon name="bulb" size={18} stroke={2} /></span>
      <div class="row-text">
        <strong>{t("modes.tip.title")}</strong>
        <small>
          {tipParts[0]}{#if v.modeShortcut}<Keys binding={v.modeShortcut} size="sm" />{:else}{t("modes.tip.noShortcut")}{/if}{tipParts[1]}
        </small>
      </div>
      <button class="dismiss" onclick={dismissTip}>{t("modes.tip.dismiss")}</button>
    </div>
  {/if}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .mode {
    overflow: hidden;
    transition:
      border-color 0.3s var(--ease),
      box-shadow 0.3s var(--ease);
  }
  .mode.open {
    border-color: var(--line-strong);
    box-shadow: var(--shadow);
  }
  .mode-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 50px;
    padding: 0 14px 0 16px;
    border: none;
    border-radius: 0;
    background: transparent;
    text-align: left;
  }
  .mode-row:hover {
    background: var(--surface-hover);
  }
  .mode-row:active {
    transform: none;
  }
  .mode-icon {
    display: inline-flex;
    color: var(--text-2);
  }
  .name {
    font-weight: 500;
  }
  .active-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #7ee36b;
    box-shadow: 0 0 0 0 rgba(126, 227, 107, 0.6);
    animation: live 2.2s var(--ease) infinite;
  }
  @keyframes live {
    50% {
      box-shadow: 0 0 0 4px rgba(126, 227, 107, 0);
    }
  }
  .spacer {
    flex: 1;
  }
  .engine {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px 3px 3px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .eng {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: #111;
    color: #fff;
  }
  .eng.local {
    background: linear-gradient(135deg, #f06292, #7c4dff);
  }
  .eng-label {
    font-size: 11px;
    color: var(--text-2);
  }
  .chev {
    display: inline-flex;
    color: var(--text-3);
    transition: transform 0.3s var(--ease);
  }
  .chev.rot {
    transform: rotate(180deg);
  }
  .editor {
    border-top: 1px solid var(--line);
    background: color-mix(in srgb, var(--surface-2) 40%, transparent);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px 18px;
    padding: 16px 16px 4px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field > span {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-2);
  }
  .icons {
    display: flex;
    gap: 4px;
  }
  .icon-choice {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text-2);
  }
  .icon-choice.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px 14px;
  }
  .danger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border-color: transparent;
    color: var(--red);
  }
  .danger:hover:not(:disabled) {
    background: rgba(255, 69, 58, 0.12);
  }
  .tip {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 16px;
  }
  .tip .row-text {
    flex: 1;
  }
  .tip small {
    line-height: 1.6;
  }
  .bulb {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 38px;
    height: 38px;
    border-radius: 10px;
    background: linear-gradient(180deg, #ffd84d, #f5b300);
    color: #fff;
  }
  .dismiss {
    align-self: flex-start;
    padding: 2px 6px;
    border: none;
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
  }
  .dismiss:hover {
    background: transparent;
    color: var(--text);
  }
</style>
