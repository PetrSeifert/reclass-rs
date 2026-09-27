// Connection to reclass-server plus the shared reactive state. The server is the
// source of truth: it pushes `session`, `defs` and `frame` messages, and every
// edit is a command whose effect arrives back as a new frame.

import type { CardView, Defs, Frame, Row, Session } from './types'

type Reply = { type: 'reply'; id: number; ok: boolean; result?: unknown; error?: string }

export const app = $state({
  connected: false,
  connecting: false,
  connectionError: '',
  session: null as Session | null,
  defs: { type: 'defs', classes: [], enums: [], signatures: [] } as Defs,
  frame: null as Frame | null,
  selected: null as { card: number; key: string } | null,
  toast: null as { text: string; error: boolean; n: number } | null,
  /** Enum open in the enum editor. */
  editingEnum: null as number | null,
  modulesOpen: false,
})

let socket: WebSocket | null = null
// Kept only in this tab's memory. Never put credentials in a URL or storage.
let apiToken = ''
let reconnectTimer: ReturnType<typeof setTimeout> | undefined
let reconnectDelay = 1000
let nextId = 1
const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>()

/** Stops reconnecting so the user can enter a different token. */
export function changeToken() {
  clearTimeout(reconnectTimer)
  const previous = socket
  // Invalidate callbacks and any in-flight auth probe before closing.
  socket = null
  previous?.close()
  apiToken = ''
  reconnectDelay = 1000
  app.connected = false
  app.connecting = false
  app.connectionError = ''
  for (const p of pending.values()) p.reject(new Error('disconnected'))
  pending.clear()
}

export function connect(token?: string) {
  if (token !== undefined) {
    apiToken = token.trim()
    reconnectDelay = 1000
  }
  if (!/^[0-9a-fA-F]{64,}$/.test(apiToken)) {
    app.connectionError = 'Enter the API token from the server terminal.'
    return
  }
  clearTimeout(reconnectTimer)
  socket?.close()
  app.connected = false
  app.connecting = true
  app.connectionError = ''
  const proto = location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${proto}://${location.host}/ws`, ['reclass', `reclass-token.${apiToken}`])
  socket = ws
  let opened = false
  ws.onopen = () => {
    if (socket !== ws) return
    opened = true
    reconnectDelay = 1000
    app.connected = true
    app.connecting = false
  }
  ws.onclose = async () => {
    if (socket !== ws) return
    app.connected = false
    app.connecting = true
    for (const p of pending.values()) p.reject(new Error('disconnected'))
    pending.clear()
    if (!opened) {
      // WebSocket hides handshake status codes. Confirm rejection over HTTP;
      // network errors, timeouts and proxy failures must preserve the token.
      let rejected = false
      try {
        const response = await fetch('/api/auth', {
          method: 'POST',
          headers: { Authorization: `Bearer ${apiToken}` },
          cache: 'no-store',
          redirect: 'error',
          signal: AbortSignal.timeout(5000),
        })
        rejected = response.status === 401 || response.status === 403
      } catch { /* Keep retrying while the server is unreachable. */ }
      if (socket !== ws) return
      if (rejected) {
        apiToken = ''
        app.connecting = false
        app.connectionError = 'Connection rejected. Check the token and allowed origin, then retry.'
        return
      }
    }
    reconnectTimer = setTimeout(() => connect(), reconnectDelay)
    reconnectDelay = Math.min(reconnectDelay * 2, 30000)
  }
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data)
    switch (msg.type) {
      case 'session': app.session = msg; break
      case 'defs': app.defs = msg; break
      case 'frame': app.frame = msg; break
      case 'reply': {
        const r = msg as Reply
        const p = pending.get(r.id)
        pending.delete(r.id)
        if (!p) break
        if (r.ok) p.resolve(r.result)
        else p.reject(new Error(r.error))
        break
      }
    }
  }
}

/** Sends a command. Errors are shown as a toast and re-thrown. */
export async function call<T = unknown>(method: string, params: object = {}): Promise<T> {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    toast('Not connected to reclass-server', true)
    throw new Error('not connected')
  }
  const id = nextId++
  const p = new Promise<T>((resolve, reject) => pending.set(id, { resolve: resolve as (v: unknown) => void, reject }))
  socket.send(JSON.stringify({ id, method, params }))
  try {
    return await p
  } catch (e) {
    toast((e as Error).message, true)
    throw e
  }
}

let toastTimer: ReturnType<typeof setTimeout> | undefined
export function toast(text: string, error = false) {
  app.toast = { text, error, n: (app.toast?.n ?? 0) + 1 }
  clearTimeout(toastTimer)
  toastTimer = setTimeout(() => (app.toast = null), error ? 4000 : 2200)
}

export function card(id: number): CardView | undefined {
  return app.frame?.cards.find((c) => c.id === id)
}

export function selectedRow(): { card: CardView; row: Row } | null {
  const s = app.selected
  if (!s) return null
  const c = card(s.card)
  const row = c?.rows.find((r) => r.key === s.key)
  return c && row ? { card: c, row } : null
}

export function className(id: number | null | undefined): string {
  return app.defs.classes.find((c) => c.id === id)?.name ?? `#${id}`
}

export async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast(`Copied ${text}`)
  } catch {
    toast('Clipboard unavailable', true)
  }
}
