// Canvas geometry and view state. Card rows have a fixed height, so wire
// endpoints are computed instead of measured; the CSS in Card.svelte must
// match these constants.

import { app, call } from './store.svelte'
import type { CardView } from './types'

export const CARD_W = 360
export const HEADER_H = 43
export const BODY_PAD_TOP = 4
export const ROW_H = 24
export const BODY_PAD_BOTTOM = 6
export const FOOTER_H = 44
const GAP_X = 140
const GAP_Y = 40

const HUES = ['#8b7bff', '#3ad0c9', '#ffb86b', '#ff8fc8', '#6bb6ff', '#3ddc97', '#ffd86b']
export const hue = (classId: number) => HUES[(classId - 1 + HUES.length * 100) % HUES.length]

export const view = $state({ x: 300, y: 110, k: 0.9 })

/** Positions of cards being dragged, until the server echoes them back. */
export const overrides = $state<Record<number, { x: number; y: number }>>({})

export function pos(c: CardView) {
  return overrides[c.id] ?? { x: c.x, y: c.y }
}

export function cardHeight(c: CardView) {
  return HEADER_H + BODY_PAD_TOP + Math.max(1, c.rows.length) * ROW_H + BODY_PAD_BOTTOM + FOOTER_H
}

/** Canvas y of the middle of row `key` (for wire endpoints). */
export function rowY(c: CardView, key: string) {
  const i = c.rows.findIndex((r) => r.key === key)
  return pos(c).y + HEADER_H + BODY_PAD_TOP + Math.max(0, i) * ROW_H + ROW_H / 2
}

export const toScreen = (x: number, y: number) => ({ x: x * view.k + view.x, y: y * view.k + view.y })

export function zoomAt(sx: number, sy: number, k: number) {
  k = Math.max(0.2, Math.min(2, k))
  view.x = sx - ((sx - view.x) * k) / view.k
  view.y = sy - ((sy - view.y) * k) / view.k
  view.k = k
}

/** Screen area not covered by the floating panels. */
function freeArea() {
  return { left: 260, top: 76, width: innerWidth - 260 - 310, height: innerHeight - 76 - 60 }
}

export function fit() {
  const cards = app.frame?.cards ?? []
  if (!cards.length) return
  const x0 = Math.min(...cards.map((c) => pos(c).x))
  const y0 = Math.min(...cards.map((c) => pos(c).y))
  const x1 = Math.max(...cards.map((c) => pos(c).x + CARD_W))
  const y1 = Math.max(...cards.map((c) => pos(c).y + cardHeight(c)))
  const a = freeArea()
  view.k = Math.max(0.25, Math.min(1.1, Math.min(a.width / (x1 - x0), a.height / (y1 - y0))))
  view.x = a.left + (a.width - (x1 - x0) * view.k) / 2 - x0 * view.k
  view.y = a.top + (a.height - (y1 - y0) * view.k) / 2 - y0 * view.k
}

/** Pans so the card is in view (and optionally pulses it). */
export function focusCard(id: number) {
  const c = app.frame?.cards.find((c) => c.id === id)
  if (!c) return
  const p = toScreen(pos(c).x, pos(c).y)
  const a = freeArea()
  if (p.x < a.left || p.x + CARD_W * view.k > a.left + a.width || p.y < a.top || p.y > a.top + a.height - 120) {
    view.x = a.left + a.width / 2 - (pos(c).x + CARD_W / 2) * view.k
    view.y = a.top + 30 - pos(c).y * view.k
  }
  pulse.id = id
  pulse.n++
}
export const pulse = $state({ id: 0, n: 0 })

function anchorOf(c: CardView) {
  return c.links.find((l) => l.anchor)?.card
}

/** Layered layout by anchor depth; sent to the server so every client sees it. */
export async function tidy() {
  const cards = app.frame?.cards ?? []
  const byId = new Map(cards.map((c) => [c.id, c]))
  const depth = (c: CardView) => {
    let d = 0
    for (let x: CardView | undefined = c; x && !x.isRoot && d < 64; d++) x = byId.get(anchorOf(x) ?? -1)
    return d
  }
  const cols = new Map<number, CardView[]>()
  for (const c of cards) cols.set(depth(c), [...(cols.get(depth(c)) ?? []), c])
  const moves: { card: number; x: number; y: number }[] = []
  for (const [d, list] of cols) {
    let y = 0
    for (const c of list.sort((a, b) => pos(a).y - pos(b).y)) {
      moves.push({ card: c.id, x: d * (CARD_W + GAP_X), y })
      overrides[c.id] = { x: d * (CARD_W + GAP_X), y }
      y += cardHeight(c) + GAP_Y
    }
  }
  fit()
  try {
    await call('moveCards', { moves })
  } finally {
    for (const m of moves) delete overrides[m.card]
  }
}
