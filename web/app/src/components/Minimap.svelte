<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { CARD_W, cardHeight, hue, pos, view, zoomAt } from '../lib/geometry.svelte'

  let canvas: HTMLCanvasElement | undefined = $state()
  let size = $state({ w: innerWidth, h: innerHeight })
  let mapping = { s: 1, x0: 0, y0: 0 }

  $effect(() => {
    const cards = app.frame?.cards ?? []
    const { x, y, k } = view
    if (!canvas) return
    const g = canvas.getContext('2d')!
    g.clearRect(0, 0, canvas.width, canvas.height)
    if (!cards.length) return
    const x0 = Math.min(...cards.map((c) => pos(c).x)) - 200
    const y0 = Math.min(...cards.map((c) => pos(c).y)) - 200
    const x1 = Math.max(...cards.map((c) => pos(c).x + CARD_W)) + 200
    const y1 = Math.max(...cards.map((c) => pos(c).y + cardHeight(c))) + 200
    const s = Math.min(canvas.width / (x1 - x0), canvas.height / (y1 - y0))
    mapping = { s, x0, y0 }
    const tx = (v: number) => (v - x0) * s, ty = (v: number) => (v - y0) * s
    const byId = new Map(cards.map((c) => [c.id, c]))
    g.strokeStyle = 'rgba(179,155,255,.5)'
    g.lineWidth = 2
    for (const c of cards) {
      for (const l of c.links) {
        const p = byId.get(l.card)
        if (!p) continue
        g.beginPath()
        g.moveTo(tx(pos(p).x + CARD_W), ty(pos(p).y + 40))
        g.lineTo(tx(pos(c).x), ty(pos(c).y + 20))
        g.stroke()
      }
    }
    for (const c of cards) {
      g.fillStyle = hue(c.classId) + (app.selected?.card === c.id ? 'ff' : '99')
      g.fillRect(tx(pos(c).x), ty(pos(c).y), CARD_W * s, cardHeight(c) * s)
    }
    g.strokeStyle = '#fff8'
    g.lineWidth = 1.5
    g.strokeRect(tx(-x / k), ty(-y / k), (size.w / k) * s, (size.h / k) * s)
  })

  function jump(e: MouseEvent) {
    const r = canvas!.getBoundingClientRect()
    const wx = ((e.clientX - r.left) * canvas!.width) / r.width / mapping.s + mapping.x0
    const wy = ((e.clientY - r.top) * canvas!.height) / r.height / mapping.s + mapping.y0
    view.x = innerWidth / 2 - wx * view.k
    view.y = innerHeight / 2 - wy * view.k
  }
</script>

<svelte:window onresize={() => (size = { w: innerWidth, h: innerHeight })} />

<div class="glass mini">
  <canvas bind:this={canvas} width="400" height="260" onclick={jump}></canvas>
</div>
<div class="glass zoom">
  <button class="tbtn" onclick={() => zoomAt(innerWidth / 2, innerHeight / 2, view.k / 1.2)}>−</button>
  <span>{Math.round(view.k * 100)}%</span>
  <button class="tbtn" onclick={() => zoomAt(innerWidth / 2, innerHeight / 2, view.k * 1.2)}>＋</button>
</div>

<style>
  .mini { bottom: 14px; right: 14px; width: 200px; height: 130px; overflow: hidden; }
  .mini canvas { width: 100%; height: 100%; display: block; cursor: pointer; }
  .zoom { bottom: 14px; right: 224px; display: flex; align-items: center; padding: 4px; }
  .zoom span { font: 11px var(--mono); color: var(--dim); width: 44px; text-align: center; }
</style>
