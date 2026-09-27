<script lang="ts">
  import { app, call, changeToken, connect, selectedRow, toast } from './lib/store.svelte'
  import { fit, tidy } from './lib/geometry.svelte'
  import { closeMenu, editValue, menu, renaming } from './lib/menu.svelte'
  import type { SignatureDef } from './lib/types'
  import Canvas from './components/Canvas.svelte'
  import Toolbar from './components/Toolbar.svelte'
  import Library from './components/Library.svelte'
  import Inspector from './components/Inspector.svelte'
  import Minimap from './components/Minimap.svelte'
  import MenuList from './components/MenuList.svelte'
  import Dialogs from './components/Dialogs.svelte'
  import EnumEditor from './components/EnumEditor.svelte'
  import Modules from './components/Modules.svelte'
  import Watches from './components/Watches.svelte'
  import Scanner from './components/Scanner.svelte'

  let dialog = $state<'attach' | 'signature' | null>(null)
  let editingSignature = $state<SignatureDef | null>(null)

  let apiToken = $state('')

  // Fit once when the first frame arrives.
  let fitted = false
  $effect(() => {
    if (!fitted && app.frame?.cards.length) {
      fitted = true
      requestAnimationFrame(fit)
    }
  })

  function keydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement).closest('input, textarea, select')
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault()
      const path = app.session?.projectPath ?? prompt('Save project to (path on the server machine):', 'memory_structure.json')
      if (path) call('save', { path }).then(() => toast('Project saved'))
      return
    }
    if (e.key === 'Escape') {
      closeMenu()
      dialog = null
      app.editingEnum = null
      return
    }
    if (typing || e.ctrlKey || e.metaKey || e.altKey) return
    const sel = selectedRow()
    if (e.key === 'f') fit()
    else if (e.key === 'm') app.modulesOpen = !app.modulesOpen
    else if (e.key === 's') app.scannerOpen = !app.scannerOpen
    else if (e.key === 'w') app.watchesOpen = !app.watchesOpen
    else if (e.key === 't') tidy()
    else if (e.key === 'F2' && sel && sel.row.classId != null && !sel.row.ty.startsWith('Hex')) {
      e.preventDefault()
      Object.assign(renaming, { card: sel.card.id, key: sel.row.key })
    } else if (e.key === 'Enter' && sel) {
      e.preventDefault()
      editValue(sel.card, sel.row)
    } else if (e.key === 'Delete' && sel?.row.classId != null) {
      call('removeField', { classId: sel.row.classId, fieldId: sel.row.fieldId })
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && sel) {
      e.preventDefault()
      const rows = sel.card.rows
      const i = rows.findIndex((r) => r.key === sel.row.key) + (e.key === 'ArrowDown' ? 1 : -1)
      if (rows[i]) app.selected = { card: sel.card.id, key: rows[i].key }
    }
  }
</script>

<svelte:window onkeydown={keydown} onmousedown={(e) => !(e.target as HTMLElement).closest('.menu-root') && closeMenu()} />

<Canvas />
<Toolbar onAttach={() => (dialog = 'attach')} />
<Library onEditSignature={(s) => ((editingSignature = s), (dialog = 'signature'))} />
<Inspector />
<Minimap />
<Modules />
<Watches />
<Scanner />
<div class="glass hint">
  Click a <b style:color="var(--k-ptr)">●</b> port to follow a pointer · drag to pan · wheel to zoom · right-click for more
</div>

{#if menu.open}
  <div class="menu-root" style:left="{menu.x}px" style:top="{menu.y}px">
    <MenuList items={menu.items} />
  </div>
{/if}

<Dialogs bind:mode={dialog} signature={editingSignature} />
<EnumEditor />

{#if !app.connected}
  <form class="banner" onsubmit={(event) => { event.preventDefault(); connect(apiToken); apiToken = '' }}>
    {#if app.connecting}
      Connecting to reclass-server…
      <button type="button" onclick={changeToken}>Change token</button>
    {:else}
      <label>API token <input type="password" bind:value={apiToken} autocomplete="off" spellcheck="false" required /></label>
      <button type="submit">Connect</button>
      {#if app.connectionError}<div role="alert">{app.connectionError}</div>{/if}
    {/if}
  </form>
{:else if app.session && !app.session.attached}
  <div class="banner info">
    Not attached to a process. <button onclick={() => (dialog = 'attach')}>Attach…</button>
  </div>
{/if}

{#if app.toast}
  {#key app.toast.n}
    <div class="toast" class:error={app.toast.error}>{app.toast.text}</div>
  {/key}
{/if}

<style>
  .hint { bottom: 14px; left: 50%; transform: translateX(-50%); padding: 7px 14px; color: var(--dim); font-size: 11px; border-radius: 999px; white-space: nowrap; }
  .menu-root { position: fixed; z-index: 100; }
  .banner { position: fixed; top: 70px; left: 50%; transform: translateX(-50%); z-index: 80; background: rgba(255, 107, 125, 0.15); border: 1px solid var(--bad); color: var(--text); padding: 8px 16px; border-radius: 999px; font-size: 12px; }
  .banner.info { background: rgba(139, 123, 255, 0.15); border-color: var(--accent); }
  .banner button { background: var(--accent); border: none; color: #fff; border-radius: 999px; padding: 2px 10px; margin-left: 6px; cursor: pointer; }
  .toast { position: fixed; bottom: 60px; left: 50%; transform: translateX(-50%); background: var(--accent); color: #fff; padding: 8px 16px; border-radius: 999px; font-size: 12px; z-index: 1000; box-shadow: 0 6px 24px rgba(139, 123, 255, 0.4); pointer-events: none; animation: pop 0.2s ease-out; max-width: 70vw; }
  .toast.error { background: #c43d52; box-shadow: 0 6px 24px rgba(255, 107, 125, 0.35); }
  @keyframes pop { from { opacity: 0; transform: translate(-50%, 8px); } }
</style>
