// Small, unstyled UI helpers shared by the prototypes. Each prototype styles
// the generated `.cm*` classes itself so the menus match its design language.
(function () {
  'use strict';
  const M = window.Model;

  const esc = s => String(s == null ? '' : s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

  let openMenu = null;
  function closeMenu() { if (openMenu) { openMenu.remove(); openMenu = null; } }
  document.addEventListener('mousedown', e => { if (openMenu && !openMenu.contains(e.target)) closeMenu(); });
  document.addEventListener('keydown', e => { if (e.key === 'Escape') closeMenu(); });

  function buildMenu(items) {
    const el = document.createElement('div');
    el.className = 'cm';
    for (const it of items) {
      if (it.sep) { el.appendChild(Object.assign(document.createElement('div'), { className: 'cm-sep' })); continue; }
      if (it.header) { el.appendChild(Object.assign(document.createElement('div'), { className: 'cm-header', textContent: it.header })); continue; }
      const row = document.createElement('div');
      row.className = 'cm-item' + (it.danger ? ' cm-danger' : '') + (it.disabled ? ' cm-disabled' : '') + (it.checked ? ' cm-checked' : '') + (it.submenu ? ' cm-has-sub' : '');
      row.innerHTML = '<span class="cm-label">' + esc(it.label) + '</span>' + (it.hint ? '<span class="cm-hint">' + esc(it.hint) + '</span>' : '') + (it.submenu ? '<span class="cm-arrow">›</span>' : '');
      if (it.submenu) {
        const sub = buildMenu(it.submenu);
        sub.classList.add('cm-sub');
        row.appendChild(sub);
        row.addEventListener('mouseenter', () => {
          sub.style.display = 'block';
          const r = sub.getBoundingClientRect();
          if (r.right > innerWidth) { sub.style.left = 'auto'; sub.style.right = '100%'; }
          if (r.bottom > innerHeight) sub.style.top = Math.min(0, innerHeight - r.bottom - 8) + 'px';
        });
        row.addEventListener('mouseleave', () => { sub.style.display = 'none'; });
      } else if (!it.disabled) {
        row.addEventListener('click', e => { e.stopPropagation(); closeMenu(); it.onClick && it.onClick(); });
      }
      el.appendChild(row);
    }
    return el;
  }

  function contextMenu(x, y, items) {
    closeMenu();
    const el = buildMenu(items);
    el.style.position = 'fixed';
    el.style.left = x + 'px';
    el.style.top = y + 'px';
    el.style.zIndex = 9999;
    document.body.appendChild(el);
    const r = el.getBoundingClientRect();
    if (r.right > innerWidth) el.style.left = Math.max(4, innerWidth - r.width - 4) + 'px';
    if (r.bottom > innerHeight) el.style.top = Math.max(4, innerHeight - r.height - 4) + 'px';
    openMenu = el;
    return el;
  }

  // The standard field context menu; mirrors context_menu.rs in the egui app.
  function fieldMenuItems(classId, d, opts) {
    opts = opts || {};
    const f = d.field;
    const editable = classId != null && !f.isElement;
    const typeItems = M.TYPE_GROUPS.map(([g, ts]) => ({
      label: g,
      submenu: ts.map(t => ({ label: t, hint: M.TYPES[t].size ? M.TYPES[t].size + 'B' : '', checked: f.type === t, onClick: () => M.setFieldType(classId, f.id, t) })),
    }));
    const items = [];
    if (editable) {
      items.push({ header: (f.name || M.SHORT[f.type]) + ' @ +0x' + M.hex(f.offset) });
      items.push({ label: 'Change type', submenu: typeItems });
      if (f.type === 'Pointer' || f.type === 'EncryptedPointer' || f.type === 'ClassInstance') {
        items.push({
          label: 'Target class',
          submenu: M.classes.map(c => ({ label: c.name, checked: f.target && f.target.classId === c.id, onClick: () => M.patchField(classId, f.id, { target: { classId: c.id } }) }))
            .concat(f.type !== 'ClassInstance' ? [{ sep: true }].concat(['Float', 'Int32', 'UInt64', 'Vector3', 'Text'].map(t => ({ label: t, checked: f.target && f.target.type === t, onClick: () => M.patchField(classId, f.id, { target: { type: t } }) }))) : [])
            .concat([{ sep: true }, { label: 'New class…', onClick: () => { const c = M.addClass(); M.patchField(classId, f.id, { target: { classId: c.id } }); } }]),
        });
      }
      if (f.type === 'Enum') {
        items.push({ label: 'Enum', submenu: M.enums.map(e => ({ label: e.name, checked: f.enumId === e.id, onClick: () => M.patchField(classId, f.id, { enumId: e.id }) })) });
      }
      if (f.type === 'Array') {
        items.push({ label: 'Array length', submenu: [2, 4, 8, 16, 32].map(n => ({ label: String(n), checked: f.length === n, onClick: () => M.patchField(classId, f.id, { length: n }) })) });
      }
      items.push({ sep: true });
      if (opts.onRename) items.push({ label: 'Rename', hint: 'F2', onClick: opts.onRename });
      items.push({
        label: 'Insert bytes above', submenu: [4, 8, 64, 256, 1024].map(n => ({ label: n + ' bytes', hint: '0x' + M.hex(n), onClick: () => M.insertBytes(classId, f.id, n, false) })),
      });
      items.push({
        label: 'Insert bytes below', submenu: [4, 8, 64, 256, 1024].map(n => ({ label: n + ' bytes', hint: '0x' + M.hex(n), onClick: () => M.insertBytes(classId, f.id, n, true) })),
      });
    }
    items.push({ label: 'Copy address', hint: M.addrStr(d.addr), onClick: () => copy(M.addrStr(d.addr)) });
    items.push({ label: 'Copy value', onClick: () => copy(d.value) });
    if (d.pointer) items.push({ label: 'Copy pointer', hint: M.addrStr(d.pointer), onClick: () => copy(M.addrStr(d.pointer)) });
    if (editable) {
      items.push({ sep: true });
      items.push({ label: 'Remove field', hint: 'Del', danger: true, onClick: () => M.removeField(classId, f.id) });
    }
    return items;
  }

  function copy(text) { try { navigator.clipboard.writeText(text); } catch (e) { /* file:// may block */ } toast('Copied ' + text); }

  let toastEl = null, toastTimer = null;
  function toast(msg) {
    if (!toastEl) { toastEl = document.createElement('div'); toastEl.className = 'toast'; document.body.appendChild(toastEl); }
    toastEl.textContent = msg;
    toastEl.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toastEl.classList.remove('show'), 1600);
  }

  // Remembers last rendered value per key so views can flash changed values.
  const last = new Map();
  function changed(key, value) {
    const prev = last.get(key);
    last.set(key, value);
    return prev !== undefined && prev !== value;
  }

  // Inline rename helper: swaps an element for an <input>, commits on Enter/blur.
  function inlineEdit(el, value, commit, cls) {
    const input = document.createElement('input');
    input.value = value || '';
    input.className = cls || 'inline-edit';
    input.spellcheck = false;
    const w = el.getBoundingClientRect().width;
    input.style.width = Math.max(80, w + 24) + 'px';
    el.replaceWith(input);
    input.focus(); input.select();
    let done = false;
    const finish = ok => { if (done) return; done = true; if (ok) commit(input.value.trim()); else input.replaceWith(el); };
    input.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') finish(true); if (e.key === 'Escape') finish(false); });
    input.addEventListener('blur', () => finish(true));
    input.addEventListener('mousedown', e => e.stopPropagation());
    return input;
  }

  // Flattens the visible part of a class tree into rows. `expanded` is a Set of
  // row keys; `stop(d)` can veto recursion (the graph view spawns cards instead).
  function flatten(classId, base, expanded, stop) {
    const rows = [];
    const walk = (ownerCid, fields, b, prefix, depth) => {
      for (const f of fields) {
        const d = M.describe(f, b);
        const key = prefix + '/' + f.id;
        const open = d.expandable && expanded.has(key) && !(stop && stop(d));
        rows.push({ key, depth, d, classId: f.isElement ? null : ownerCid, open });
        if (open && depth < 24) walk(d.child.classId, d.child.fields, d.child.base, key, depth + 1);
      }
    };
    const c = M.getClass(classId);
    if (c && base != null) walk(classId, c.fields, base, '', 0);
    return rows;
  }
  // Structural signature: if unchanged between ticks a view can patch values in place.
  const sig = rows => rows.map(r => r.key + (r.open ? '+' + r.d.child.base : '') + r.d.typeLabel + r.d.name).join('|');

  window.UI = { flatten, sig, esc, contextMenu, closeMenu, fieldMenuItems, copy, toast, changed, inlineEdit };
})();
