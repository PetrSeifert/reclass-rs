<script lang="ts">
  import { untrack } from 'svelte'
  import { app, call } from '../lib/store.svelte'
  import { cardMenu, openMenu } from '../lib/menu.svelte'
  import { hue, overrides, pos, pulse, view } from '../lib/geometry.svelte'
  import type { CardView } from '../lib/types'
  import Row from './Row.svelte'

  let { card }: { card: CardView } = $props()

  const p = $derived(pos(card))
  const aliases = $derived(card.links.length - (card.isRoot ? 0 : 1))
  const selected = $derived(app.selected?.card === card.id)

  let el: HTMLDivElement | undefined = $state()
  // Play the pulse once per request. `card` is a new object on every frame, so
  // without remembering the handled request this would replay 4× a second.
  let pulsed = 0
  $effect(() => {
    const n = pulse.n
    if (n === pulsed || pulse.id !== untrack(() => card.id) || !el) return
    pulsed = n
    el.animate([{ boxShadow: '0 0 0 0 rgba(139,123,255,.8)' }, { boxShadow: '0 0 0 22px rgba(139,123,255,0)' }], { duration: 900 })
  })

  function startDrag(e: MouseEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button, input, .dup')) return
    e.stopPropagation()
    const sx = e.clientX, sy = e.clientY, start = { ...p }
    let moved = false
    const move = (ev: MouseEvent) => {
      moved = true
      overrides[card.id] = { x: start.x + (ev.clientX - sx) / view.k, y: start.y + (ev.clientY - sy) / view.k }
    }
    const up = async () => {
      removeEventListener('mousemove', move)
      removeEventListener('mouseup', up)
      if (!moved) return
      const o = overrides[card.id]
      try {
        await call('moveCards', { moves: [{ card: card.id, x: o.x, y: o.y }] })
      } finally {
        delete overrides[card.id]
      }
    }
    addEventListener('mousemove', move)
    addEventListener('mouseup', up)
  }

  let renamingClass = $state(false)
  let draft = $state('')
  function rename() {
    draft = card.className
    renamingClass = true
  }
  async function commitRename() {
    renamingClass = false
    if (draft.trim() && draft.trim() !== card.className) await call('renameClass', { classId: card.classId, name: draft.trim() })
  }
  function focus(el: HTMLInputElement) {
    el.focus()
    el.select()
  }
</script>

<div
  bind:this={el}
  class="node"
  class:sel={selected}
  style:left="{p.x}px"
  style:top="{p.y}px"
  style:--hue={hue(card.classId)}
  role="group"
  aria-label={card.className}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="nh" onmousedown={startDrag} oncontextmenu={(e) => openMenu(e, cardMenu(card, rename))}>
    <span class="gl">{card.className[0] ?? '?'}</span>
    {#if renamingClass}
      <input
        class="inline-edit"
        bind:value={draft}
        use:focus
        onblur={commitRename}
        onkeydown={(e) => {
          e.stopPropagation()
          if (e.key === 'Enter') commitRename()
          if (e.key === 'Escape') renamingClass = false
        }}
      />
    {:else}
      <span class="nm" ondblclick={rename}>{card.className}</span>
    {/if}
    {#if card.isRoot}
      <span class="root">ROOT</span>
    {:else}
      <span class="via">via {card.via}{card.encrypted ? ' 🔒' : ''}</span>
    {/if}
    {#if aliases > 0}<span class="refs" title="{aliases} more pointer(s) lead to this card">+{aliases}</span>{/if}
    {#if card.dupOf}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <span class="dup" title="Another card shows the same instance — click to merge" onclick={() => call('mergeCard', { card: card.id })}>⧉ merge</span>
    {/if}
    <span class="ad">{card.base ?? 'invalid'}</span>
    {#if !card.isRoot}
      <button class="x" title="Close" onclick={() => call('closeCard', { card: card.id })}>✕</button>
    {/if}
  </div>
  <div class="nb">
    {#each card.rows as row (row.key)}
      <Row {card} {row} />
    {:else}
      <div class="empty" title={card.error ?? undefined}>{card.error ?? 'Address does not resolve'}</div>
    {/each}
  </div>
  <div class="nf">
    <button onclick={() => call('insertBytes', { classId: card.classId, fieldId: null, count: 8 })}>＋ 8 bytes</button>
    <button onclick={() => call('insertBytes', { classId: card.classId, fieldId: null, count: 64 })}>＋ 64</button>
    <button onclick={rename}>Rename class</button>
    <span class="size">0x{card.size.toString(16).toUpperCase()}</span>
  </div>
</div>

<style>
  /* Heights must match geometry.svelte.ts: header 43, body padding 4/6, rows 24, footer 44. */
  .node {
    position: absolute;
    width: 360px;
    background: var(--card);
    border: 1px solid var(--edge);
    border-radius: 14px;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.45);
    box-sizing: border-box;
  }
  .node.sel { border-color: var(--accent); box-shadow: 0 0 0 3px rgba(139, 123, 255, 0.22), 0 10px 40px rgba(0, 0, 0, 0.5); }
  .nh {
    height: 42px;
    box-sizing: content-box;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    cursor: grab;
    border-radius: 14px 14px 0 0;
    border-bottom: 1px solid var(--edge);
    background: linear-gradient(180deg, color-mix(in srgb, var(--hue) 16%, transparent), transparent);
    white-space: nowrap;
  }
  .gl { width: 22px; height: 22px; border-radius: 7px; display: grid; place-items: center; background: color-mix(in srgb, var(--hue) 25%, transparent); color: var(--hue); font-size: 12px; font-weight: 700; flex: none; }
  .nm { font-weight: 650; font-size: 13px; overflow: hidden; text-overflow: ellipsis; }
  .via { color: var(--dim); font-size: 11px; overflow: hidden; text-overflow: ellipsis; }
  .root { font-size: 9px; letter-spacing: 0.08em; background: var(--accent); color: #fff; padding: 1px 5px; border-radius: 4px; }
  .refs { font: 10px var(--mono); color: var(--accent2); border: 1px solid color-mix(in srgb, var(--accent2) 40%, transparent); border-radius: 4px; padding: 0 4px; }
  .dup { font-size: 10px; color: #ffb86b; border: 1px solid #ffb86b66; border-radius: 4px; padding: 0 5px; cursor: pointer; }
  .dup:hover { background: #ffb86b22; }
  .ad { margin-left: auto; font: 11px var(--mono); color: var(--dim); }
  .x { border: none; background: none; color: var(--faint); width: 20px; height: 20px; border-radius: 5px; cursor: pointer; flex: none; }
  .x:hover { background: #ffffff14; color: var(--text); }
  .nb { padding: 4px 0 6px; }
  .empty { height: 24px; line-height: 24px; padding: 0 12px; color: var(--bad); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .nf { height: 43px; box-sizing: content-box; display: flex; gap: 6px; align-items: center; padding: 0 12px; border-top: 1px solid var(--edge); }
  .nf button { border: 1px solid var(--edge2); background: transparent; color: var(--dim); border-radius: 7px; padding: 3px 8px; font-size: 11px; cursor: pointer; }
  .nf button:hover { color: var(--text); border-color: var(--accent); }
  .size { margin-left: auto; color: var(--faint); font: 10px var(--mono); }
  .inline-edit { background: var(--bg); color: var(--text); border: 1px solid var(--accent); border-radius: 4px; font: 13px var(--ui); outline: none; padding: 0 4px; height: 20px; width: 150px; }
</style>
