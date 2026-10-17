<script lang="ts">
  import { Tween } from "svelte/motion";
  import { cubicOut } from "svelte/easing";
  import { intlLocale } from "../../i18n/index.svelte";

  /** A number that eases from its previous value whenever it changes. */
  let { value, format = (n: number) => Math.round(n).toLocaleString(intlLocale()) }: { value: number; format?: (n: number) => string } =
    $props();

  const tween = new Tween(0, { duration: 1100, easing: cubicOut });
  $effect(() => {
    tween.target = value;
  });
</script>

<span class="count">{format(tween.current)}</span>

<style>
  .count {
    font-variant-numeric: tabular-nums;
  }
</style>
