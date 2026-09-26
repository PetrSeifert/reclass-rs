<script lang="ts">
  import { app, call, toast } from '../lib/store.svelte'
  import { fit, tidy } from '../lib/geometry.svelte'

  let { onAttach }: { onAttach: () => void } = $props()

  const s = $derived(app.session)
  const f = $derived(app.frame)

  // Local draft of the root expression; sent to the server while typing.
  let expr = $state('')
  let focused = $state(false)
  $effect(() => {
    if (!focused && s) expr = s.rootExpr
  })
  let timer: ReturnType<typeof setTimeout> | undefined
  function onInput() {
    clearTimeout(timer)
    timer = setTimeout(() => call('setRoot', { expr }), 180)
  }

  async function save() {
    if (!s?.projectPath) {
      const path = prompt('Save project to (path on the server machine):', 'memory_structure.json')
      if (!path) return
      await call('save', { path })
    } else {
      await call('save', {})
    }
    toast('Project saved')
  }
</script>

<div class="glass brand">
  <div class="logo"></div>
  <b>reclass</b>
  <button class="proc" onclick={onAttach} title="Attach to a process">
    {#if s?.attached}
      <i></i>{s.attached.name} <span class="pid">{s.attached.pid}</span>
    {:else}
      <i class="off"></i>Attach…
    {/if}
  </button>
</div>

<div class="glass toolbar">
  <select
    title="Root class"
    value={s?.rootClassId}
    onchange={(e) => call('setRoot', { classId: +(e.currentTarget as HTMLSelectElement).value })}
  >
    {#each app.defs.classes as c (c.id)}
      <option value={c.id}>{c.name}</option>
    {/each}
  </select>
  <div class="expr" class:bad={!!f?.rootError}>
    <span class="at">@</span>
    <input
      bind:value={expr}
      spellcheck="false"
      oninput={onInput}
      onfocus={() => (focused = true)}
      onblur={() => (focused = false)}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()}
      placeholder="[$Signature] + 0x10"
    />
    <span class="res" class:bad={!!f?.rootError} title={f?.rootError ?? ''}>{f?.rootError ?? f?.rootAddress ?? ''}</span>
  </div>
  <div class="vsep"></div>
  <button class="tbtn" class:on={s?.live} title="Live updates" onclick={() => call('setLive', { on: !s?.live })}>
    {s?.live ? '● Live' : '❚❚ Paused'}
  </button>
  <button class="tbtn" title="Auto-layout (T)" onclick={tidy}>⌘ Tidy</button>
  <button class="tbtn" title="Fit to screen (F)" onclick={fit}>⤢ Fit</button>
  <button class="tbtn" title="Close all cards but the root" onclick={() => call('closeAll')}>⊖</button>
  <div class="vsep"></div>
  <button
    class="tbtn"
    class:on={s?.share}
    title="When on, following a pointer to an instance that is already open links to that card instead of opening a copy"
    onclick={() => call('setShare', { on: !s?.share })}
  >
    ⧉ {s?.share ? 'Share cards' : 'Separate cards'}
  </button>
  <button class="tbtn" class:active={app.modulesOpen} title="Loaded modules (M)" onclick={() => (app.modulesOpen = !app.modulesOpen)}>▣ Modules</button>
  <button class="tbtn" title="Save project (Ctrl+S)" onclick={save}>
    {s?.dirty ? '● ' : ''}Save
  </button>
</div>

<style>
  .brand { top: 14px; left: 14px; display: flex; align-items: center; gap: 10px; padding: 8px 12px 8px 10px; }
  .logo {
    width: 26px; height: 26px; border-radius: 8px;
    background: radial-gradient(circle at 30% 30%, #fff 0 2px, transparent 3px), radial-gradient(circle at 70% 60%, #fff 0 1.5px, transparent 2.5px),
      radial-gradient(circle at 40% 75%, #fff 0 1px, transparent 2px), linear-gradient(135deg, var(--accent), var(--accent2));
  }
  .brand b { font-size: 13px; }
  .proc { display: flex; gap: 6px; align-items: center; padding: 4px 10px; border-radius: 999px; background: #ffffff0a; border: none; cursor: pointer; }
  .proc:hover { background: #ffffff14; }
  .proc i { width: 7px; height: 7px; border-radius: 50%; background: var(--good); box-shadow: 0 0 8px var(--good); }
  .proc i.off { background: var(--faint); box-shadow: none; }
  .pid { color: var(--faint); }
  .toolbar { top: 14px; left: 50%; transform: translateX(-50%); display: flex; align-items: center; gap: 4px; padding: 6px; white-space: nowrap; }
  .expr { display: flex; align-items: center; gap: 8px; background: #00000040; border: 1px solid var(--edge); border-radius: 9px; padding: 0 10px; height: 32px; }
  .expr:focus-within { border-color: var(--accent); }
  .expr.bad { border-color: var(--bad); }
  .expr input { background: none; border: none; outline: none; color: var(--text); font: 12px var(--mono); width: 170px; }
  .at { color: var(--faint); }
  .res { font: 11px var(--mono); color: var(--good); max-width: 200px; overflow: hidden; text-overflow: ellipsis; }
  .res.bad { color: var(--bad); }
  select { background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 9px; height: 32px; padding: 0 8px; font: 12px var(--ui); outline: none; max-width: 150px; }
  .tbtn.active { color: var(--text); background: #ffffff14; }
  .vsep { width: 1px; height: 20px; background: var(--edge2); margin: 0 4px; }
</style>
