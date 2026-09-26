<script lang="ts">
  import { closeMenu, type MenuItem } from '../lib/menu.svelte'
  import MenuList from './MenuList.svelte'

  let { items, sub = false }: { items: MenuItem[]; sub?: boolean } = $props()
  let openIndex = $state(-1)

  // Keep submenus on screen.
  function place(el: HTMLDivElement) {
    const r = el.getBoundingClientRect()
    if (r.right > innerWidth) {
      el.style.left = 'auto'
      el.style.right = 'calc(100% + 4px)'
    }
    if (r.bottom > innerHeight) el.style.top = `${Math.min(0, innerHeight - r.bottom - 8)}px`
  }
</script>

<!-- Only menus without submenus may scroll: overflow would clip nested submenus. -->
<div class="cm" class:sub class:scroll={!items.some((i) => i.submenu)} use:place role="menu">
  {#each items as it, i (i)}
    {#if it.sep}
      <div class="sep"></div>
    {:else if it.header}
      <div class="header">{it.header}</div>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="item"
        class:danger={it.danger}
        class:disabled={it.disabled}
        class:checked={it.checked}
        role="menuitem"
        tabindex="-1"
        onmouseenter={() => (openIndex = i)}
        onclick={(e) => {
          e.stopPropagation()
          if (it.disabled || it.submenu) return
          closeMenu()
          it.onClick?.()
        }}
      >
        <span class="label">{it.label}</span>
        {#if it.hint}<span class="hint">{it.hint}</span>{/if}
        {#if it.submenu}
          <span class="arrow">›</span>
          {#if openIndex === i}<MenuList items={it.submenu} sub />{/if}
        {/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  .cm {
    background: rgba(22, 26, 35, 0.97);
    backdrop-filter: blur(14px);
    border: 1px solid var(--edge2);
    border-radius: 12px;
    box-shadow: 0 16px 50px rgba(0, 0, 0, 0.5);
    padding: 5px;
    min-width: 210px;
    font: 12.5px var(--ui);
    color: var(--text);
  }
  .cm.sub { position: absolute; left: calc(100% + 4px); top: -6px; }
  .cm.scroll { max-height: 70vh; overflow-y: auto; }
  .item { display: flex; align-items: center; padding: 6px 24px 6px 26px; border-radius: 7px; position: relative; white-space: nowrap; cursor: default; }
  .item:hover { background: linear-gradient(90deg, rgba(139, 123, 255, 0.35), rgba(139, 123, 255, 0.15)); }
  .hint { margin-left: auto; padding-left: 20px; color: var(--dim); font: 10.5px var(--mono); }
  .arrow { position: absolute; right: 9px; color: var(--dim); }
  .checked::before { content: ''; position: absolute; left: 10px; top: 50%; width: 6px; height: 6px; margin-top: -3px; border-radius: 50%; background: var(--accent2); }
  .danger .label { color: var(--bad); }
  .disabled { opacity: 0.4; }
  .sep { height: 1px; background: var(--edge); margin: 4px 6px; }
  .header { padding: 4px 10px 6px; color: var(--faint); font: 10.5px var(--mono); }
</style>
