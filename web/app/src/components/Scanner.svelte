<script lang="ts">
  import { app, call, copy } from '../lib/store.svelte'
  import { openMenu } from '../lib/menu.svelte'
  import type { ScanReply, ScanResults, ScanHit } from '../lib/types'

  const TYPES = ['i8', 'i16', 'i32', 'i64', 'u8', 'u16', 'u32', 'u64', 'f32', 'f64', 'text', 'bytes']
  const FIRST: [string, string][] = [['exact', 'Exact value'], ['between', 'Value between'], ['unknown', 'Unknown initial value']]
  const NEXT: [string, string][] = [
    ['exact', 'Exact value'], ['between', 'Value between'], ['changed', 'Changed'], ['unchanged', 'Unchanged'],
    ['increased', 'Increased'], ['decreased', 'Decreased'], ['increasedBy', 'Increased by'], ['decreasedBy', 'Decreased by'],
  ]
  /** Text and byte scans only compare for equality. */
  const PATTERN_CONDS = ['exact', 'changed', 'unchanged']
  const PAGE = 100

  /** The server's scan; null when there is none. Scans started from the CLI show up here too. */
  let results = $state<ScanResults | null>(null)
  let problem = $state('')
  let busy = $state(false)
  /** What this panel's last scan reported, kept while the server's scan is still that one. */
  let last = $state<{ text: string; count: number; type: string } | null>(null)
  let limit = $state(PAGE)

  let ty = $state('i32')
  let cond = $state('exact')
  let value = $state('')
  let value2 = $state('')
  let scope = $state('')
  let start = $state('')
  let end = $state('')
  let readOnly = $state(false)
  let modules = $state<string[]>([])

  const scanning = $derived(results != null)
  const type = $derived(results?.type ?? ty)
  const pattern = $derived(type === 'text' || type === 'bytes')
  const conds = $derived((scanning ? NEXT : FIRST).filter(([c]) => !pattern || PATTERN_CONDS.includes(c)))
  const inputs = $derived(cond === 'between' ? 2 : ['exact', 'increasedBy', 'decreasedBy'].includes(cond) ? 1 : 0)
  const placeholder = $derived(type === 'bytes' ? '48 8B ?? 05' : type === 'text' ? 'text to find' : type.startsWith('f') ? '100.5' : '100 or 0x64')

  // Show the type of the server's scan, which another client may have started.
  $effect(() => {
    if (results) ty = results.type
  })

  const summary = $derived(
    busy ? 'Scanning…' : last && results?.count === last.count && results.type === last.type ? last.text : '',
  )

  // A condition the current mode or type does not offer falls back to an exact value.
  $effect(() => {
    if (!conds.some(([c]) => c === cond)) cond = 'exact'
  })

  async function refresh() {
    try {
      results = await call<ScanResults>('scanResults', { limit }, { quiet: true })
      problem = ''
    } catch (e) {
      const message = (e as Error).message
      results = null
      // No scan yet is the normal empty state, not a problem.
      problem = message.startsWith('no scan') || message === 'not connected' ? '' : message
    }
  }

  // Poll while open, so live values and scans from other clients appear.
  $effect(() => {
    if (!app.scannerOpen || !app.connected || !app.session?.attached) return
    refresh()
    const timer = setInterval(() => !busy && refresh(), 1000)
    return () => clearInterval(timer)
  })

  $effect(() => {
    const pid = app.session?.attached?.pid
    if (!app.scannerOpen || pid == null) return
    call<{ name: string }[]>('modules', {}, { quiet: true }).then(
      (m) => (modules = m.map((x) => x.name)),
      () => (modules = []),
    )
  })

  function values(): object {
    if (inputs === 2) return { min: value.trim(), max: value2.trim() }
    if (inputs === 1) return { value: pattern ? value : value.trim() }
    return {}
  }

  async function run(method: string, params: object) {
    busy = true
    last = null
    try {
      const r = await call<ScanReply>(method, params)
      const size = r.bytes != null ? ` · ${Math.round(r.bytes / (1 << 20))} MiB in ${r.regions} regions` : ''
      last = { text: `${r.count.toLocaleString()} in ${r.ms} ms${size}`, count: r.count, type: results?.type ?? ty }
      limit = PAGE
    } catch {
      // The error is already shown as a toast.
    } finally {
      busy = false
      await refresh()
    }
  }

  function newScan() {
    const params: Record<string, unknown> = { type: ty, readOnly, ...(cond === 'unknown' ? { unknown: true } : values()) }
    if (scope === 'range') Object.assign(params, { start: start.trim(), end: end.trim() })
    else if (scope) params.module = scope
    run('scan', params)
  }

  function nextScan() {
    run('scanNext', { cond, ...values() })
  }

  async function clear() {
    await call('scanClear')
    last = null
    await refresh()
  }

  function submit(e: SubmitEvent) {
    e.preventDefault()
    if (busy) return
    if (scanning) nextScan()
    else newScan()
  }

  function menu(e: MouseEvent, h: ScanHit) {
    const address = `0x${h.address}`
    // `game.exe+0x10` as an expression: `<game.exe>+0x10`.
    const reference = h.symbol && `<${h.symbol.replace('+', '>+')}`
    openMenu(e, [
      { header: h.symbol ?? address },
      { label: 'Copy address', hint: address, onClick: () => copy(address) },
      ...(reference ? [{ label: 'Copy module reference', hint: reference, onClick: () => copy(reference) }] : []),
      { label: 'Copy value', hint: h.value ?? '', disabled: h.value == null, onClick: () => copy(h.value ?? '') },
      { sep: true },
      { label: 'Use as root expression', hint: address, onClick: () => call('setRoot', { expr: address }) },
    ])
  }

  function more() {
    limit = Math.min(limit + PAGE, 1000)
    refresh()
  }
</script>

{#if app.scannerOpen}
  <div class="glass panel">
    <div class="top">
      <b>Scanner</b>
      {#if results}<span class="count">{results.count.toLocaleString()} {results.count === 1 ? 'result' : 'results'} · {results.type}</span>{/if}
      <span class="sp"></span>
      <button class="x" title="Close (S)" onclick={() => (app.scannerOpen = false)}>✕</button>
    </div>

    {#if !app.session?.attached}
      <div class="none">Attach to a process to scan its memory.</div>
    {:else}
      <form onsubmit={submit}>
        <div class="line">
          <select bind:value={ty} disabled={scanning || busy} title={scanning ? 'Start a new scan to change the type' : 'Value type'}>
            {#each TYPES as t (t)}<option value={t}>{t}</option>{/each}
            {#if scanning && !TYPES.includes(type)}<option value={type}>{type}</option>{/if}
          </select>
          <select bind:value={cond} disabled={busy}>
            {#each conds as [c, label] (c)}<option value={c}>{label}</option>{/each}
          </select>
        </div>
        {#if inputs > 0}
          <div class="line">
            <input bind:value placeholder={inputs === 2 ? 'min' : placeholder} spellcheck="false" disabled={busy} />
            {#if inputs === 2}<input bind:value={value2} placeholder="max" spellcheck="false" disabled={busy} />{/if}
          </div>
        {/if}
        {#if !scanning}
          <div class="line">
            <select bind:value={scope} disabled={busy} title="Memory to scan">
              <option value="">All readable memory</option>
              <option value="range">Address range…</option>
              {#each modules as m (m)}<option value={m}>{m}</option>{/each}
            </select>
            <label class="check" title="Also scan code and constants in module images">
              <input type="checkbox" bind:checked={readOnly} disabled={busy} /> read-only
            </label>
          </div>
          {#if scope === 'range'}
            <div class="line">
              <input bind:value={start} placeholder="start, e.g. <game.exe>" spellcheck="false" disabled={busy} />
              <input bind:value={end} placeholder="end" spellcheck="false" disabled={busy} />
            </div>
          {/if}
        {/if}
        <div class="line buttons">
          {#if scanning}
            <button type="submit" class="primary" disabled={busy}>Next scan</button>
            <button type="button" disabled={busy} onclick={clear}>New scan</button>
          {:else}
            <button type="submit" class="primary" disabled={busy}>First scan</button>
          {/if}
        </div>
      </form>

      {#if summary}<div class="summary" class:busy>{summary}</div>{/if}
      {#if problem}<div class="problem">{problem}</div>{/if}

      {#if results}
        <div class="cols"><span>Address</span><span>Value</span><span>Previous</span></div>
        <div class="list">
          {#each results.results as h (h.address)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="row" title="Click to copy the address · right-click for more" onclick={() => copy(`0x${h.address}`)} oncontextmenu={(e) => menu(e, h)}>
              <span class="addr">0x{h.address}{#if h.symbol}<small>{h.symbol}</small>{/if}</span>
              <span class="val" class:changed={h.value !== h.previous} class:gone={h.value == null}>{h.value ?? 'unreadable'}</span>
              <span class="prev">{h.previous}</span>
            </div>
          {:else}
            <div class="none">No addresses left. Start a new scan.</div>
          {/each}
          {#if results.results.length < results.count && limit < 1000}
            <button class="more" onclick={more}>Show {Math.min(PAGE, results.count - results.results.length)} more</button>
          {:else if results.results.length < results.count}
            <div class="none">Showing the first {results.results.length}; narrow the scan to see the rest.</div>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .panel { top: 76px; left: 258px; width: 400px; max-height: calc(100vh - 140px); display: flex; flex-direction: column; padding: 12px; gap: 8px; }
  .top { display: flex; align-items: center; gap: 8px; }
  .count { font: 11px var(--mono); color: var(--faint); }
  .sp { flex: 1; }
  .x { border: none; background: none; color: var(--faint); width: 22px; height: 22px; border-radius: 6px; cursor: pointer; }
  .x:hover { background: #ffffff14; color: var(--text); }
  form { display: flex; flex-direction: column; gap: 6px; }
  .line { display: flex; gap: 6px; align-items: center; }
  .line > * { flex: 1; min-width: 0; }
  input:not([type='checkbox']), select { background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 8px; padding: 6px 9px; font: 12px var(--ui); outline: none; }
  input:not([type='checkbox']) { font-family: var(--mono); }
  input:focus, select:focus { border-color: var(--accent); }
  select:disabled, input:disabled { opacity: 0.6; }
  .check { flex: 0 0 auto; display: flex; gap: 4px; align-items: center; color: var(--dim); cursor: pointer; }
  .buttons button { border: 1px solid var(--edge2); background: #ffffff0a; color: var(--text); border-radius: 8px; padding: 6px; cursor: pointer; }
  .buttons button:hover:not(:disabled) { background: #ffffff14; }
  .buttons .primary { border-color: var(--accent); background: rgba(139, 123, 255, 0.18); }
  .buttons button:disabled { opacity: 0.5; cursor: default; }
  .summary { font: 11px var(--mono); color: var(--dim); }
  .summary.busy { color: var(--accent); }
  .problem { color: var(--bad); font-size: 11px; }
  .cols, .row { display: grid; grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr); gap: 8px; align-items: baseline; }
  .cols { padding: 0 8px; font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--faint); }
  .list { overflow: auto; min-height: 0; }
  .row { padding: 4px 8px; border-radius: 7px; cursor: pointer; font: 11px var(--mono); }
  .row:hover { background: #ffffff0a; }
  .row > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .addr { color: var(--k-ptr); }
  .addr small { display: block; color: var(--faint); font-size: 10px; }
  .val { color: var(--text); }
  .val.changed { color: var(--k-float); }
  .val.gone { color: var(--bad); }
  .prev { color: var(--faint); }
  .none { color: var(--faint); padding: 8px; }
  .more { width: 100%; margin-top: 4px; border: 1px dashed var(--edge2); background: none; color: var(--dim); border-radius: 8px; padding: 5px; cursor: pointer; }
  .more:hover { color: var(--text); }
</style>
