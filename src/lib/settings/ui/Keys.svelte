<script lang="ts">
  import { chips } from "../../shortcutFormat";

  /** Key chips for a stored binding (⌘ᴿ, ⌥ ⇧ K, esc…). */
  let { binding, size = "md" }: { binding: string; size?: "sm" | "md" } = $props();
</script>

<span class="keys {size}">
  {#each chips(binding) as c, i (i)}
    <kbd class:wide={c.wide}>{c.label}{#if c.side}<sup>{c.side}</sup>{/if}</kbd>
  {/each}
</span>

<style>
  .keys {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 20px;
    padding: 0 6px;
    border-radius: 5px;
    background: var(--chip);
    border: 1px solid var(--chip-line);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.25);
    font-family: inherit;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text);
    line-height: 1;
  }
  kbd.wide {
    padding: 0 8px;
    font-size: 11px;
  }
  sup {
    font-size: 7.5px;
    margin-left: 1px;
    transform: translateY(-1px);
    font-weight: 600;
  }
  .sm kbd {
    min-width: 18px;
    height: 17px;
    font-size: 10.5px;
    padding: 0 4px;
  }
</style>
