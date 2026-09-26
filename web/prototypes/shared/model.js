// Shared mock backend for all reclass-rs web prototypes.
// Simulates an attached process with byte-level memory, class/enum registries,
// signatures and the root-address expression language, so every design is
// fully clickable without the Rust side.
(function () {
  'use strict';

  // ---------------------------------------------------------------- types
  const TYPES = {
    Hex64: { size: 8, cat: 'hex' }, Hex32: { size: 4, cat: 'hex' },
    Hex16: { size: 2, cat: 'hex' }, Hex8: { size: 1, cat: 'hex' },
    Int64: { size: 8, cat: 'int' }, Int32: { size: 4, cat: 'int' },
    Int16: { size: 2, cat: 'int' }, Int8: { size: 1, cat: 'int' },
    UInt64: { size: 8, cat: 'uint' }, UInt32: { size: 4, cat: 'uint' },
    UInt16: { size: 2, cat: 'uint' }, UInt8: { size: 1, cat: 'uint' },
    Bool: { size: 1, cat: 'bool' },
    Float: { size: 4, cat: 'float' }, Double: { size: 8, cat: 'float' },
    Vector2: { size: 8, cat: 'vec' }, Vector3: { size: 12, cat: 'vec' }, Vector4: { size: 16, cat: 'vec' },
    Text: { size: 32, cat: 'text' }, TextPointer: { size: 8, cat: 'text' },
    Pointer: { size: 8, cat: 'ptr' }, EncryptedPointer: { size: 8, cat: 'ptr' },
    Enum: { size: 4, cat: 'enum' },
    Array: { size: 0, cat: 'array' }, ClassInstance: { size: 0, cat: 'class' },
  };
  const TYPE_GROUPS = [
    ['Hex', ['Hex64', 'Hex32', 'Hex16', 'Hex8']],
    ['Signed', ['Int64', 'Int32', 'Int16', 'Int8']],
    ['Unsigned', ['UInt64', 'UInt32', 'UInt16', 'UInt8']],
    ['Float', ['Float', 'Double', 'Bool']],
    ['Vector', ['Vector2', 'Vector3', 'Vector4']],
    ['Text', ['Text', 'TextPointer']],
    ['Reference', ['Pointer', 'EncryptedPointer', 'ClassInstance', 'Array', 'Enum']],
  ];
  const SHORT = {
    Hex64: 'hex64', Hex32: 'hex32', Hex16: 'hex16', Hex8: 'hex8',
    Int64: 'i64', Int32: 'i32', Int16: 'i16', Int8: 'i8',
    UInt64: 'u64', UInt32: 'u32', UInt16: 'u16', UInt8: 'u8',
    Bool: 'bool', Float: 'f32', Double: 'f64',
    Vector2: 'vec2', Vector3: 'vec3', Vector4: 'vec4',
    Text: 'char[32]', TextPointer: 'char*', Pointer: 'ptr', EncryptedPointer: 'eptr',
    Enum: 'enum', Array: 'array', ClassInstance: 'class',
  };

  // ---------------------------------------------------------------- memory
  const MODULES = [
    { name: 'Hollowfield.exe', base: 0x7FF6A2600000, size: 0x1200000, path: 'C:\\Games\\Hollowfield\\Hollowfield.exe' },
    { name: 'GameCore.dll', base: 0x7FFB6A100000, size: 0x2C4000, path: 'C:\\Games\\Hollowfield\\GameCore.dll' },
    { name: 'ntdll.dll', base: 0x7FFB8E6A0000, size: 0x1F8000, path: 'C:\\Windows\\System32\\ntdll.dll' },
    { name: 'KERNEL32.DLL', base: 0x7FFB8D1C0000, size: 0xC2000, path: 'C:\\Windows\\System32\\kernel32.dll' },
    { name: 'KERNELBASE.dll', base: 0x7FFB8BE10000, size: 0x3A6000, path: 'C:\\Windows\\System32\\KernelBase.dll' },
    { name: 'd3d11.dll', base: 0x7FFB86A30000, size: 0x264000, path: 'C:\\Windows\\System32\\d3d11.dll' },
    { name: 'dxgi.dll', base: 0x7FFB88210000, size: 0x13B000, path: 'C:\\Windows\\System32\\dxgi.dll' },
    { name: 'user32.dll', base: 0x7FFB8D3A0000, size: 0x1AD000, path: 'C:\\Windows\\System32\\user32.dll' },
    { name: 'vcruntime140.dll', base: 0x7FFB7E8C0000, size: 0x1D000, path: 'C:\\Windows\\System32\\vcruntime140.dll' },
  ];
  const HEAP_LO = 0x1F300000000, HEAP_HI = 0x1F400000000;
  const EXE = MODULES[0];

  const pages = new Map();
  function readable(a) {
    if (a >= HEAP_LO && a < HEAP_HI) return true;
    return MODULES.some(m => a >= m.base && a < m.base + m.size);
  }
  function page(pn) {
    let p = pages.get(pn);
    if (p) return p;
    p = new Uint8Array(4096);
    // deterministic noise: heap is mostly zeroed, images are dense
    let s = (pn * 2654435761) >>> 0 || 1;
    const heap = pn * 4096 >= HEAP_LO && pn * 4096 < HEAP_HI;
    for (let i = 0; i < 4096; i++) {
      s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0;
      if (heap) p[i] = (s & 0xff) < 200 ? 0 : (s >>> 8) & 0xff;
      else p[i] = (s & 3) === 0 ? 0 : (s >>> 8) & 0xff;
    }
    pages.set(pn, p);
    return p;
  }
  function read(addr, n) {
    if (!readable(addr) || !readable(addr + n - 1)) return null;
    const out = new Uint8Array(n);
    for (let i = 0; i < n; i++) {
      const a = addr + i;
      out[i] = page(Math.floor(a / 4096))[a % 4096];
    }
    return out;
  }
  function write(addr, bytes) {
    for (let i = 0; i < bytes.length; i++) {
      const a = addr + i;
      page(Math.floor(a / 4096))[a % 4096] = bytes[i];
    }
  }
  const dv = n => { const b = new Uint8Array(n); return [b, new DataView(b.buffer)]; };
  const w8 = (a, v) => write(a, [v & 0xff]);
  const w16 = (a, v) => { const [b, d] = dv(2); d.setUint16(0, v, true); write(a, b); };
  const w32 = (a, v) => { const [b, d] = dv(4); d.setUint32(0, v >>> 0, true); write(a, b); };
  const wi32 = (a, v) => { const [b, d] = dv(4); d.setInt32(0, v, true); write(a, b); };
  const wf = (a, v) => { const [b, d] = dv(4); d.setFloat32(0, v, true); write(a, b); };
  const w64 = (a, v) => { const [b, d] = dv(8); d.setBigUint64(0, BigInt(v), true); write(a, b); };
  const wstr = (a, s, pad) => { const b = new Uint8Array(pad || s.length + 1); for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i); write(a, b); };
  const wvec = (a, arr) => arr.forEach((v, i) => wf(a + i * 4, v));

  function view(addr, n) { const b = read(addr, n); return b ? new DataView(b.buffer) : null; }
  const rU64 = a => { const d = view(a, 8); return d ? d.getBigUint64(0, true) : null; };
  const rPtr = a => { const v = rU64(a); return v === null ? null : Number(v); };

  // ---------------------------------------------------------------- seeded world
  const ENC_KEY = 0x3A5F00C0FFEE1234n;
  const A = {
    gworldGlobal: EXE.base + 0x5A1230,
    world: 0x1F3A8C40000,
    entityList: 0x1F3A8E00000,
    camera: 0x1F3A8F08800,
    player: i => 0x1F3A8D12400 + i * 0x200,
    weapon: i => 0x1F3A8D40000 + i * 0x80,
    str: 0x1F3A8D60000,
  };
  const PLAYERS = [
    { name: 'peterrock', team: 1, weapon: 'AR-7 "Kestrel"', wtype: 1, ammo: 27, max: 30 },
    { name: 'xX_n0sc0pe_Xx', team: 2, weapon: 'Longshot M2', wtype: 3, ammo: 4, max: 5 },
    { name: 'Ghost', team: 1, weapon: 'Breacher 12g', wtype: 2, ammo: 6, max: 8 },
    { name: 'Mira', team: 2, weapon: 'AR-7 "Kestrel"', wtype: 1, ammo: 30, max: 30 },
    { name: 'b0t_Harold', team: 3, weapon: 'P9 Sidearm', wtype: 0, ammo: 12, max: 15 },
    { name: 'Sable', team: 2, weapon: 'Vektor SMG', wtype: 4, ammo: 19, max: 32 },
  ];
  let strCursor = A.str;
  const allocStr = s => { const a = strCursor; wstr(a, s); strCursor += (s.length + 16) & ~15; return a; };

  function seed() {
    w64(A.gworldGlobal, A.world);
    // GameWorld
    w64(A.world + 0x00, EXE.base + 0x58E4F0);
    w64(A.world + 0x08, A.player(0));
    w64(A.world + 0x10, A.entityList);
    wi32(A.world + 0x18, PLAYERS.length);
    w32(A.world + 0x1C, 184220);
    wf(A.world + 0x20, 1.0);
    w8(A.world + 0x24, 0); w8(A.world + 0x25, 0); w16(A.world + 0x26, 0);
    wstr(A.world + 0x28, 'de_hollow_ridge', 32);
    w64(A.world + 0x48, BigInt(A.camera) ^ ENC_KEY);
    wf(A.world + 0x50, -9.81);
    w32(A.world + 0x54, 0);
    w64(A.world + 0x58, EXE.base + 0x7C10A0);
    // EntityList
    wi32(A.entityList, PLAYERS.length); wi32(A.entityList + 4, 8);
    for (let i = 0; i < 8; i++) w64(A.entityList + 8 + i * 8, i < PLAYERS.length ? A.player(i) : 0);
    // Players + weapons
    PLAYERS.forEach((p, i) => {
      const a = A.player(i), wa = A.weapon(i);
      w64(a, EXE.base + 0x591A08);
      w64(a + 0x08, allocStr(p.name));
      wvec(a + 0x10, [120 + i * 40, 64 - i * 12, 12.5]);
      w32(a + 0x1C, 0);
      wvec(a + 0x20, [0, 0, 0]);
      wf(a + 0x2C, 100 - i * 13); wf(a + 0x30, 100); wf(a + 0x34, [50, 100, 0, 75, 25, 100][i]);
      w32(a + 0x38, p.team); w32(a + 0x3C, 1 | (i % 3 === 0 ? 4 : 0));
      w64(a + 0x40, wa); w64(a + 0x48, A.world);
      wi32(a + 0x50, [14, 22, 9, 17, 0, 11][i]); wi32(a + 0x54, [6, 9, 12, 8, 21, 7][i]);
      w16(a + 0x58, [23, 41, 18, 67, 0, 35][i]); w16(a + 0x5A, 0); w32(a + 0x5C, 0x1000 + i);
      w64(a + 0x60, 0); w64(a + 0x68, 0);
      w64(wa, EXE.base + 0x59C2E8);
      w64(wa + 0x08, allocStr(p.weapon));
      wi32(wa + 0x10, p.ammo); wi32(wa + 0x14, p.max);
      wf(wa + 0x18, [0.1, 1.4, 0.9, 0.1, 0.25, 0.07][i]); w32(wa + 0x1C, p.wtype);
      wf(wa + 0x20, [24, 95, 12, 24, 18, 16][i]); wi32(wa + 0x24, 90); w64(wa + 0x28, 0);
    });
    // Camera
    w64(A.camera, EXE.base + 0x5A0010);
    wvec(A.camera + 0x08, [120, 64, 76.5]);
    wvec(A.camera + 0x14, [-4.5, 87.2, 0]);
    wf(A.camera + 0x20, 90); wf(A.camera + 0x24, 0.1); wf(A.camera + 0x28, 5000); w32(A.camera + 0x2C, 0);
    [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [-120, -64, -76.5, 1]].forEach((r, i) => wvec(A.camera + 0x30 + i * 16, r));
  }
  seed();

  // ---------------------------------------------------------------- registries
  let nextId = 100;
  const F = (type, name, extra) => Object.assign({ id: nextId++, type, name: name || null, offset: 0 }, extra || {});
  const classes = [
    { id: 1, name: 'GameWorld', fields: [
      F('Hex64'), F('Pointer', 'localPlayer', { target: { classId: 2 } }),
      F('Pointer', 'entityList', { target: { classId: 4 } }),
      F('Int32', 'entityCount'), F('UInt32', 'tickCount'), F('Float', 'timeScale'),
      F('Bool', 'isPaused'), F('Hex8'), F('Hex16'), F('Text', 'mapName'),
      F('EncryptedPointer', 'camera', { target: { classId: 5 } }),
      F('Hex32'), F('Hex32'), F('Hex64'),
    ] },
    { id: 2, name: 'Player', fields: [
      F('Hex64'), F('TextPointer', 'name'), F('Vector3', 'position'), F('Hex32'),
      F('Vector3', 'velocity'), F('Float', 'health'), F('Float', 'maxHealth'), F('Hex32'),
      F('Enum', 'team', { enumId: 1 }), F('Enum', 'flags', { enumId: 3 }),
      F('Pointer', 'weapon', { target: { classId: 3 } }),
      F('Hex64'), F('Hex64'), F('Hex64'), F('Hex64'), F('Hex64'),
    ] },
    { id: 3, name: 'Weapon', fields: [
      F('Hex64'), F('TextPointer', 'displayName'), F('Int32', 'ammo'), F('Int32', 'maxAmmo'),
      F('Float', 'fireRate'), F('Enum', 'weaponType', { enumId: 2 }), F('Hex64'), F('Hex64'),
    ] },
    { id: 4, name: 'EntityList', fields: [
      F('Int32', 'count'), F('Int32', 'capacity'),
      F('Array', 'entities', { elem: { type: 'Pointer', target: { classId: 2 } }, length: 8 }),
    ] },
    { id: 5, name: 'Camera', fields: [
      F('Hex64'), F('Vector3', 'origin'), F('Vector3', 'angles'), F('Float', 'fov'),
      F('Hex32'), F('Hex32'), F('Hex32'),
      F('Vector4', 'viewRow0'), F('Vector4', 'viewRow1'), F('Vector4', 'viewRow2'), F('Vector4', 'viewRow3'),
    ] },
    { id: 6, name: 'UnknownClass_6', fields: [F('Hex64'), F('Hex64'), F('Hex64'), F('Hex64')] },
  ];
  const enums = [
    { id: 1, name: 'ETeam', isFlags: false, size: 4, variants: [['None', 0], ['Red', 1], ['Blue', 2], ['Spectator', 3]] },
    { id: 2, name: 'EWeaponType', isFlags: false, size: 4, variants: [['Pistol', 0], ['Rifle', 1], ['Shotgun', 2], ['Sniper', 3], ['SMG', 4]] },
    { id: 3, name: 'EPlayerFlags', isFlags: true, size: 4, variants: [['OnGround', 1], ['Crouching', 2], ['Sprinting', 4], ['Reloading', 8]] },
  ];
  const signatures = [
    { name: 'GWorld', module: 'Hollowfield.exe', pattern: '48 8B 05 ?? ?? ?? ?? 48 85 C0 74 ?? 48 8B 40 18', offset: 3, instLen: 7, value: A.gworldGlobal, error: null },
    { name: 'LocalPlayerOff', module: 'Hollowfield.exe', pattern: '48 8B 8B ?? ?? ?? ?? 48 85 C9 0F 84', offset: 3, instLen: 7, value: 0x08, error: null },
    { name: 'XenuineDecrypt', module: 'GameCore.dll', pattern: '48 8B 05 ?? ?? ?? ?? FF D0 48 8B C8', offset: 3, instLen: 7, value: 0x7FFB6A1F37D6, error: null },
    { name: 'ViewMatrix', module: 'Hollowfield.exe', pattern: 'F3 0F 10 0D ?? ?? ?? ?? 0F 28 C1 E8', offset: 4, instLen: 8, value: null, error: 'pattern not found' },
  ];
  const processes = [
    { pid: 4, name: 'System' }, { pid: 812, name: 'csrss.exe' }, { pid: 1204, name: 'explorer.exe' },
    { pid: 3380, name: 'Discord.exe' }, { pid: 4412, name: 'Code.exe' }, { pid: 5120, name: 'steam.exe' },
    { pid: 6716, name: 'Hollowfield.exe' }, { pid: 7024, name: 'chrome.exe' }, { pid: 7168, name: 'chrome.exe' },
    { pid: 8800, name: 'WindowsTerminal.exe' }, { pid: 9124, name: 'obs64.exe' }, { pid: 10332, name: 'Spotify.exe' },
  ];

  // ---------------------------------------------------------------- formatting
  function hex(n, pad) {
    if (n === null || n === undefined) return '??';
    const s = (typeof n === 'bigint' ? n : BigInt(Math.floor(n))).toString(16).toUpperCase();
    return pad ? s.padStart(pad, '0') : s;
  }
  const addrStr = n => hex(n, 12);
  const offStr = n => hex(n, 4);
  function fmtFloat(v) {
    if (!isFinite(v)) return String(v);
    const a = Math.abs(v);
    if (a !== 0 && (a >= 1e7 || a < 1e-4)) return v.toExponential(3);
    const s = v.toFixed(3).replace(/\.?0+$/, '');
    return s.includes('.') ? s : s + '.0';
  }
  function symbolize(a) {
    const m = MODULES.find(m => a >= m.base && a < m.base + m.size);
    if (m) return m.name + '+0x' + hex(a - m.base);
    if (a >= HEAP_LO && a < HEAP_HI) return 'heap';
    return null;
  }
  function cstr(addr, max) {
    const b = read(addr, max || 64);
    if (!b) return null;
    let s = '';
    for (const c of b) { if (!c) break; s += c >= 32 && c < 127 ? String.fromCharCode(c) : '.'; }
    return s;
  }
  const ascii = b => Array.from(b, c => (c >= 32 && c < 127 ? String.fromCharCode(c) : '.')).join('');

  // ---------------------------------------------------------------- layout
  const getClass = id => classes.find(c => c.id === id);
  const getEnum = id => enums.find(e => e.id === id);
  function fieldSize(f, seen) {
    if (f.type === 'Enum') return (getEnum(f.enumId) || {}).size || 4;
    if (f.type === 'ClassInstance') {
      const c = getClass(f.target && f.target.classId);
      return c && !(seen || []).includes(c.id) ? classSize(c.id, (seen || []).concat(c.id)) : 0;
    }
    if (f.type === 'Array') return fieldSize(f.elem, seen) * (f.length || 0);
    return TYPES[f.type].size;
  }
  function classSize(id, seen) {
    const c = getClass(id);
    return c ? c.fields.reduce((s, f) => s + fieldSize(f, seen || [id]), 0) : 0;
  }
  function recalc(c) { let o = 0; for (const f of c.fields) { f.offset = o; o += fieldSize(f); } }
  classes.forEach(recalc);

  function typeLabel(f) {
    if (f.type === 'Pointer' || f.type === 'EncryptedPointer') {
      const t = f.target || {};
      const inner = t.classId ? (getClass(t.classId) || {}).name : t.type || 'void';
      return (f.type === 'EncryptedPointer' ? 'enc ' : '') + inner + '*';
    }
    if (f.type === 'ClassInstance') return (getClass((f.target || {}).classId) || {}).name || 'class?';
    if (f.type === 'Enum') return (getEnum(f.enumId) || {}).name || 'enum?';
    if (f.type === 'Array') return typeLabel(f.elem) + '[' + f.length + ']';
    return f.type;
  }

  // ---------------------------------------------------------------- field evaluation
  // describe(field, base) -> everything a view needs to render one row.
  function describe(f, base) {
    const addr = base + f.offset;
    const size = fieldSize(f);
    const d = {
      field: f, addr, offset: f.offset, size, type: f.type, cat: TYPES[f.type].cat,
      name: f.name, typeLabel: typeLabel(f), value: '', hints: [], error: null,
      bytes: null, expandable: false, child: null,
    };
    const bytes = size > 0 ? read(addr, Math.min(size, 64)) : null;
    d.bytes = bytes;
    if (size > 0 && !bytes) { d.error = 'unreadable'; d.value = '<unreadable>'; return d; }
    const v = bytes ? new DataView(bytes.buffer) : null;
    switch (f.type) {
      case 'Hex64': case 'Hex32': case 'Hex16': case 'Hex8': {
        const n = size === 8 ? v.getBigUint64(0, true) : BigInt(size === 4 ? v.getUint32(0, true) : size === 2 ? v.getUint16(0, true) : v.getUint8(0));
        d.value = hex(n, size * 2);
        if (size === 8) {
          const sym = symbolize(Number(n));
          if (sym && n !== 0n) d.hints.push('-> ' + sym);
          else if (n !== 0n && n < 0x100000000n) d.hints.push('int ' + n);
          else if (n !== 0n && (n >> 32n) < 100000n && (n & 0xFFFFFFFFn) < 100000n) d.hints.push('i32 ' + (n & 0xFFFFFFFFn) + ', ' + (n >> 32n));
        } else if (size === 4) {
          const fl = v.getFloat32(0, true);
          if (n !== 0n && Math.abs(fl) > 1e-4 && Math.abs(fl) < 1e7) d.hints.push('f32 ' + fmtFloat(fl));
          else if (n !== 0n) d.hints.push('int ' + v.getInt32(0, true));
        } else if (n !== 0n) d.hints.push('int ' + n);
        const s = ascii(bytes);
        if (/[A-Za-z]{3,}/.test(s)) d.hints.push("'" + s + "'");
        break;
      }
      case 'Int64': d.value = v.getBigInt64(0, true).toString(); break;
      case 'Int32': d.value = String(v.getInt32(0, true)); break;
      case 'Int16': d.value = String(v.getInt16(0, true)); break;
      case 'Int8': d.value = String(v.getInt8(0)); break;
      case 'UInt64': d.value = v.getBigUint64(0, true).toString(); break;
      case 'UInt32': d.value = String(v.getUint32(0, true)); break;
      case 'UInt16': d.value = String(v.getUint16(0, true)); break;
      case 'UInt8': d.value = String(v.getUint8(0)); break;
      case 'Bool': d.value = v.getUint8(0) ? 'true' : 'false'; break;
      case 'Float': d.value = fmtFloat(v.getFloat32(0, true)); break;
      case 'Double': d.value = fmtFloat(v.getFloat64(0, true)); break;
      case 'Vector2': case 'Vector3': case 'Vector4': {
        const n = size / 4, xs = [];
        for (let i = 0; i < n; i++) xs.push(fmtFloat(v.getFloat32(i * 4, true)));
        d.value = '(' + xs.join(', ') + ')';
        d.components = xs;
        break;
      }
      case 'Text': d.value = '"' + cstr(addr, 32) + '"'; break;
      case 'TextPointer': {
        const p = Number(v.getBigUint64(0, true));
        const s = p ? cstr(p, 64) : null;
        d.pointer = p;
        d.value = s === null ? (p ? '<bad ptr>' : 'nullptr') : '"' + s + '"';
        d.hints.push('@ ' + addrStr(p));
        break;
      }
      case 'Pointer': case 'EncryptedPointer': {
        const raw = v.getBigUint64(0, true);
        let p = Number(raw);
        if (f.type === 'EncryptedPointer') {
          p = Number(raw ^ ENC_KEY);
          d.encrypted = raw;
          d.hints.push('enc ' + hex(raw, 16));
        }
        d.pointer = p;
        const t = f.target || {};
        if (!p) { d.value = 'nullptr'; break; }
        if (!readable(p)) { d.value = addrStr(p); d.error = 'invalid pointer'; break; }
        d.value = addrStr(p);
        const sym = symbolize(p);
        if (sym && sym !== 'heap') d.hints.push(sym);
        if (t.classId && getClass(t.classId)) {
          d.expandable = true;
          d.child = { classId: t.classId, base: p, fields: getClass(t.classId).fields };
        } else if (t.type) {
          const inner = describe({ id: -1, type: t.type, offset: 0, name: '*' }, p);
          d.hints.unshift('-> ' + inner.value);
        }
        break;
      }
      case 'Enum': {
        const e = getEnum(f.enumId);
        const n = size === 1 ? v.getUint8(0) : size === 2 ? v.getUint16(0, true) : v.getUint32(0, true);
        d.raw = n;
        if (!e) { d.value = String(n); break; }
        if (e.isFlags) {
          const on = e.variants.filter(([, x]) => n & x).map(([k]) => k);
          d.value = on.length ? on.join(' | ') : '0';
        } else {
          const hit = e.variants.find(([, x]) => x === n);
          d.value = hit ? hit[0] : String(n);
        }
        d.hints.push('= ' + n);
        break;
      }
      case 'ClassInstance': {
        const c = getClass((f.target || {}).classId);
        d.value = c ? '{ ' + c.name + ' }' : '{ ? }';
        if (c) { d.expandable = true; d.child = { classId: c.id, base: addr, fields: c.fields }; }
        break;
      }
      case 'Array': {
        const es = fieldSize(f.elem);
        d.value = '[' + f.length + ' × ' + typeLabel(f.elem) + ']';
        d.expandable = true;
        d.child = {
          classId: null, base: addr,
          fields: Array.from({ length: f.length }, (_, i) => Object.assign({}, f.elem, { id: f.id * 1000 + i, name: '[' + i + ']', offset: i * es, isElement: true })),
        };
        break;
      }
    }
    return d;
  }

  // ---------------------------------------------------------------- expressions
  function evalExpr(src) {
    let i = 0;
    const s = src;
    const ws = () => { while (i < s.length && /\s/.test(s[i])) i++; };
    const fail = m => { throw new Error(m + ' at ' + i); };
    function factor() {
      ws();
      const c = s[i];
      if (c === '(') { i++; const v = expr(); ws(); if (s[i++] !== ')') fail("expected ')'"); return v; }
      if (c === '[') {
        i++; const v = expr(); ws(); if (s[i++] !== ']') fail("expected ']'");
        const p = rPtr(v); if (p === null) fail('cannot read 0x' + hex(v)); return p;
      }
      if (c === '<') {
        const j = s.indexOf('>', i); if (j < 0) fail("expected '>'");
        const name = s.slice(i + 1, j).trim(); i = j + 1;
        const m = MODULES.find(m => m.name.toLowerCase() === name.toLowerCase());
        if (!m) fail('unknown module ' + name); return m.base;
      }
      if (c === '$') {
        i++; const m = /^[A-Za-z_][A-Za-z0-9_]*/.exec(s.slice(i)); if (!m) fail('expected signature name');
        i += m[0].length;
        const sig = signatures.find(x => x.name === m[0]);
        if (!sig) fail('unknown signature $' + m[0]);
        if (sig.value === null) fail('$' + m[0] + ': ' + sig.error);
        return sig.value;
      }
      const m = /^(0x[0-9a-fA-F]+|[0-9]+)/.exec(s.slice(i));
      if (!m) fail('unexpected ' + (c === undefined ? 'end' : "'" + c + "'"));
      i += m[0].length;
      return Number(m[0]);
    }
    function expr() {
      let v = factor();
      for (;;) {
        ws();
        if (s[i] === '+') { i++; v += factor(); }
        else if (s[i] === '-') { i++; v -= factor(); }
        else return v;
      }
    }
    try {
      const v = expr(); ws();
      if (i < s.length) fail("unexpected '" + s[i] + "'");
      return { ok: true, value: v };
    } catch (e) { return { ok: false, error: e.message }; }
  }

  // ---------------------------------------------------------------- mutations
  const listeners = [];
  const emit = what => listeners.forEach(fn => fn(what));
  const findField = (cid, fid) => { const c = getClass(cid); return c ? [c, c.fields.findIndex(f => f.id === fid)] : [null, -1]; };
  function hexFill(n) {
    const out = [];
    for (const [t, s] of [['Hex64', 8], ['Hex32', 4], ['Hex16', 2], ['Hex8', 1]]) while (n >= s) { out.push(F(t)); n -= s; }
    return out;
  }
  function setFieldType(cid, fid, type, extra) {
    const [c, idx] = findField(cid, fid);
    if (idx < 0) return;
    const f = c.fields[idx];
    const oldSize = fieldSize(f);
    const nf = Object.assign({ id: f.id, type, offset: f.offset, name: TYPES[type].cat === 'hex' ? null : f.name || defaultName(type, f.offset) }, extra || {});
    if (type === 'Pointer' || type === 'EncryptedPointer') nf.target = nf.target || f.target || { classId: null };
    if (type === 'ClassInstance') nf.target = nf.target || { classId: classes[0].id };
    if (type === 'Enum') nf.enumId = nf.enumId || f.enumId || enums[0].id;
    if (type === 'Array') { nf.elem = nf.elem || { type: 'Hex32' }; nf.length = nf.length || 4; }
    const newSize = fieldSize(nf);
    const repl = [nf];
    let removeCount = 1;
    if (newSize < oldSize) repl.push(...hexFill(oldSize - newSize));
    else if (newSize > oldSize) {
      let need = newSize - oldSize;
      while (need > 0 && idx + removeCount < c.fields.length) {
        const s = fieldSize(c.fields[idx + removeCount]);
        removeCount++;
        if (s > need) { repl.push(...hexFill(s - need)); need = 0; } else need -= s;
      }
    }
    c.fields.splice(idx, removeCount, ...repl);
    recalc(c); classes.forEach(recalc); emit('layout');
  }
  function defaultName(type, off) { return (SHORT[type] || 'field').replace(/\W/g, '') + '_' + hex(off, 2); }
  function insertBytes(cid, fid, n, after) {
    const [c, idx] = findField(cid, fid);
    if (!c) return;
    c.fields.splice(idx < 0 ? c.fields.length : idx + (after ? 1 : 0), 0, ...hexFill(n));
    recalc(c); classes.forEach(recalc); emit('layout');
  }
  function removeField(cid, fid) {
    const [c, idx] = findField(cid, fid);
    if (idx < 0) return;
    c.fields.splice(idx, 1); recalc(c); classes.forEach(recalc); emit('layout');
  }
  function renameField(cid, fid, name) {
    const [c, idx] = findField(cid, fid);
    if (idx < 0) return;
    c.fields[idx].name = name || null; emit('names');
  }
  function patchField(cid, fid, patch) {
    const [c, idx] = findField(cid, fid);
    if (idx < 0) return;
    Object.assign(c.fields[idx], patch); recalc(c); classes.forEach(recalc); emit('layout');
  }
  // Retype whatever lives at `offset`, splitting a covering hex field if needed.
  function defineAt(cid, offset, type) {
    const c = getClass(cid);
    if (!c) return;
    const idx = c.fields.findIndex(f => offset >= f.offset && offset < f.offset + fieldSize(f));
    if (idx < 0) return;
    const f = c.fields[idx];
    if (f.offset !== offset) {
      if (TYPES[f.type].cat !== 'hex') return;
      const size = fieldSize(f);
      c.fields.splice(idx, 1, ...hexFill(offset - f.offset), ...hexFill(f.offset + size - offset));
      recalc(c);
    }
    const target = c.fields.find(x => x.offset === offset);
    if (target) setFieldType(cid, target.id, type);
    return target;
  }
  function renameClass(cid, name) { const c = getClass(cid); if (c && name) { c.name = name; emit('names'); } }
  function addClass(name) {
    const id = Math.max(...classes.map(c => c.id)) + 1;
    const c = { id, name: name || 'NewClass_' + id, fields: hexFill(0x40) };
    recalc(c); classes.push(c); emit('classes'); return c;
  }
  function removeClass(cid) {
    const i = classes.findIndex(c => c.id === cid);
    if (i >= 0 && cid !== state.rootClassId) { classes.splice(i, 1); emit('classes'); }
  }
  function classRefs(cid) {
    let n = 0;
    const hit = f => f && f.target && f.target.classId === cid;
    classes.forEach(c => c.fields.forEach(f => { if (hit(f) || hit(f.elem)) n++; }));
    return n + (state.rootClassId === cid ? 1 : 0);
  }

  // ---------------------------------------------------------------- live simulation
  let t = 0;
  function tick() {
    t += 1;
    PLAYERS.forEach((p, i) => {
      const a = A.player(i);
      if (i === 4) return; // bot stands still & dead
      const ang = t * 0.04 + i;
      const r = 30 + i * 6;
      const pos = [120 + i * 40 + Math.cos(ang) * r, 64 - i * 12 + Math.sin(ang) * r, 12.5 + (i === 2 ? Math.abs(Math.sin(t * 0.3)) * 2 : 0)];
      wvec(a + 0x10, pos);
      wvec(a + 0x20, [-Math.sin(ang) * r * 0.04 * 4, Math.cos(ang) * r * 0.04 * 4, 0]);
      const hv = new DataView(read(a + 0x2C, 4).buffer).getFloat32(0, true);
      let h = hv;
      if (Math.random() < 0.05) h = Math.max(1, h - 5 - Math.random() * 20);
      else h = Math.min(100, h + 0.4);
      wf(a + 0x2C, h);
      const flags = 1 | (Math.sin(ang * 2) > 0.3 ? 4 : 0) | (Math.random() < 0.03 ? 2 : 0);
      w32(a + 0x3C, flags);
      const wa = A.weapon(i);
      const ammo = new DataView(read(wa + 0x10, 4).buffer).getInt32(0, true);
      if (Math.random() < 0.15) wi32(wa + 0x10, ammo <= 0 ? p.max : ammo - 1);
    });
    wi32(A.world + 0x1C, 184220 + t * 4);
    const cam = new DataView(read(A.player(0) + 0x10, 12).buffer);
    wvec(A.camera + 0x08, [cam.getFloat32(0, true), cam.getFloat32(4, true), 76.5]);
    wvec(A.camera + 0x14, [-4.5 + Math.sin(t * 0.05) * 3, (87.2 + t * 1.3) % 360, 0]);
    emit('tick');
  }
  let timer = null;
  const state = { attached: true, pid: 6716, rootClassId: 1, rootExpr: '[$GWorld]', paused: false };
  function setLive(on) {
    clearInterval(timer); timer = null; state.paused = !on;
    if (on) timer = setInterval(tick, 250);
  }
  setLive(true);

  // ---------------------------------------------------------------- export
  window.Model = {
    TYPES, TYPE_GROUPS, SHORT, MODULES, classes, enums, signatures, processes, state,
    getClass, getEnum, classSize, fieldSize, typeLabel, describe, evalExpr, read, readable,
    hex, addrStr, offStr, fmtFloat, symbolize, ascii, cstr,
    setFieldType, defineAt, insertBytes, removeField, renameField, patchField, renameClass, addClass, removeClass, classRefs,
    setLive, tick,
    // raw pointer access for demos that simulate the target mutating its own memory
    debug: { readPtr: rPtr, writePtr: w64 },
    on: fn => listeners.push(fn),
    rootAddress() { const r = evalExpr(state.rootExpr); return r.ok ? r.value : null; },
    process() { return processes.find(p => p.pid === state.pid); },
    attach(pid) { state.pid = pid; state.attached = !!pid; emit('attach'); },
  };
})();
