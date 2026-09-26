<script lang="ts">
  import { app, call, copy } from '../lib/store.svelte'
  import { openMenu } from '../lib/menu.svelte'

  interface Module { name: string; base: string; size: number }

  let modules = $state<Module[]>([])
  let query = $state('')
  let sort = $state<'base' | 'name' | 'size'>('base')
  let loading = $state(false)

  // Reload whenever the panel opens or the attached process changes.
  $effect(() => {
    const pid = app.session?.attached?.pid
    if (!app.modulesOpen) return
    if (pid == null) {
      modules = []
      return
    }
    loading = true
    call<Module[]>('modules').then(
      (m) => ((modules = m), (loading = false)),
      () => (loading = false),
    )
  })

  const big = (h: string) => BigInt('0x' + h)
  const shown = $derived(
    modules
      .filter((m) => `${m.name} ${m.base}`.toLowerCase().includes(query.trim().toLowerCase()))
      .sort((a, b) =>
        sort === 'name' ? a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
        : sort === 'size' ? b.size - a.size
        : big(a.base) < big(b.base) ? -1 : 1,
      ),
  )

  const hex = (n: number) => '0x' + n.toString(16).toUpperCase()
  const kb = (n: number) => (n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`)

  function menu(e: MouseEvent, m: Module) {
    openMenu(e, [
      { header: `${m.name} · ${hex(m.size)} bytes` },
      { label: 'Copy reference', hint: `<${m.name}>`, onClick: () => copy(`<${m.name}>`) },
      { label: 'Copy base address', hint: m.base, onClick: () => copy(m.base) },
      { sep: true },
      { label: 'Use as root expression', hint: `<${m.name}>`, onClick: () => call('setRoot', { expr: `<${m.name}>` }) },
    ])
  }
</script>

{#if app.modulesOpen}
  <div class="glass panel">
    <div class="top">
      <b>Modules</b>
      <span class="count">{app.session?.attached ? `${shown.length}/${modules.length}` : ''}</span>
      <span class="sp"></span>
      <button class="x" title="Close (M)" onclick={() => (app.modulesOpen = false)}>✕</button>
    </div>
    <input bind:value={query} placeholder="Filter by name or address…" spellcheck="false" />
    <div class="cols">
      {#each [['name', 'Name'], ['base', 'Base'], ['size', 'Size']] as [key, label] (key)}
        <button class:on={sort === key} onclick={() => (sort = key as typeof sort)}>{label}{sort === key ? ' ↓' : ''}</button>
      {/each}
    </div>
    <div class="list">
      {#if !app.session?.attached}
        <div class="none">Attach to a process to see its modules.</div>
      {:else if loading && !modules.length}
        <div class="none">Loading…</div>
      {:else}
        {#each shown as m (m.base)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="row" title="Click to copy <{m.name}> · right-click for more" onclick={() => copy(`<${m.name}>`)} oncontextmenu={(e) => menu(e, m)}>
            <span class="name">{m.name}</span>
            <span class="base">{m.base}</span>
            <span class="size">{kb(m.size)}</span>
          </div>
        {:else}
          <div class="none">No matching modules</div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .panel { top: 76px; right: 308px; width: 380px; max-height: calc(100vh - 250px); display: flex; flex-direction: column; padding: 12px; gap: 8px; }
  .top { display: flex; align-items: center; gap: 8px; }
  .count { font: 11px var(--mono); color: var(--faint); }
  .sp { flex: 1; }
  .x { border: none; background: none; color: var(--faint); width: 22px; height: 22px; border-radius: 6px; cursor: pointer; }
  .x:hover { background: #ffffff14; color: var(--text); }
  input { background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 8px; padding: 7px 10px; font: 12px var(--ui); outline: none; }
  input:focus { border-color: var(--accent); }
  .cols, .row { display: grid; grid-template-columns: minmax(0, 1fr) 120px 64px; gap: 8px; align-items: center; }
  .cols { padding: 0 8px; }
  .cols button { background: none; border: none; padding: 0; text-align: left; font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--faint); cursor: pointer; }
  .cols button.on, .cols button:hover { color: var(--text); }
  .list { overflow: auto; min-height: 0; }
  .row { padding: 5px 8px; border-radius: 7px; cursor: pointer; }
  .row:hover { background: #ffffff0a; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .base { font: 11px var(--mono); color: var(--k-int); }
  .size { font: 11px var(--mono); color: var(--dim); text-align: right; }
  .none { color: var(--faint); padding: 8px; }
</style>
