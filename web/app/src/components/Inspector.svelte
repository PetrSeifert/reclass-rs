<script lang="ts">
  import { app, call, copy, selectedRow } from '../lib/store.svelte'
  import { follow } from '../lib/menu.svelte'
  import type { FieldType } from '../lib/types'

  const sel = $derived(selectedRow())
  const row = $derived(sel?.row)
  const editable = $derived(row?.classId != null)

  const COMMON: FieldType[] = [
    'Hex64', 'Hex32', 'Int32', 'UInt32', 'Float', 'Double', 'Bool', 'Vector3',
    'Pointer', 'TextPointer', 'Int64', 'Vector2', 'Vector4', 'Text', 'EncryptedPointer', 'Enum', 'Array', 'ClassInstance',
  ]
  const SHORT_LABEL: Partial<Record<FieldType, string>> = {
    Hex64: 'hex64', Hex32: 'hex32', Int32: 'i32', UInt32: 'u32', Float: 'f32', Double: 'f64', Bool: 'bool', Vector3: 'vec3',
    Pointer: 'ptr', TextPointer: 'char*', Int64: 'i64', Vector2: 'vec2', Vector4: 'vec4', Text: 'char[32]',
    EncryptedPointer: 'eptr', Enum: 'enum', Array: 'array', ClassInstance: 'class',
  }

  const interp = $derived.by(() => {
    if (!row?.bytes) return []
    const b = new Uint8Array(row.bytes.match(/../g)!.map((h) => parseInt(h, 16)))
    const pad = new Uint8Array(Math.max(8, b.length))
    pad.set(b)
    const v = new DataView(pad.buffer)
    const f = (n: number) => (Math.abs(n) >= 1e7 || (n !== 0 && Math.abs(n) < 1e-4) ? n.toExponential(3) : String(+n.toFixed(4)))
    return [
      ['i8 / u8', `${v.getInt8(0)} / ${v.getUint8(0)}`],
      ['i16', v.getInt16(0, true)],
      ['i32', v.getInt32(0, true)],
      ['u32', v.getUint32(0, true)],
      ['i64', v.getBigInt64(0, true)],
      ['f32', f(v.getFloat32(0, true))],
      ['f64', f(v.getFloat64(0, true))],
      ['bytes', row.bytes.match(/../g)!.join(' ')],
    ] as [string, unknown][]
  })

  let name = $state('')
  $effect(() => {
    name = row?.name ?? ''
  })
  const ref = $derived(row ? { classId: row.classId, fieldId: row.fieldId } : null)
</script>

<div class="glass insp">
  {#if row && sel}
    <div class="lab first">
      {row.name ?? 'unnamed'} <span style:color="var(--k-{row.cat})">{row.typeLabel}</span>
    </div>
    <div class="big" class:err={!!row.error}>{row.value}</div>
    <div class="sub">{[row.error, ...row.hints].filter(Boolean).join(' · ')}</div>
    {#if row.port && row.port.state !== 'null'}
      <button class="action" onclick={() => follow(sel.card, row)}>
        {row.port.state === 'open' ? 'Unlink card' : row.port.state === 'known' ? 'Link to open card' : 'Follow pointer'} →
      </button>
    {/if}

    {#if editable}
      <div class="lab">Name</div>
      <input
        bind:value={name}
        placeholder={row.ty.startsWith('Hex') ? 'hex fields are unnamed' : 'name'}
        disabled={row.ty.startsWith('Hex')}
        onchange={() => name.trim() && call('renameField', { ...ref, name: name.trim() })}
      />
      <div class="lab">Type</div>
      <div class="typegrid">
        {#each COMMON as t (t)}
          <button class:on={t === row.ty} onclick={() => call('retype', { ...ref, ty: t })}>{SHORT_LABEL[t]}</button>
        {/each}
      </div>
      {#if row.ty === 'Pointer' || row.ty === 'EncryptedPointer'}
        <div class="lab">Points to</div>
        <select
          value={row.pointerTarget && 'ClassId' in row.pointerTarget ? String(row.pointerTarget.ClassId) : ''}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value
            call('setPointerTarget', { ...ref, target: v ? { ClassId: +v } : { FieldType: 'Hex64' } })
          }}
        >
          <option value="">void (unknown)</option>
          {#each app.defs.classes as c (c.id)}<option value={String(c.id)}>{c.name}</option>{/each}
        </select>
      {:else if row.ty === 'ClassInstance'}
        <div class="lab">Embedded class</div>
        <select value={String(row.embeddedClass ?? '')} onchange={(e) => call('setEmbeddedClass', { ...ref, target: +(e.currentTarget as HTMLSelectElement).value })}>
          <option value="" disabled>choose…</option>
          {#each app.defs.classes as c (c.id)}<option value={String(c.id)}>{c.name}</option>{/each}
        </select>
      {:else if row.ty === 'Enum'}
        <div class="lab">Enum</div>
        <select value={String(row.enumId ?? '')} onchange={(e) => call('setEnum', { ...ref, enumId: +(e.currentTarget as HTMLSelectElement).value })}>
          {#each app.defs.enums as en (en.id)}<option value={String(en.id)}>{en.name}</option>{/each}
        </select>
      {:else if row.ty === 'Array'}
        <div class="lab">Length</div>
        <input type="number" min="1" max="4096" value={row.arrayLength} onchange={(e) => call('setArray', { ...ref, length: +(e.currentTarget as HTMLInputElement).value })} />
        <div class="hint">Right-click the field to pick the element type.</div>
      {/if}
    {:else}
      <div class="hint">Array elements take their type from the array field.</div>
    {/if}

    <div class="lab">Location</div>
    <div class="grid2">
      <span>address</span><button class="link" onclick={() => copy(row.address)}>{row.address}</button>
      <span>offset</span><span>+0x{row.offset.toString(16).toUpperCase()}</span>
      <span>size</span><span>{row.size} B</span>
      {#if row.pointer}<span>pointer</span><button class="link" onclick={() => copy(row.pointer!)}>{row.pointer}</button>{/if}
    </div>
    <div class="lab">As</div>
    <div class="grid2">
      {#each interp as [k, v] (k)}<span>{k}</span><span>{String(v)}</span>{/each}
    </div>
  {:else}
    <div class="lab first">Inspector</div>
    <div class="empty">
      Select a field on any card.<br /><br />
      <kbd>F</kbd> fit view · <kbd>T</kbd> tidy layout<br />
      <kbd>F2</kbd> rename · <kbd>Del</kbd> remove field<br />
      {#if app.session?.canWrite}<kbd>Enter</kbd> edit the value in memory<br />{/if}
      <kbd>M</kbd> modules<br />
      Double-click a pointer to follow it<br />
      Right-click fields and cards for more
    </div>
  {/if}
</div>

<style>
  .insp { top: 76px; right: 14px; width: 280px; padding: 14px; max-height: calc(100vh - 250px); overflow: auto; }
  .big { font: 18px var(--mono); word-break: break-all; margin: 4px 0 2px; }
  .big.err { color: var(--bad); }
  .sub { color: var(--dim); font-size: 11px; word-break: break-all; }
  .lab { font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--faint); margin: 14px 0 6px; }
  .lab.first { margin-top: 0; }
  .grid2 { display: grid; grid-template-columns: auto 1fr; gap: 4px 12px; font: 11px var(--mono); }
  .grid2 > :nth-child(odd) { color: var(--dim); }
  .grid2 > :nth-child(even) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
  select, input { width: 100%; background: #00000040; border: 1px solid var(--edge); color: var(--text); border-radius: 8px; padding: 6px 8px; font: 12px var(--mono); outline: none; box-sizing: border-box; }
  input:disabled { opacity: 0.5; }
  select:focus, input:focus { border-color: var(--accent); }
  .typegrid { display: flex; flex-wrap: wrap; gap: 4px; }
  .typegrid button { border: 1px solid var(--edge2); background: transparent; border-radius: 6px; padding: 3px 7px; font: 10.5px var(--mono); color: var(--dim); cursor: pointer; }
  .typegrid button:hover { border-color: var(--accent); color: var(--text); }
  .typegrid button.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .action { margin-top: 10px; width: 100%; border: 1px solid var(--accent); background: rgba(139, 123, 255, 0.15); color: var(--text); border-radius: 8px; padding: 6px; cursor: pointer; }
  .action:hover { background: rgba(139, 123, 255, 0.3); }
  .link { background: none; border: none; padding: 0; color: var(--text); font: inherit; cursor: pointer; }
  .link:hover { color: var(--accent2); }
  .hint { color: var(--faint); font-size: 11px; margin-top: 6px; }
  .empty { color: var(--dim); line-height: 1.7; }
</style>
