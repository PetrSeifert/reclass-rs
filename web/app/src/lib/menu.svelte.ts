// Context menus: a single global menu plus builders for field and card menus.

import { focusCard } from './geometry.svelte'
import { app, call, copy, toast } from './store.svelte'
import { TYPE_GROUPS, typeSize, type CardView, type FieldType, type PointerTarget, type Row } from './types'

export interface MenuItem {
  label?: string
  hint?: string
  header?: string
  sep?: boolean
  danger?: boolean
  disabled?: boolean
  checked?: boolean
  submenu?: MenuItem[]
  onClick?: () => void
}

export const menu = $state({ open: false, x: 0, y: 0, items: [] as MenuItem[] })

export function openMenu(e: MouseEvent, items: MenuItem[]) {
  e.preventDefault()
  e.stopPropagation()
  menu.x = e.clientX
  menu.y = e.clientY
  menu.items = items
  menu.open = true
}

export function closeMenu() {
  menu.open = false
}

/** Row currently being renamed inline. */
export const renaming = $state({ card: 0, key: '' })

const hex = (n: number) => n.toString(16).toUpperCase()
const same = (a: PointerTarget | null, b: PointerTarget) => JSON.stringify(a) === JSON.stringify(b)

export async function follow(card: CardView, row: Row) {
  const r = await call<{ card: number | null; existing: boolean }>('follow', { card: card.id, key: row.key })
  if (r.card == null) return
  // The new card may arrive in a frame just after this reply.
  requestAnimationFrame(() => setTimeout(() => focusCard(r.card!), 30))
  if (r.existing) toast(`${row.name ?? 'Pointer'} leads to a card that is already open — linked to it`)
}

function targetItems(row: Row, set: (t: PointerTarget) => void, allowNewClass: boolean): MenuItem[] {
  const classes = app.defs.classes.map((c) => ({
    label: c.name,
    checked: same(row.pointerTarget, { ClassId: c.id }),
    onClick: () => set({ ClassId: c.id }),
  }))
  const prims: FieldType[] = ['Float', 'Double', 'Int32', 'UInt32', 'Int64', 'UInt64', 'Vector3', 'TextPointer']
  return [
    { header: 'Class' },
    ...classes,
    ...(allowNewClass
      ? [{ label: 'New class…', onClick: async () => set({ ClassId: await call<number>('addClass', {}) }) }]
      : []),
    { sep: true },
    { header: 'Value' },
    { label: 'void (unknown)', checked: same(row.pointerTarget, { FieldType: 'Hex64' }), onClick: () => set({ FieldType: 'Hex64' }) },
    ...prims.map((t) => ({ label: t, checked: same(row.pointerTarget, { FieldType: t }), onClick: () => set({ FieldType: t }) })),
  ]
}

function arrayElementItems(row: Row): MenuItem[] {
  const set = (element: PointerTarget) => call('setArray', { classId: row.classId, fieldId: row.fieldId, element })
  const prims: FieldType[] = ['Hex8', 'Hex32', 'Hex64', 'Int32', 'UInt32', 'Float', 'Vector3', 'TextPointer']
  return [
    { header: 'Pointer to class' },
    ...app.defs.classes.map((c) => ({ label: `${c.name}*`, checked: same(row.arrayElement, { Pointer: { ClassId: c.id } }), onClick: () => set({ Pointer: { ClassId: c.id } }) })),
    { sep: true },
    { header: 'Inline class' },
    ...app.defs.classes
      .filter((c) => c.id !== row.classId)
      .map((c) => ({ label: c.name, checked: same(row.arrayElement, { ClassId: c.id }), onClick: () => set({ ClassId: c.id }) })),
    { sep: true },
    { header: 'Value' },
    ...prims.map((t) => ({ label: t, checked: same(row.arrayElement, { FieldType: t }), onClick: () => set({ FieldType: t }) })),
  ]
}

export function fieldMenu(card: CardView, row: Row): MenuItem[] {
  const editable = row.classId != null
  const ref = { classId: row.classId, fieldId: row.fieldId }
  const items: MenuItem[] = [{ header: `${row.name ?? row.ty} @ +0x${hex(row.offset)}` }]
  if (row.port && row.port.state !== 'null') {
    const linked = row.port.state === 'open'
    items.push({ label: linked ? 'Unlink card' : 'Follow pointer', hint: 'port', onClick: () => follow(card, row) })
  }
  if (editable) {
    items.push({
      label: 'Change type',
      submenu: TYPE_GROUPS.map(([group, types]) => ({
        label: group,
        submenu: types.map((ty) => ({
          label: ty,
          hint: typeSize(ty, app.session?.pointerSize) ? `${typeSize(ty, app.session?.pointerSize)}B` : '',
          checked: row.ty === ty,
          onClick: () => call('retype', { ...ref, ty }),
        })),
      })),
    })
    if (row.ty === 'Pointer' || row.ty === 'EncryptedPointer') {
      items.push({ label: 'Points to', submenu: targetItems(row, (target) => call('setPointerTarget', { ...ref, target }), true) })
    }
    if (row.ty === 'ClassInstance') {
      items.push({
        label: 'Embedded class',
        submenu: app.defs.classes.map((c) => ({ label: c.name, checked: row.embeddedClass === c.id, onClick: () => call('setEmbeddedClass', { ...ref, target: c.id }) })),
      })
    }
    if (row.ty === 'Enum') {
      items.push({
        label: 'Enum',
        submenu: [
          ...app.defs.enums.map((e) => ({ label: e.name, hint: `${e.size}B`, checked: row.enumId === e.id, onClick: () => call('setEnum', { ...ref, enumId: e.id }) })),
          ...(app.defs.enums.length ? [{ sep: true }] : []),
          {
            label: 'New enum…',
            onClick: async () => {
              // Match the field's current size so the class layout doesn't shift.
              const size = [1, 2, 4, 8].includes(row.size) ? row.size : 4
              const enumId = await call<number>('addEnum', { size })
              await call('setEnum', { ...ref, enumId })
              app.editingEnum = enumId
            },
          },
          ...(row.enumId != null ? [{ label: 'Edit enum…', onClick: () => (app.editingEnum = row.enumId) }] : []),
        ],
      })
    }
    if (row.ty === 'Array') {
      items.push({ label: 'Element type', submenu: arrayElementItems(row) })
      items.push({
        label: 'Length',
        submenu: [1, 2, 4, 8, 16, 32, 64, 128].map((n) => ({ label: String(n), checked: row.arrayLength === n, onClick: () => call('setArray', { ...ref, length: n }) })),
      })
    }
    items.push({ sep: true })
    if (row.ty.startsWith('Hex')) items.push({ label: 'Rename', hint: 'hex fields are unnamed', disabled: true })
    else items.push({ label: 'Rename', hint: 'F2', onClick: () => Object.assign(renaming, { card: card.id, key: row.key }) })
    const sizes = [4, 8, 64, 256, 1024]
    items.push({ label: 'Insert bytes above', submenu: sizes.map((n) => ({ label: `${n} bytes`, hint: `0x${hex(n)}`, onClick: () => call('insertBytes', { ...ref, count: n, after: false }) })) })
    items.push({ label: 'Insert bytes below', submenu: sizes.map((n) => ({ label: `${n} bytes`, hint: `0x${hex(n)}`, onClick: () => call('insertBytes', { ...ref, count: n, after: true }) })) })
  }
  items.push({ sep: true })
  items.push({ label: 'Copy address', hint: row.address, onClick: () => copy(row.address) })
  items.push({ label: 'Copy value', onClick: () => copy(row.value) })
  if (row.pointer) items.push({ label: 'Copy pointer', hint: row.pointer, onClick: () => copy(row.pointer!) })
  if (editable) {
    items.push({ sep: true })
    items.push({ label: 'Remove field', hint: 'Del', danger: true, onClick: () => call('removeField', ref) })
  }
  return items
}

export function cardMenu(card: CardView, onRename: () => void): MenuItem[] {
  return [
    { header: `${card.className} · 0x${hex(card.size)} bytes` },
    { label: 'Rename class', onClick: onRename },
    { label: 'Add 8 bytes', onClick: () => call('insertBytes', { classId: card.classId, fieldId: null, count: 8 }) },
    { label: 'Add 64 bytes', onClick: () => call('insertBytes', { classId: card.classId, fieldId: null, count: 64 }) },
    { sep: true },
    ...(card.base ? [{ label: 'Copy base address', hint: card.base, onClick: () => copy(card.base!) }] : []),
    ...(!card.isRoot && card.base
      ? [{ label: 'Make this the root', onClick: () => call('setRoot', { classId: card.classId, expr: `0x${card.base}` }) }]
      : []),
    ...(card.dupOf ? [{ label: 'Merge with duplicate card', onClick: () => call('mergeCard', { card: card.id }) }] : []),
    ...(card.isRoot ? [] : [{ sep: true }, { label: 'Close card', danger: true, onClick: () => call('closeCard', { card: card.id }) }]),
  ]
}
