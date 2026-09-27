<script lang="ts">
  import { untrack } from 'svelte'
  import { app, call, copy } from '../lib/store.svelte'
  import { openMenu } from '../lib/menu.svelte'
  import type { Watch } from '../lib/types'

  const TYPES = ['i8', 'i16', 'i32', 'i64', 'u8', 'u16', 'u32', 'u64', 'f32', 'f64']
  const W = 140
  const H = 26

  let expr = $state('')
  let type = $state('i32')
  let label = $state('')

  /** Watch and field being edited inline: its label or its value. */
  let editing = $state<{ id: number; what: 'label' | 'value' } | null>(null)
  let draft = $state('')

  const canWrite = $derived(!!app.session?.canWrite)

  function focus(el: HTMLInputElement) {
    el.focus()
    el.select()
  }

  function start(w: Watch, what: 'label' | 'value') {
    if (what === 'value' && !canWrite) return
    editing = { id: w.id, what }
    draft = untrack(() => (what === 'label' ? w.label : (w.value ?? '')))
  }

  async function commit(w: Watch) {
    const e = editing
    editing = null
    const text = draft.trim()
    if (!e || !text) return
    if (e.what === 'label' && text !== w.label) await call('watchUpdate', { id: w.id, label: text })
    if (e.what === 'value' && text !== w.value) await call('watchSet', { id: w.id, value: text })
  }

  async function add(e: SubmitEvent) {
    e.preventDefault()
    if (!expr.trim()) return
    await call('watchAdd', { expr: expr.trim(), type, label: label.trim() || undefined })
    expr = ''
    label = ''
  }

  function freeze(w: Watch) {
    call('watchFreeze', { id: w.id, on: w.frozen == null })
  }

  /** Samples as an SVG path scaled between their extremes; gaps where unreadable. */
  function spark(history: (number | null)[]) {
    const nums = history.filter((v): v is number => v != null)
    if (!nums.length) return { d: '', lo: null, hi: null }
    const lo = Math.min(...nums)
    const hi = Math.max(...nums)
    const step = W / Math.max(history.length - 1, 1)
    let d = ''
    let pen = false
    history.forEach((v, i) => {
      if (v == null) return void (pen = false)
      const y = hi === lo ? H / 2 : H - 2 - ((v - lo) / (hi - lo)) * (H - 4)
      d += `${pen ? 'L' : 'M'}${(i * step).toFixed(1)},${y.toFixed(1)}`
      pen = true
    })
    return { d, lo, hi }
  }

  const fmt = (n: number) => (Number.isInteger(n) ? String(n) : n.toFixed(3).replace(/0+$/, '').replace(/\.$/, ''))

  function menu(e: MouseEvent, w: Watch) {
    openMenu(e, [
      { header: `${w.label} · ${w.type}` },
      { label: 'Find related values…', hint: 'scan', onClick: () => ((app.relatedWatch = w.id), (app.scannerOpen = true)) },
      { label: 'Edit value', hint: canWrite ? '' : 'driver is read-only', disabled: !canWrite, onClick: () => start(w, 'value') },
      { label: w.frozen == null ? 'Freeze' : 'Unfreeze', hint: canWrite ? '' : 'driver is read-only', disabled: !canWrite, onClick: () => freeze(w) },
      { label: 'Rename', onClick: () => start(w, 'label') },
      {
        label: 'Type',
        submenu: TYPES.map((t) => ({ label: t, checked: t === w.type, onClick: () => call('watchUpdate', { id: w.id, type: t }) })),
      },
      { sep: true },
      ...(w.address ? [{ label: 'Copy address', hint: w.address, onClick: () => copy(`0x${w.address}`) }] : []),
      ...(w.address ? [{ label: 'Use as root expression', onClick: () => call('setRoot', { expr: w.expr }) }] : []),
      { sep: true },
      { label: 'Remove', danger: true, onClick: () => call('watchRemove', { id: w.id }) },
    ])
  }
</script>

{#if app.watchesOpen}
  <div class="glass panel">
    <div class="top">
      <b>Watches</b>
      <span class="sp"></span>
      <button class="x" title="Close (W)" onclick={() => (app.watchesOpen = false)}>✕</button>
    </div>

    <div class="list">
      {#each app.watches as w (w.id)}
        {@const s = spark(w.history)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="w" oncontextmenu={(e) => menu(e, w)}>
          <div class="head">
            {#if editing?.id === w.id && editing.what === 'label'}
              <input class="edit" bind:value={draft} use:focus onblur={() => (editing = null)} onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') commit(w); if (e.key === 'Escape') editing = null }} />
            {:else}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="label" title="Double-click to rename" ondblclick={() => start(w, 'label')}>{w.label}</span>
            {/if}
            <span class="sp"></span>
            {#if editing?.id === w.id && editing.what === 'value'}
              <input class="edit value-edit" bind:value={draft} use:focus spellcheck="false" title="Enter writes to memory · Esc cancels" onblur={() => (editing = null)} onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') commit(w); if (e.key === 'Escape') editing = null }} />
            {:else}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="value" class:gone={w.value == null} title={w.error ?? (canWrite ? 'Double-click to write a new value' : '')} ondblclick={() => start(w, 'value')}>{w.value ?? w.error}</span>
            {/if}
            <button class="frz" class:on={w.frozen != null} disabled={!canWrite} title={canWrite ? (w.frozen == null ? 'Freeze: write this value back every tick' : `Frozen at ${w.frozen}; click to release`) : 'The driver cannot write memory'} onclick={() => freeze(w)}>❄</button>
          </div>
          <div class="body">
            <svg width={W} height={H} viewBox="0 0 {W} {H}"><path d={s.d} /></svg>
            <div class="meta">
              <span title={w.expr}>{w.expr}</span>
              <span>{w.type}{w.address && w.address !== w.expr.replace(/^0x/i, '').toUpperCase() ? ` · 0x${w.address}` : ''}</span>
              {#if s.lo != null && s.hi !== s.lo}<span>{fmt(s.lo)} … {fmt(s.hi!)}</span>{/if}
            </div>
          </div>
        </div>
      {:else}
        <div class="none">Right-click a field or scan result and pick <i>Watch value</i>, or add an address below.</div>
      {/each}
    </div>

    <form onsubmit={add}>
      <input bind:value={expr} placeholder="address, e.g. [<game.exe>+0x10]+0x2C" spellcheck="false" />
      <select bind:value={type}>{#each TYPES as t (t)}<option value={t}>{t}</option>{/each}</select>
      <input class="lbl" bind:value={label} placeholder="label" spellcheck="false" />
      <button type="submit" disabled={!expr.trim()}>Add</button>
    </form>
  </div>
{/if}

<style>
  .panel { bottom: 60px; right: 308px; width: min(440px, calc(100vw - 330px)); max-height: 45vh; display: flex; flex-direction: column; padding: 12px; gap: 8px; }
  .top { display: flex; align-items: center; gap: 8px; }
  .sp { flex: 1; }
  .x { border: none; background: none; color: var(--faint); width: 22px; height: 22px; border-radius: 6px; cursor: pointer; }
  .x:hover { background: #ffffff14; color: var(--text); }
  .list { overflow: auto; min-height: 0; display: flex; flex-direction: column; gap: 4px; }
  .w { padding: 6px 8px; border-radius: 8px; background: #ffffff05; }
  .w:hover { background: #ffffff0a; }
  .head { display: flex; align-items: center; gap: 8px; }
  .label { color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: default; }
  .value { font: 13px var(--mono); color: var(--k-float); cursor: default; }
  .value.gone { color: var(--bad); font-size: 11px; }
  .frz { border: 1px solid var(--edge2); background: none; color: var(--faint); border-radius: 6px; width: 24px; height: 22px; cursor: pointer; }
  .frz.on { color: #8fd8ff; border-color: #8fd8ff; background: rgba(143, 216, 255, 0.12); }
  .frz:disabled { opacity: 0.35; cursor: default; }
  .body { display: flex; gap: 10px; align-items: center; margin-top: 3px; }
  svg { flex: none; background: #00000030; border-radius: 5px; }
  path { fill: none; stroke: var(--k-float); stroke-width: 1.5; }
  .meta { display: flex; flex-direction: column; min-width: 0; font: 10px var(--mono); color: var(--faint); }
  .meta span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .none { color: var(--faint); padding: 8px; font-size: 12px; }
  form { display: flex; gap: 6px; }
  form input, form select, .edit { background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 8px; padding: 5px 8px; font: 11.5px var(--mono); outline: none; min-width: 0; }
  form input { flex: 1; }
  form .lbl { flex: 0 0 80px; font-family: var(--ui); }
  form input:focus, form select:focus, .edit { border-color: var(--accent); }
  form button { border: 1px solid var(--accent); background: rgba(139, 123, 255, 0.18); color: var(--text); border-radius: 8px; padding: 5px 10px; cursor: pointer; }
  form button:disabled { opacity: 0.5; cursor: default; }
  .edit { padding: 2px 6px; }
  .value-edit { width: 110px; text-align: right; }
</style>
