<script lang="ts">
  import { app, call, toast } from '../lib/store.svelte'

  const def = $derived(app.defs.enums.find((e) => e.id === app.editingEnum))

  let name = $state('')
  let isFlags = $state(false)
  let size = $state(4)
  let variants = $state<{ name: string; value: string }[]>([])
  let loadedFor: number | null = null

  // Load the draft when the editor opens (not on every defs update, which would wipe edits).
  $effect(() => {
    const d = def
    if (!d || loadedFor === d.id) return
    loadedFor = d.id
    name = d.name
    isFlags = d.isFlags
    size = d.size
    variants = d.variants.map(([n, v]) => ({ name: n, value: d.isFlags ? `0x${v.toString(16).toUpperCase()}` : String(v) }))
  })
  $effect(() => {
    if (app.editingEnum == null) loadedFor = null
  })

  const parse = (s: string) => {
    const t = s.trim().toLowerCase()
    const n = t.startsWith('0x') ? parseInt(t.slice(2), 16) : parseInt(t, 10)
    return Number.isFinite(n) && n >= 0 && n <= 0xffffffff ? n : NaN
  }

  function addVariant() {
    const values = variants.map((v) => parse(v.value)).filter((n) => !Number.isNaN(n))
    const next = isFlags
      ? values.length ? 2 ** Math.floor(Math.log2(Math.max(...values)) + 1) : 1
      : values.length ? Math.max(...values) + 1 : 0
    variants.push({ name: '', value: isFlags ? `0x${next.toString(16).toUpperCase()}` : String(next) })
  }

  const close = () => (app.editingEnum = null)

  async function save() {
    const bad = variants.find((v) => Number.isNaN(parse(v.value)))
    if (bad) return toast(`Invalid value for ${bad.name || 'a variant'}: ${bad.value}`, true)
    await call('updateEnum', {
      enumId: def!.id,
      name,
      isFlags,
      size,
      variants: variants.map((v) => [v.name, parse(v.value)]),
    })
    toast(`${name.trim()} saved`)
    close()
  }

  function focus(el: HTMLInputElement) {
    el.focus()
    el.select()
  }
</script>

{#if app.editingEnum != null && def}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal" onclick={(e) => e.target === e.currentTarget && close()}>
    <form class="glass box" onsubmit={(e) => (e.preventDefault(), save())}>
      <h3>Enum <span class="refs">{def.refs} field{def.refs === 1 ? '' : 's'} use it</span></h3>
      <div class="row">
        <label class="grow">Name <input bind:value={name} use:focus required /></label>
        <label>Size
          <select bind:value={size}>
            {#each [1, 2, 4, 8] as n (n)}<option value={n}>{n} byte{n > 1 ? 's' : ''}</option>{/each}
          </select>
        </label>
      </div>
      <label class="check"><input type="checkbox" bind:checked={isFlags} /> Flags (show every set bit, e.g. <code>OnGround | Sprinting</code>)</label>

      <div class="head"><span>Variant</span><span>Value</span><span></span></div>
      <div class="variants">
        {#each variants as v, i (i)}
          <div class="variant">
            <input bind:value={v.name} placeholder="Name" required />
            <input bind:value={v.value} class="mono" class:bad={Number.isNaN(parse(v.value))} placeholder="0" />
            <button type="button" class="x" title="Remove" onclick={() => variants.splice(i, 1)}>✕</button>
          </div>
        {:else}
          <div class="none">No variants yet</div>
        {/each}
      </div>
      <button type="button" class="add" onclick={addVariant}>＋ Add variant</button>

      <div class="actions">
        {#if def.refs === 0}
          <button type="button" class="danger" onclick={async () => (await call('deleteEnum', { enumId: def.id }), close())}>Delete</button>
        {/if}
        <span class="sp"></span>
        <button type="button" class="tbtn" onclick={close}>Cancel</button>
        <button type="submit" class="primary">Save</button>
      </div>
    </form>
  </div>
{/if}

<style>
  .modal { position: fixed; inset: 0; background: rgba(5, 6, 10, 0.55); display: grid; place-items: center; z-index: 90; }
  .box { position: static; width: 460px; max-height: 80vh; display: flex; flex-direction: column; gap: 10px; padding: 16px; }
  h3 { margin: 0; font-size: 14px; display: flex; align-items: baseline; gap: 10px; }
  .refs { font-size: 11px; color: var(--faint); font-weight: 400; }
  .row { display: flex; gap: 10px; }
  .grow { flex: 1; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--dim); }
  label.check { flex-direction: row; align-items: center; gap: 8px; }
  label.check input { width: auto; }
  code { font: 11px var(--mono); color: var(--text); }
  input, select { background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 8px; padding: 7px 10px; font: 13px var(--ui); outline: none; width: 100%; }
  input:focus, select:focus { border-color: var(--accent); }
  .mono { font-family: var(--mono); font-size: 12px; }
  input.bad { border-color: var(--bad); }
  .head, .variant { display: grid; grid-template-columns: 1fr 110px 28px; gap: 6px; align-items: center; }
  .head { font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--faint); margin-top: 4px; }
  .variants { display: flex; flex-direction: column; gap: 6px; overflow: auto; min-height: 0; }
  .x { border: none; background: none; color: var(--faint); border-radius: 6px; height: 28px; cursor: pointer; }
  .x:hover { background: #ffffff14; color: var(--bad); }
  .add { align-self: flex-start; border: 1px dashed var(--edge2); background: none; color: var(--dim); border-radius: 8px; padding: 5px 10px; cursor: pointer; }
  .add:hover { color: var(--text); border-color: var(--accent); }
  .none { color: var(--faint); padding: 4px 0; }
  .actions { display: flex; gap: 8px; align-items: center; margin-top: 4px; }
  .sp { flex: 1; }
  .primary { background: var(--accent); color: #fff; border: none; border-radius: 9px; padding: 8px 14px; cursor: pointer; }
  .danger { background: none; border: 1px solid var(--bad); color: var(--bad); border-radius: 9px; padding: 7px 12px; cursor: pointer; }
</style>
