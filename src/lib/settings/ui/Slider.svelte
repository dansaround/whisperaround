<script lang="ts">
  /** 0..1 range slider with an animated fill. `oninput` while dragging, `onchange` on release. */
  let {
    value,
    oninput,
    onchange,
    disabled = false,
    label,
  }: {
    value: number;
    oninput?: (v: number) => void;
    onchange?: (v: number) => void;
    disabled?: boolean;
    label?: string;
  } = $props();

  let dragging = $state(false);
</script>

<div class="slider" class:disabled class:dragging style="--v:{value}">
  <div class="track"><div class="fill"></div></div>
  <input
    type="range"
    min="0"
    max="1"
    step="0.01"
    aria-label={label}
    {value}
    {disabled}
    onpointerdown={() => (dragging = true)}
    onpointerup={() => (dragging = false)}
    oninput={(e) => oninput?.(Number(e.currentTarget.value))}
    onchange={(e) => {
      dragging = false;
      onchange?.(Number(e.currentTarget.value));
    }}
  />
  <div class="thumb"></div>
</div>

<style>
  .slider {
    position: relative;
    flex: 1;
    height: 22px;
    display: flex;
    align-items: center;
  }
  .track {
    position: absolute;
    left: 0;
    right: 0;
    height: 5px;
    border-radius: 999px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    width: calc(var(--v) * 100%);
    background: linear-gradient(90deg, #5b8cff, var(--accent));
    border-radius: inherit;
  }
  .thumb {
    position: absolute;
    left: calc(var(--v) * (100% - 18px));
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--knob);
    box-shadow:
      0 1px 4px rgba(0, 0, 0, 0.35),
      0 0 0 0.5px rgba(0, 0, 0, 0.08);
    pointer-events: none;
    transition:
      transform 0.25s var(--spring),
      box-shadow 0.25s var(--ease);
  }
  .slider:hover .thumb {
    transform: scale(1.06);
  }
  .slider.dragging .thumb {
    transform: scale(1.18);
    box-shadow:
      0 2px 8px rgba(0, 0, 0, 0.35),
      0 0 0 5px var(--accent-soft);
  }
  input {
    position: absolute;
    inset: 0;
    width: 100%;
    margin: 0;
    padding: 0;
    opacity: 0;
    cursor: pointer;
    box-shadow: none !important;
  }
  .disabled {
    opacity: 0.45;
  }
  .disabled input {
    cursor: not-allowed;
  }
</style>
