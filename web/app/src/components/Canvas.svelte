<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { CARD_W, HEADER_H, pos, rowY, view, zoomAt } from '../lib/geometry.svelte'
  import { closeMenu } from '../lib/menu.svelte'
  import type { CardView } from '../lib/types'
  import Card from './Card.svelte'

  const cards = $derived(app.frame?.cards ?? [])

  interface Wire { d: string; x2: number; y2: number; lx: number; ly: number; label: string; kind: 'anchor' | 'alias' | 'stale'; enc: boolean }

  const wires = $derived.by(() => {
    const byId = new Map(cards.map((c) => [c.id, c]))
    const out: Wire[] = []
    for (const c of cards) {
      for (const l of c.links) {
        const src = byId.get(l.card)
        if (!src) continue
        const x1 = pos(src).x + CARD_W, y1 = rowY(src, l.key)
        const x2 = pos(c).x, y2 = pos(c).y + HEADER_H / 2
        const dx = Math.max(80, Math.abs(x2 - x1) * 0.5)
        const kind = !l.ok ? 'stale' : l.anchor ? 'anchor' : 'alias'
        const label = kind === 'stale'
          ? `${l.label} moved → ${l.now ?? 'null'}`
          : `${l.label}${l.encrypted ? ' · decrypted' : ''}${kind === 'alias' ? ' · shared' : ''}`
        out.push({ d: `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`, x2, y2, lx: (x1 + x2) / 2, ly: (y1 + y2) / 2 - 6, label, kind, enc: l.encrypted })
      }
    }
    return out
  })

  function pan(e: MouseEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest('.node')) return
    closeMenu()
    app.selected = null
    const sx = e.clientX, sy = e.clientY, vx = view.x, vy = view.y
    const stage = e.currentTarget as HTMLElement
    stage.classList.add('panning')
    const move = (ev: MouseEvent) => {
      view.x = vx + ev.clientX - sx
      view.y = vy + ev.clientY - sy
    }
    const up = () => {
      stage.classList.remove('panning')
      removeEventListener('mousemove', move)
      removeEventListener('mouseup', up)
    }
    addEventListener('mousemove', move)
    addEventListener('mouseup', up)
  }

  function wheel(e: WheelEvent) {
    e.preventDefault()
    zoomAt(e.clientX, e.clientY, view.k * Math.exp(-e.deltaY * 0.0015))
  }

  const keyed = (c: CardView) => c.id
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="stage"
  onmousedown={pan}
  onwheel={wheel}
  style:background-size="{22 * view.k}px {22 * view.k}px"
  style:background-position="{view.x}px {view.y}px"
>
  <div class="world" style:transform="translate({view.x}px, {view.y}px) scale({view.k})">
    <svg class="wires">
      <defs>
        <linearGradient id="gp" x1="0" x2="1"><stop offset="0" stop-color="#b39bff" /><stop offset="1" stop-color="#6bb6ff" /></linearGradient>
        <linearGradient id="ge" x1="0" x2="1"><stop offset="0" stop-color="#3ad0c9" /><stop offset="1" stop-color="#8b7bff" /></linearGradient>
      </defs>
      {#each wires as w, i (i)}
        {#if w.kind === 'stale'}
          <path d={w.d} stroke="#ff6b7d" stroke-width="1.5" stroke-dasharray="3 5" opacity="0.85" />
          <text class="lbl stale" x={w.lx} y={w.ly} text-anchor="middle">{w.label}</text>
        {:else}
          <path
            d={w.d}
            stroke="url(#{w.enc ? 'ge' : 'gp'})"
            stroke-width={w.kind === 'anchor' ? 2 : 1.4}
            stroke-dasharray={w.enc ? '6 5' : undefined}
            opacity={w.kind === 'anchor' ? 0.85 : 0.55}
          />
          <circle cx={w.x2} cy={w.y2} r={w.kind === 'anchor' ? 4 : 3} fill={w.enc ? '#8b7bff' : '#6bb6ff'} />
          <text class="lbl" x={w.lx} y={w.ly} text-anchor="middle">{w.label}</text>
        {/if}
      {/each}
    </svg>
    {#each cards as card (keyed(card))}
      <Card {card} />
    {/each}
  </div>
</div>

<style>
  .stage {
    position: fixed;
    inset: 0;
    cursor: grab;
    background-color: var(--bg);
    background-image: radial-gradient(circle, #1c2130 1.2px, transparent 1.3px);
    overflow: hidden;
  }
  :global(.stage.panning) { cursor: grabbing; }
  .world { position: absolute; left: 0; top: 0; transform-origin: 0 0; }
  .wires { position: absolute; left: 0; top: 0; width: 1px; height: 1px; overflow: visible; pointer-events: none; }
  .wires path { fill: none; }
  .lbl { font: 10px var(--mono); fill: var(--dim); }
  .lbl.stale { fill: var(--bad); }
</style>
