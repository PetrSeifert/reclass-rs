import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'

function setup() {
  const sockets = []
  const timers = new Map()
  let timerId = 0
  const requests = []
  const context = vm.createContext({
    exports: {},
    $state: (value) => value,
    location: { protocol: 'http:', host: 'localhost:7878' },
    AbortSignal,
    fetch: (url, options) => new Promise((resolve, reject) => requests.push({ url, options, resolve, reject })),
    WebSocket: class {
      static OPEN = 1
      constructor(url, protocols) { this.protocols = protocols; this.readyState = 1; this.sent = []; sockets.push(this) }
      send(data) { this.sent.push(JSON.parse(data)) }
      close() {}
    },
    setTimeout: (callback, delay) => { timers.set(++timerId, { callback, delay }); return timerId },
    clearTimeout: (id) => timers.delete(id),
  })
  const source = readFileSync(new URL('./store.svelte.ts', import.meta.url), 'utf8')
  vm.runInContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, context)
  return {
    ...context.exports, sockets, requests, timers,
    retry() {
      assert.equal(timers.size, 1, 'a disconnected client must keep retrying')
      const [id, { callback, delay }] = timers.entries().next().value
      timers.delete(id)
      callback()
      return delay
    },
  }
}

const token = 'a'.repeat(64)

test('changing the token cancels backoff and allows a fresh connection', async () => {
  const client = setup()
  client.connect(token)
  const oldSocket = client.sockets.at(-1)
  oldSocket.onopen()
  await oldSocket.onclose()
  assert.equal(client.timers.size, 1)
  client.changeToken()
  assert.equal(client.app.connecting, false)
  assert.equal(client.timers.size, 0)
  oldSocket.onopen()
  await oldSocket.onclose()
  assert.equal(client.app.connected, false)
  assert.equal(client.timers.size, 0)
  client.connect('b'.repeat(64))
  assert.equal(client.sockets.at(-1).protocols[1], `reclass-token.${'b'.repeat(64)}`)
  client.sockets.at(-1).onopen()
  await client.sockets.at(-1).onclose()
  assert.equal(client.retry(), 1000)
})

test('changing the token during an auth probe keeps the form available after it settles', async () => {
  for (const status of [204, 401, 503]) {
    const client = setup()
    client.connect(token)
    const closed = client.sockets.at(-1).onclose()
    client.changeToken()
    client.requests[0].resolve({ status })
    await closed
    assert.equal(client.app.connecting, false)
    assert.equal(client.app.connectionError, '')
    assert.equal(client.timers.size, 0)
  }
})

test('retains the token through repeated restart failures and resets backoff after recovery', async () => {
  const client = setup()
  client.connect(token)
  client.sockets.at(-1).onopen()
  await client.sockets.at(-1).onclose()
  assert.equal(client.retry(), 1000)
  for (const delay of [2000, 4000, 8000, 16000, 30000, 30000]) {
    const closed = client.sockets.at(-1).onclose()
    client.requests.at(-1)?.reject(new TypeError('Failed to fetch'))
    await closed
    assert.equal(client.app.connecting, true)
    assert.equal(client.retry(), delay)
    assert.equal(client.sockets.at(-1).protocols[1], `reclass-token.${token}`)
  }
  client.sockets.at(-1).onopen()
  await client.sockets.at(-1).onclose()
  assert.equal(client.retry(), 1000)
})

test('only confirmed 401/403 responses clear the token', async () => {
  for (const status of [204, 401, 403, 502, 503]) {
    const client = setup()
    client.connect(token)
    const closed = client.sockets.at(-1).onclose()
    assert.equal(client.requests.length, 1)
    assert.equal(client.requests[0].options.headers.Authorization, `Bearer ${token}`)
    client.requests[0].resolve({ status })
    await closed
    if (status === 401 || status === 403) {
      assert.equal(client.timers.size, 0)
      assert.equal(client.app.connecting, false)
      client.connect()
      assert.equal(client.sockets.length, 1, 'rejected token must be cleared')
    } else {
      client.retry()
      assert.equal(client.sockets.length, 2)
    }
  }
})

test('an old auth response cannot clear a replacement token or schedule a retry', async () => {
  const client = setup()
  client.connect(token)
  const closed = client.sockets.at(-1).onclose()
  client.connect('b'.repeat(64))
  client.sockets.at(-1).onopen()
  client.requests[0].resolve({ status: 401 })
  await closed
  assert.equal(client.app.connected, true)
  assert.equal(client.timers.size, 0)
  client.connect()
  assert.equal(client.sockets.at(-1).protocols[1], `reclass-token.${'b'.repeat(64)}`)
})

test('quiet calls reject without a toast, others show the error', async () => {
  const client = setup()
  client.connect(token)
  const socket = client.sockets.at(-1)
  socket.onopen()
  const fail = (error) => {
    const { id } = socket.sent.at(-1)
    socket.onmessage({ data: JSON.stringify({ type: 'reply', id, ok: false, error }) })
  }
  const quiet = client.call('scanResults', {}, { quiet: true })
  fail('no scan yet')
  await assert.rejects(quiet, /no scan yet/)
  assert.equal(client.app.toast, null)
  const loud = client.call('scanResults')
  fail('no scan yet')
  await assert.rejects(loud, /no scan yet/)
  assert.equal(client.app.toast.text, 'no scan yet')
})
