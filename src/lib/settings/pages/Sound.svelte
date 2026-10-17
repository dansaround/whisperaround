<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import Toggle from "../ui/Toggle.svelte";
  import Slider from "../ui/Slider.svelte";
  import { store } from "../store.svelte";
  import { previewSound } from "../../ipc";
  import type { SoundName, StartSound } from "../../types";
  import { t, intlLocale, type Key } from "../../i18n/index.svelte";

  const v = $derived(store.view!);
  /** Live value while dragging; committed on release. */
  let dragVolume = $state<number | null>(null);
  const volume = $derived(dragVolume ?? v.soundVolume);
  let playing = $state<string | null>(null);
  let playTimer: ReturnType<typeof setTimeout> | undefined;

  // Text fields hold dictionary keys; t() runs in the template so they follow the UI language.
  const STARTS: { id: StartSound; name: string; blurb: Key }[] = [
    { id: "orus", name: "Orus", blurb: "sound.start.blurbs.orus" },
    { id: "loto", name: "Loto", blurb: "sound.start.blurbs.loto" },
    { id: "okaso", name: "Okaso", blurb: "sound.start.blurbs.okaso" },
  ];

  const EVENTS: { id: SoundName; title: Key; sub: Key; icon: string }[] = [
    { id: "start", title: "sound.preview.events.start", sub: "sound.preview.events.startSub", icon: "record" },
    { id: "stop", title: "sound.preview.events.stop", sub: "sound.preview.events.stopSub", icon: "mic" },
    { id: "error", title: "sound.preview.events.error", sub: "sound.preview.events.errorSub", icon: "warning" },
  ];

  const level = () => Math.max(volume, 0.05);

  function flash(key: string, ms = 700) {
    playing = key;
    clearTimeout(playTimer);
    playTimer = setTimeout(() => (playing = null), ms);
  }

  function play(sound: SoundName) {
    previewSound(v.startSound, sound, level());
    flash(sound);
  }

  function chooseStart(id: StartSound) {
    store.patch({ startSound: id });
    previewSound(id, "start", level());
    flash(`start:${id}`, 1200);
  }

  function commitVolume(x: number) {
    dragVolume = null;
    store.patch({ soundVolume: x });
    previewSound(v.startSound, "start", x);
  }
</script>

<div class="page">
  <section class="section">
    <h3 class="section-title">{t("sound.effects.title")}</h3>
    <div class="card">
      <div class="row">
        <div class="row-text">
          <strong>{t("sound.effects.play")}</strong>
          <small>{t("sound.effects.playSub")}</small>
        </div>
        <div class="control">
          <Toggle label={t("sound.effects.play")} checked={v.soundsEnabled} onchange={(c) => store.patch({ soundsEnabled: c })} />
        </div>
      </div>
      <div class="row" class:disabled={!v.soundsEnabled}>
        <div class="row-text"><strong>{t("sound.effects.volume")}</strong></div>
        <div class="control volume">
          <button class="icon-button" title={t("sound.effects.mute")} onclick={() => commitVolume(0)}>
            <Icon name={volume === 0 ? "speaker-off" : "speaker-low"} size={15} />
          </button>
          <Slider label={t("sound.effects.volume")} value={volume} oninput={(x) => (dragVolume = x)} onchange={commitVolume} />
          <button class="icon-button" title={t("sound.effects.max")} onclick={() => commitVolume(1)}><Icon name="speaker" size={15} /></button>
          <span class="pct">{volume.toLocaleString(intlLocale(), { style: "percent", maximumFractionDigits: 0 })}</span>
        </div>
      </div>
    </div>
  </section>

  <section class="section" class:off={!v.soundsEnabled}>
    <h3 class="section-title">{t("sound.start.title")}</h3>
    <p class="section-sub">{t("sound.start.sub")}</p>
    <div class="starts">
      {#each STARTS as st (st.id)}
        <button class="start" class:selected={v.startSound === st.id} class:playing={playing === `start:${st.id}`} onclick={() => chooseStart(st.id)}>
          <span class="start-icon"><span class="eq" aria-hidden="true"><i></i><i></i><i></i><i></i></span></span>
          <span class="start-text">
            <strong>{st.name}</strong>
            <small>{t(st.blurb)}</small>
          </span>
          {#if st.id === "orus"}<span class="badge blue default">{t("sound.start.default")}</span>{/if}
        </button>
      {/each}
    </div>
  </section>

  <section class="section" class:off={!v.soundsEnabled}>
    <h3 class="section-title">{t("sound.preview.title")}</h3>
    <div class="card">
      {#each EVENTS as e (e.id)}
        <div class="row">
          <div class="row-text ev">
            <span class="ev-icon {e.id}"><Icon name={e.icon} size={13} stroke={2} /></span>
            <span class="row-text">
              <strong>{t(e.title)}</strong>
              <small>{t(e.sub)}</small>
            </span>
          </div>
          <div class="control">
            <button class="play" class:on={playing === e.id} aria-label={t("sound.preview.play", { name: t(e.title) })} onclick={() => play(e.id)}>
              <Icon name="play" size={11} />
            </button>
          </div>
        </div>
      {/each}
    </div>
  </section>
</div>

<style>
  .volume {
    width: 300px;
    gap: 6px;
  }
  .pct {
    width: 36px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
    font-size: 12px;
  }
  .section.off {
    opacity: 0.55;
    transition: opacity 0.3s var(--ease);
  }

  .starts {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .start {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 9px;
    padding: 12px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--line);
    text-align: left;
    transition:
      border-color 0.3s var(--ease),
      box-shadow 0.3s var(--ease),
      transform 0.3s var(--ease);
  }
  .start:hover {
    background: var(--surface);
    transform: translateY(-2px);
  }
  .start.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .start-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: linear-gradient(135deg, #2f6bf0, #8b4cf0);
    color: #fff;
  }
  .start.selected .start-icon,
  .start.playing .start-icon {
    box-shadow: 0 0 14px rgba(99, 102, 241, 0.55);
  }
  .eq {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 14px;
  }
  .eq i {
    width: 2.5px;
    height: 100%;
    border-radius: 2px;
    background: #fff;
    transform: scaleY(0.35);
  }
  .start.playing .eq i,
  .start:hover .eq i {
    animation: eq 0.5s var(--ease) infinite alternate;
  }
  .eq i:nth-child(2) { animation-delay: 0.12s !important; }
  .eq i:nth-child(3) { animation-delay: 0.24s !important; }
  .eq i:nth-child(4) { animation-delay: 0.36s !important; }
  @keyframes eq {
    to {
      transform: scaleY(1);
    }
  }
  .start-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .start-text strong {
    font-weight: 600;
  }
  .start-text small {
    color: var(--text-2);
    font-size: 11px;
    line-height: 1.35;
  }
  .default {
    position: absolute;
    top: 12px;
    right: 10px;
  }
  .ev {
    flex-direction: row;
    align-items: center;
    gap: 11px;
  }
  .ev-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--selected);
    color: var(--text-2);
  }
  .ev-icon.start {
    color: var(--red);
  }
  .ev-icon.error {
    color: var(--orange);
  }
  .play {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0 0 0 2px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
    color: var(--text);
  }
  .play:hover {
    background: var(--accent);
    border-color: transparent;
    color: #fff;
  }
  .play.on::after {
    content: "";
    position: absolute;
    inset: -1px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    animation: ping 0.7s var(--ease-out);
  }
  @keyframes ping {
    from {
      transform: scale(1);
      opacity: 1;
    }
    to {
      transform: scale(1.8);
      opacity: 0;
    }
  }
</style>
