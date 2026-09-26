<script lang="ts">
  import { app, call, copy, toast } from '../lib/store.svelte'
  import { focusCard, hue } from '../lib/geometry.svelte'
  import { openMenu } from '../lib/menu.svelte'
  import type { SignatureDef } from '../lib/types'

  let { onEditSignature }: { onEditSignature: (s: SignatureDef | null) => void } = $props()

  const counts = $derived.by(() => {
    const m = new Map<number, number>()
    for (const c of app.frame?.cards ?? []) m.set(c.classId, (m.get(c.classId) ?? 0) + 1)
    return m
  })

  const sortedClasses = $derived([...app.defs.classes].sort((a, b) => a.name.localeCompare(b.name)))

  let renaming = $state<number | null>(null)
  let draft = $state('')
  function focus(el: HTMLInputElement) {
    el.focus()
    el.select()
  }
  async function commit(id: number) {
    renaming = null
    if (draft.trim()) await call('renameClass', { classId: id, name: draft.trim() })
  }

  function showClass(id: number) {
    const card = app.frame?.cards.find((c) => c.classId === id)
    if (card) focusCard(card.id)
    else toast('Not on the canvas — follow a pointer to it, or make it the root')
  }
</script>

<div class="glass lib">
  <h5>Classes <button title="New class" onclick={() => call('addClass', {})}>＋</button></h5>
  {#each sortedClasses as c (c.id)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="li"
      class:unused={c.refs === 0}
      onclick={() => showClass(c.id)}
      oncontextmenu={(e) =>
        openMenu(e, [
          { header: `${c.name} · ${c.refs} reference${c.refs === 1 ? '' : 's'}` },
          { label: 'Make root', onClick: () => call('setRoot', { classId: c.id }) },
          { label: 'Rename', onClick: () => ((draft = c.name), (renaming = c.id)) },
          { sep: true },
          { label: 'Delete', danger: true, disabled: c.refs > 0, onClick: () => call('deleteClass', { classId: c.id }) },
          { label: 'Delete all unused', onClick: async () => toast(`Deleted ${await call('deleteUnusedClasses')} classes`) },
        ])}
    >
      <span class="dot" style:background={hue(c.id)}></span>
      {#if renaming === c.id}
        <input
          class="inline-edit"
          bind:value={draft}
          use:focus
          onclick={(e) => e.stopPropagation()}
          onblur={() => commit(c.id)}
          onkeydown={(e) => {
            e.stopPropagation()
            if (e.key === 'Enter') commit(c.id)
            if (e.key === 'Escape') renaming = null
          }}
        />
      {:else}
        <span class="name">{c.name}</span>
      {/if}
      {#if counts.get(c.id)}<span class="cnt">×{counts.get(c.id)}</span>{/if}
      {#if app.session?.rootClassId === c.id}<span class="star" title="Root class">★</span>{/if}
      <span class="m">{c.size.toString(16).toUpperCase()}h</span>
    </div>
  {/each}

  <h5>Signatures <button title="New signature" onclick={() => onEditSignature(null)}>＋</button></h5>
  {#each app.defs.signatures as s (s.name)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="li"
      title="{s.module}: {s.pattern}"
      onclick={() => copy(`$${s.name}`)}
      oncontextmenu={(e) =>
        openMenu(e, [
          { header: `$${s.name} · ${s.module}` },
          { label: 'Use as root', hint: `[$${s.name}]`, onClick: () => call('setRoot', { expr: `[$${s.name}]` }) },
          { label: 'Copy reference', onClick: () => copy(`$${s.name}`) },
          ...(s.value ? [{ label: 'Copy value', hint: s.value, onClick: () => copy(s.value!) }] : []),
          { label: 'Edit…', onClick: () => onEditSignature(s) },
          { label: 'Rescan all', onClick: () => call('rescan') },
          { sep: true },
          { label: 'Remove', danger: true, onClick: () => call('removeSignature', { name: s.name }) },
        ])}
    >
      <span class="dot round" style:background={s.error ? 'var(--bad)' : 'var(--good)'}></span>
      <span class="name">${s.name}</span>
      <span class="m" class:bad={!!s.error} title={s.error ?? s.value ?? ''}>{s.error ? 'missing' : s.value?.slice(-6)}</span>
    </div>
  {:else}
    <div class="none">No signatures yet</div>
  {/each}

  <h5>Enums <button title="New enum" onclick={async () => (app.editingEnum = await call<number>('addEnum', {}))}>＋</button></h5>
  {#each app.defs.enums as e (e.id)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="li"
      class:unused={e.refs === 0}
      title={e.variants.map(([n, v]) => `${n} = ${v}`).join('\n') || 'No variants'}
      onclick={() => (app.editingEnum = e.id)}
      oncontextmenu={(ev) =>
        openMenu(ev, [
          { header: `${e.name} · ${e.refs} field${e.refs === 1 ? '' : 's'}` },
          { label: 'Edit…', onClick: () => (app.editingEnum = e.id) },
          { sep: true },
          { label: 'Delete', danger: true, disabled: e.refs > 0, hint: e.refs > 0 ? 'in use' : '', onClick: () => call('deleteEnum', { enumId: e.id }) },
        ])}
    >
      <span class="dot" style:background="var(--k-enum)"></span>
      <span class="name">{e.name}</span>
      <span class="m">{e.isFlags ? 'flags · ' : ''}{e.variants.length}</span>
    </div>
  {:else}
    <div class="none">No enums yet</div>
  {/each}
</div>

<style>
  .lib { top: 76px; left: 14px; width: 230px; max-height: calc(100vh - 100px); overflow: auto; padding: 8px; }
  h5 { margin: 8px 6px 6px; font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--faint); display: flex; }
  h5 button { margin-left: auto; border: none; background: none; color: var(--dim); padding: 0; cursor: pointer; }
  h5 button:hover { color: var(--accent); }
  .li { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 8px; cursor: pointer; }
  .li:hover { background: #ffffff0a; }
  .li.unused { opacity: 0.5; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 8px; height: 8px; border-radius: 3px; flex: none; }
  .dot.round { border-radius: 50%; }
  .m { margin-left: auto; font: 10px var(--mono); color: var(--faint); }
  .m.bad { color: var(--bad); }
  .cnt { font-size: 10px; color: var(--accent); }
  .star { color: #ffd86b; font-size: 10px; }
  .none { color: var(--faint); padding: 4px 8px; font-size: 11px; }
  .inline-edit { background: var(--bg); color: var(--text); border: 1px solid var(--accent); border-radius: 4px; font: 12px var(--ui); outline: none; padding: 0 4px; height: 20px; width: 120px; }
</style>
