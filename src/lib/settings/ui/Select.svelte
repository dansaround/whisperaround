<script lang="ts" module>
  export interface Option<V> {
    value: V;
    label: string;
    hint?: string;
    group?: string;
    icon?: string;
  }
</script>

<script lang="ts" generics="T extends string">
  import { scale } from "svelte/transition";
  import { cubicInOut } from "svelte/easing";
  import Icon from "./Icon.svelte";

  let {
    value,
    options,
    onchange,
    align = "left",
    variant = "field",
    icon,
    minWidth = 200,
    display,
  }: {
    value: T;
    options: Option<T>[];
    onchange: (v: T) => void;
    align?: "left" | "right";
    /** "field" = bordered control, "ghost" = plain text button (headers). */
    variant?: "field" | "ghost";
    icon?: string;
    minWidth?: number;
    display?: string;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let menu = $state<HTMLDivElement>();
  /** Fixed-position placement, computed from the trigger when opening. */
  let place = $state("");
  let up = $state(false);
  const current = $derived(options.find((o) => o.value === value));

  const GAP = 6;
  const MAX_H = 320;

  function toggle() {
    if (open) {
      open = false;
      return;
    }
    // The menu lives in <body> with fixed positioning, so no card or scroll
    // container can clip it. Open upward when there's more room above.
    const r = trigger.getBoundingClientRect();
    const below = window.innerHeight - r.bottom - GAP - 8;
    const above = r.top - GAP - 8;
    up = below < Math.min(MAX_H, 220) && above > below;
    const vertical = up ? `bottom:${window.innerHeight - r.top + GAP}px` : `top:${r.bottom + GAP}px`;
    const horizontal = align === "right" ? `right:${window.innerWidth - r.right}px` : `left:${r.left}px`;
    place = `${vertical};${horizontal};max-height:${Math.min(MAX_H, up ? above : below)}px;min-width:${Math.max(minWidth, r.width)}px`;
    open = true;
  }

  function pick(v: T) {
    open = false;
    if (v !== value) onchange(v);
  }

  function onWindowClick(e: MouseEvent) {
    const t = e.target as Node;
    if (open && !root.contains(t) && !menu?.contains(t)) open = false;
  }

  /** Move the node to <body> (escapes `overflow: hidden` ancestors). */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<svelte:window
  onclick={onWindowClick}
  onkeydown={(e) => e.key === "Escape" && (open = false)}
  onresize={() => (open = false)}
/>
<!-- A scrolled page would leave the fixed menu behind: close it instead. -->
<svelte:document onscrollcapture={(e) => open && !menu?.contains(e.target as Node) && (open = false)} />

<div class="select {variant}" class:open bind:this={root}>
  <button type="button" class="trigger" bind:this={trigger} onclick={toggle} aria-haspopup="listbox" aria-expanded={open}>
    {#if variant === "ghost"}
      <span class="label">{display ?? current?.label ?? value}</span>
      {#if icon}<Icon name={icon} size={15} />{:else}<Icon name="updown" size={12} />{/if}
    {:else}
      {#if current?.icon}<Icon name={current.icon} size={14} />{/if}
      <span class="label">{display ?? current?.label ?? value}</span>
      <Icon name="updown" size={12} />
    {/if}
  </button>
  {#if open}
    <div
      class="menu {align}"
      class:up
      role="listbox"
      style={place}
      bind:this={menu}
      use:portal
      transition:scale={{ duration: 110, start: 0.97, opacity: 0, easing: cubicInOut }}
    >
      {#each options as o, i (o.value)}
        {#if o.group && o.group !== options[i - 1]?.group}
          <div class="group">{o.group}</div>
        {/if}
        <button type="button" class="item" class:active={o.value === value} role="option" aria-selected={o.value === value} onclick={() => pick(o.value)}>
          {#if o.icon}<span class="item-icon"><Icon name={o.icon} size={14} /></span>{/if}
          <span class="item-text">
            <span>{o.label}</span>
            {#if o.hint}<small>{o.hint}</small>{/if}
          </span>
          <span class="tick">{#if o.value === value}<Icon name="check" size={14} stroke={2.2} />{/if}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .select {
    position: relative;
    display: inline-flex;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    max-width: 320px;
  }
  .field .trigger {
    padding: 5px 9px 5px 11px;
    border-radius: 7px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    font-weight: 450;
    min-width: 160px;
    justify-content: space-between;
  }
  .field .trigger :global(.icon:last-child) {
    color: var(--text-2);
  }
  .ghost .trigger {
    padding: 4px 6px;
    border: none;
    background: transparent;
    color: var(--text);
    font-weight: 500;
  }
  .ghost .trigger:hover,
  .ghost.open .trigger {
    background: var(--selected);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .menu {
    position: fixed;
    z-index: 1000;
    color: var(--text);
    user-select: none;
    -webkit-user-select: none;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    max-height: 320px;
    overflow-y: auto;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    backdrop-filter: blur(20px);
  }
  .menu.left {
    transform-origin: top left;
  }
  .menu.right {
    transform-origin: top right;
  }
  .menu.up.left {
    transform-origin: bottom left;
  }
  .menu.up.right {
    transform-origin: bottom right;
  }
  .group {
    padding: 6px 9px 3px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 9px;
    border: none;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    font-weight: 450;
    white-space: nowrap;
  }
  .item:hover {
    background: var(--accent);
    color: #fff;
  }
  .item:hover small,
  .item:hover .item-icon {
    color: rgba(255, 255, 255, 0.8);
  }
  .item-icon {
    color: var(--text-2);
    display: inline-flex;
  }
  .item-text {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .item-text small {
    font-size: 11px;
    color: var(--text-2);
  }
  .tick {
    width: 14px;
    display: inline-flex;
    color: var(--accent);
  }
  .item:hover .tick {
    color: #fff;
  }
</style>
