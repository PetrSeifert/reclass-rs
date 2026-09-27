<script lang="ts">
  import { untrack } from 'svelte'
  import { app, call } from '../lib/store.svelte'
  import { editValue, fieldMenu, follow, openMenu, renaming, writeBlock, writing } from '../lib/menu.svelte'
  import { SHORT, type CardView, type Row } from '../lib/types'

  let { card, row }: { card: CardView; row: Row } = $props()

  const selected = $derived(app.selected?.card === card.id && app.selected?.key === row.key)
  const editing = $derived(renaming.card === card.id && renaming.key === row.key)
  const writingValue = $derived(writing.card === card.id && writing.key === row.key)

  // Flash the value when it changes (not on first render).
  let valueEl: HTMLSpanElement | undefined = $state()
  let last: string | undefined
  $effect(() => {
    const v = row.value
    if (last !== undefined && last !== v && valueEl) {
      valueEl.animate(
        [{ background: 'rgba(58,208,201,.28)', color: '#fff' }, { background: 'transparent' }],
        { duration: 800, easing: 'ease-out' },
      )
    }
    last = v
  })

  let draft = $state('')
  $effect(() => {
    if (editing) draft = row.name ?? ''
  })
  function focus(el: HTMLInputElement) {
    el.focus()
    el.select()
  }
  async function commit() {
    const name = draft.trim()
    renaming.key = ''
    if (name && name !== row.name && row.classId != null) {
      await call('renameField', { classId: row.classId, fieldId: row.fieldId, name })
    }
  }

  // Start from the value shown when editing began; live frames must not overwrite typing.
  let value = $state('')
  $effect(() => {
    if (writingValue) value = untrack(() => row.value)
  })
  async function write() {
    const typed = value
    writing.key = ''
    if (typed.trim() && typed !== row.value) await call('write', { card: card.id, key: row.key, value: typed })
  }

  function select(e: MouseEvent) {
    if (e.button !== 0) return
    app.selected = { card: card.id, key: row.key }
  }

  function dblclick(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.v') && !writeBlock(row)) {
      editValue(card, row)
    } else if ((e.target as HTMLElement).closest('.n') && row.classId != null && !row.ty.startsWith('Hex')) {
      Object.assign(renaming, { card: card.id, key: row.key })
    } else if (row.port && row.port.state !== 'null') {
      follow(card, row)
    } else if (row.expandable) {
      call('toggleExpand', { card: card.id, key: row.key })
    }
  }

  /** What an untyped hex value probably is, colored like the type it suggests. */
  const guesses = $derived(
    row.cat !== 'hex'
      ? []
      : row.hints.map((h) =>
          h.startsWith('-> ') ? { text: `→ ${h.slice(3)}`, cat: 'ptr' }
          : h.startsWith('f32 ') ? { text: h, cat: 'float' }
          : h.startsWith("'") ? { text: h, cat: 'text' }
          : { text: h, cat: 'int' },
        ),
  )

  const portTitle: Record<string, string> = {
    closed: 'Follow pointer',
    open: 'Linked — click to unlink',
    stale: 'Pointer moved away from the linked card — click to re-follow',
    known: 'Target is already on the canvas — click to link to it',
    null: 'Null or invalid pointer',
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="r"
  class:on={selected}
  style:padding-left="{12 + row.depth * 14}px"
  onmousedown={select}
  ondblclick={dblclick}
  oncontextmenu={(e) => {
    app.selected = { card: card.id, key: row.key }
    openMenu(e, fieldMenu(card, row))
  }}
>
  <span class="bar" style:background="var(--k-{row.cat})" style:opacity={row.cat === 'hex' ? 0.3 : 0.9}></span>
  <span class="o">{row.offset.toString(16).toUpperCase().padStart(3, '0')}</span>
  <span class="nm">
    {#if row.expandable}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <span class="tg" onmousedown={(e) => e.stopPropagation()} onclick={() => call('toggleExpand', { card: card.id, key: row.key })}>{row.open ? '▾' : '▸'}</span>
    {/if}
    {#if editing}
      <input
        class="inline-edit"
        bind:value={draft}
        use:focus
        onmousedown={(e) => e.stopPropagation()}
        onblur={commit}
        onkeydown={(e) => {
          e.stopPropagation()
          if (e.key === 'Enter') commit()
          if (e.key === 'Escape') renaming.key = ''
        }}
      />
    {:else if row.cat !== 'hex'}
      <!-- Hex fields have no name; their type label says the rest. -->
      <span class="n" class:anon={!row.name}>{row.name ?? SHORT[row.ty]}</span>
    {/if}
    <span class="t" style:color="var(--k-{row.cat})">{row.typeLabel}</span>
    {#if guesses.length}
      <span class="g" title={guesses.map((g) => g.text).join(' · ')}>{#each guesses as g, i (i)}<span style:color="var(--k-{g.cat})">{g.text}</span>{/each}</span>
    {/if}
  </span>
  {#if writingValue}
    <input
      class="inline-edit value-edit"
      bind:value
      use:focus
      spellcheck="false"
      title="Enter writes to memory · Esc cancels"
      onmousedown={(e) => e.stopPropagation()}
      ondblclick={(e) => e.stopPropagation()}
      onblur={() => (writing.key = '')}
      onkeydown={(e) => {
        e.stopPropagation()
        if (e.key === 'Enter') write()
        if (e.key === 'Escape') writing.key = ''
      }}
    />
  {:else}
    <span class="v" class:err={!!row.error} bind:this={valueEl} title={[row.error, ...row.hints].filter(Boolean).join(' · ')}>{row.value}</span>
  {/if}
  {#if row.port}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <span
      class="port {row.port.state}"
      class:enc={row.port.encrypted}
      title={portTitle[row.port.state]}
      onmousedown={(e) => e.stopPropagation()}
      onclick={() => row.port!.state !== 'null' && follow(card, row)}
    ></span>
  {/if}
</div>

<style>
  .r {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) auto;
    align-items: center;
    column-gap: 8px;
    padding-right: 22px;
    height: 24px;
    position: relative;
    font-size: 12px;
    cursor: default;
  }
  .r:hover { background: #ffffff06; }
  .r.on { background: rgba(139, 123, 255, 0.14); }
  .o { font: 10px var(--mono); color: var(--faint); }
  .nm { display: flex; gap: 6px; align-items: baseline; min-width: 0; overflow: hidden; white-space: nowrap; }
  .n { color: var(--text); overflow: hidden; text-overflow: ellipsis; }
  .n.anon { color: var(--faint); }
  .t { font: 10px var(--mono); flex: none; }
  .g { margin-left: auto; display: flex; gap: 8px; min-width: 0; overflow: hidden; font: 10.5px var(--mono); opacity: 0.75; }
  .g > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tg { color: var(--dim); width: 10px; flex: none; cursor: pointer; }
  .tg:hover { color: var(--text); }
  .v { font: 11.5px var(--mono); color: #cfd5e2; text-align: right; max-width: 175px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border-radius: 3px; padding: 0 2px; }
  .v.err { color: var(--bad); }
  .bar { position: absolute; left: 0; top: 5px; bottom: 5px; width: 2px; border-radius: 2px; }
  .port { position: absolute; right: -7px; top: 5px; width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--k-ptr); background: var(--card); cursor: pointer; z-index: 2; transition: transform 0.1s; }
  .port:hover { transform: scale(1.25); }
  .port.open { background: var(--k-ptr); }
  .port.known { background: radial-gradient(circle, var(--k-ptr) 0 2.5px, var(--card) 3px); border-style: dotted; }
  .port.stale { border-color: var(--bad); background: var(--bad); animation: blinkp 1.2s infinite; }
  .port.null { border-color: var(--faint); cursor: not-allowed; }
  .port.enc { border-color: var(--accent2); border-style: dashed; }
  .port.enc.open { background: var(--accent2); }
  @keyframes blinkp { 50% { box-shadow: 0 0 0 4px rgba(255, 107, 125, 0.25); } }
  .inline-edit.value-edit { width: 175px; font: 11.5px var(--mono); text-align: right; }
  .inline-edit { background: var(--bg); color: var(--text); border: 1px solid var(--accent); border-radius: 4px; font: 12px var(--ui); outline: none; padding: 0 4px; height: 19px; width: 130px; }
</style>
