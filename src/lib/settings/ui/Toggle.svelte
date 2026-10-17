<script lang="ts">
  let {
    checked,
    onchange,
    disabled = false,
    label,
  }: { checked: boolean; onchange: (v: boolean) => void; disabled?: boolean; label?: string } = $props();
</script>

<button
  type="button"
  class="toggle"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 34px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: var(--track);
    flex: none;
    transition: background-color 0.28s var(--ease);
  }
  .toggle:hover {
    background: var(--track);
    filter: brightness(1.08);
  }
  .toggle.on {
    background: var(--accent);
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--knob);
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.3),
      0 0 0 0.5px rgba(0, 0, 0, 0.06);
    transition:
      transform 0.34s var(--spring),
      width 0.2s var(--ease);
  }
  .toggle:active:not(:disabled) .knob {
    width: 20px;
  }
  .toggle.on .knob {
    transform: translateX(14px);
  }
  .toggle.on:active:not(:disabled) .knob {
    transform: translateX(10px);
  }
  .toggle:active:not(:disabled) {
    transform: none;
  }
</style>
