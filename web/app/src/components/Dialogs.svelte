<script lang="ts">
  import { app, call, toast } from '../lib/store.svelte'
  import type { ProcessEntry, SignatureDef } from '../lib/types'

  let {
    mode = $bindable(),
    signature,
  }: { mode: 'attach' | 'signature' | null; signature: SignatureDef | null } = $props()

  // ---- attach
  let processes = $state<ProcessEntry[]>([])
  let query = $state('')
  $effect(() => {
    if (mode === 'attach') {
      query = ''
      call<ProcessEntry[]>('processes').then((p) => (processes = p), () => (mode = null))
    }
  })
  const filtered = $derived(processes.filter((p) => `${p.name} ${p.pid}`.toLowerCase().includes(query.toLowerCase())))
  async function attach(pid: number) {
    await call('attach', { pid })
    mode = null
    toast('Attached')
  }

  // ---- signature editor
  let sig = $state({ name: '', module: '', pattern: '', offset: '0', isRelative: true, relInstLen: '7' })
  $effect(() => {
    if (mode === 'signature') {
      const s = signature
      sig = s
        ? { name: s.name, module: s.module, pattern: s.pattern, offset: String(s.offset), isRelative: s.isRelative, relInstLen: String(s.relInstLen) }
        : { name: '', module: app.session?.attached?.name ?? '', pattern: '', offset: '3', isRelative: true, relInstLen: '7' }
    }
  })
  const num = (s: string) => (s.trim().toLowerCase().startsWith('0x') ? parseInt(s, 16) : parseInt(s, 10)) || 0
  async function saveSignature() {
    await call('setSignature', {
      replace: signature?.name ?? null,
      signature: {
        name: sig.name.trim(),
        module: sig.module.trim(),
        pattern: sig.pattern.trim(),
        offset: num(sig.offset),
        is_relative: sig.isRelative,
        rel_inst_len: num(sig.relInstLen),
      },
    })
    mode = null
    // The rescanned definitions arrive in a separate message right after the reply.
    const name = sig.name.trim()
    setTimeout(() => {
      const s = app.defs.signatures.find((x) => x.name === name)
      toast(s?.error ? `$${name}: ${s.error}` : `$${name} resolved to ${s?.value}`, !!s?.error)
    }, 200)
  }

  function focus(el: HTMLInputElement) {
    el.focus()
  }
</script>

{#if mode}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal" onclick={(e) => e.target === e.currentTarget && (mode = null)} onkeydown={(e) => e.key === 'Escape' && (mode = null)}>
    {#if mode === 'attach'}
      <div class="glass box">
        <input bind:value={query} placeholder="Attach to process…" use:focus />
        <div class="list">
          {#each filtered as p (p.pid)}
            <button class="li" onclick={() => attach(p.pid)}>
              <span class="dot" class:on={app.session?.attached?.pid === p.pid}></span>{p.name}<span class="m">{p.pid}</span>
            </button>
          {:else}
            <div class="none">No matching processes</div>
          {/each}
        </div>
      </div>
    {:else}
      <form class="glass box form" onsubmit={(e) => (e.preventDefault(), saveSignature())}>
        <h3>{signature ? `Edit $${signature.name}` : 'New signature'}</h3>
        <label>Name <input bind:value={sig.name} placeholder="GWorld" use:focus required /></label>
        <label>Module <input bind:value={sig.module} placeholder="game.exe" required /></label>
        <label>Pattern <input bind:value={sig.pattern} placeholder="48 8B 05 ?? ?? ?? ??" required /></label>
        <div class="row2">
          <label>Offset <input bind:value={sig.offset} /></label>
          <label>Instruction length <input bind:value={sig.relInstLen} disabled={!sig.isRelative} /></label>
        </div>
        <label class="check"><input type="checkbox" bind:checked={sig.isRelative} /> RIP-relative (resolve to the address the instruction points at)</label>
        <div class="actions">
          <button type="button" class="tbtn" onclick={() => (mode = null)}>Cancel</button>
          <button type="submit" class="primary">Save &amp; scan</button>
        </div>
      </form>
    {/if}
  </div>
{/if}

<style>
  .modal { position: fixed; inset: 0; background: rgba(5, 6, 10, 0.55); display: grid; place-items: center; z-index: 90; }
  .box { position: static; width: 400px; max-height: 60vh; display: flex; flex-direction: column; padding: 10px; }
  input { background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 9px; padding: 8px 12px; font: 13px var(--ui); outline: none; box-sizing: border-box; width: 100%; }
  input:focus { border-color: var(--accent); }
  .list { overflow: auto; margin-top: 6px; }
  .li { display: flex; width: 100%; align-items: center; gap: 8px; padding: 7px 8px; border-radius: 8px; background: none; border: none; text-align: left; cursor: pointer; }
  .li:hover { background: #ffffff0f; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--faint); }
  .dot.on { background: var(--good); }
  .m { margin-left: auto; font: 11px var(--mono); color: var(--faint); }
  .none { color: var(--faint); padding: 8px; }
  .form { width: 460px; gap: 10px; padding: 16px; }
  h3 { margin: 0 0 4px; font-size: 14px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--dim); }
  label.check { flex-direction: row; align-items: center; gap: 8px; }
  label.check input { width: auto; }
  .row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
  .primary { background: var(--accent); color: #fff; border: none; border-radius: 9px; padding: 8px 14px; cursor: pointer; }
  .primary:hover { filter: brightness(1.1); }
</style>
