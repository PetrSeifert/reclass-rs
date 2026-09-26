//! A simulated process for developing and testing frontends without the driver.
//!
//! `Hollowfield.exe` has a `GameWorld` reachable through the `GWorld` signature,
//! players that move around, an entity list, an encrypted camera pointer and a
//! few fields that are still undiscovered hex in the demo project.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

use crate::{
    layout::hex_fill,
    memory::{
        ClassDefinition,
        EnumDefinition,
        EnumVariant,
        FieldDefinition,
        FieldType,
        MemoryStructure,
        PointerTarget,
    },
    signature::SignatureDef,
    source::{
        MemorySource,
        ModuleEntry,
        ProcessEntry,
        ProcessProvider,
    },
};

pub const DEMO_PID: u32 = 6716;
const ENC_KEY: u64 = 0x3A5F_00C0_FFEE_1234;
const HEAP: (u64, u64) = (0x1F3_0000_0000, 0x1F4_0000_0000);

const EXE_BASE: u64 = 0x7FF6_A260_0000;
const GWORLD_GLOBAL: u64 = EXE_BASE + 0x5A_1230;
const WORLD: u64 = 0x1F3_A8C4_0000;
const ENTITY_LIST: u64 = 0x1F3_A8E0_0000;
const CAMERA: u64 = 0x1F3_A8F0_8800;
const STRINGS: u64 = 0x1F3_A8D6_0000;
const fn player(i: u64) -> u64 {
    0x1F3_A8D1_2400 + i * 0x200
}
const fn weapon(i: u64) -> u64 {
    0x1F3_A8D4_0000 + i * 0x80
}

struct PlayerSeed {
    name: &'static str,
    team: u32,
    weapon: &'static str,
    weapon_type: u32,
    ammo: i32,
    max_ammo: i32,
}

const PLAYERS: [PlayerSeed; 6] = [
    PlayerSeed {
        name: "peterrock",
        team: 1,
        weapon: "AR-7 \"Kestrel\"",
        weapon_type: 1,
        ammo: 27,
        max_ammo: 30,
    },
    PlayerSeed {
        name: "xX_n0sc0pe_Xx",
        team: 2,
        weapon: "Longshot M2",
        weapon_type: 3,
        ammo: 4,
        max_ammo: 5,
    },
    PlayerSeed {
        name: "Ghost",
        team: 1,
        weapon: "Breacher 12g",
        weapon_type: 2,
        ammo: 6,
        max_ammo: 8,
    },
    PlayerSeed {
        name: "Mira",
        team: 2,
        weapon: "AR-7 \"Kestrel\"",
        weapon_type: 1,
        ammo: 30,
        max_ammo: 30,
    },
    PlayerSeed {
        name: "b0t_Harold",
        team: 3,
        weapon: "P9 Sidearm",
        weapon_type: 0,
        ammo: 12,
        max_ammo: 15,
    },
    PlayerSeed {
        name: "Sable",
        team: 2,
        weapon: "Vektor SMG",
        weapon_type: 4,
        ammo: 19,
        max_ammo: 32,
    },
];

fn modules() -> Vec<ModuleEntry> {
    [
        ("Hollowfield.exe", EXE_BASE, 0x120_0000),
        ("GameCore.dll", 0x7FFB_6A10_0000, 0x2C_4000),
        ("ntdll.dll", 0x7FFB_8E6A_0000, 0x1F_8000),
        ("KERNEL32.DLL", 0x7FFB_8D1C_0000, 0xC_2000),
        ("KERNELBASE.dll", 0x7FFB_8BE1_0000, 0x3A_6000),
        ("d3d11.dll", 0x7FFB_86A3_0000, 0x26_4000),
        ("dxgi.dll", 0x7FFB_8821_0000, 0x13_B000),
        ("user32.dll", 0x7FFB_8D3A_0000, 0x1A_D000),
    ]
    .into_iter()
    .map(|(name, base, size)| ModuleEntry {
        name: name.into(),
        base,
        size,
    })
    .collect()
}

pub struct DemoProvider;

impl ProcessProvider for DemoProvider {
    fn list_processes(&self) -> anyhow::Result<Vec<ProcessEntry>> {
        Ok([
            (4, "System"),
            (812, "csrss.exe"),
            (1204, "explorer.exe"),
            (3380, "Discord.exe"),
            (4412, "Code.exe"),
            (5120, "steam.exe"),
            (DEMO_PID, "Hollowfield.exe"),
            (7024, "chrome.exe"),
            (8800, "WindowsTerminal.exe"),
            (9124, "obs64.exe"),
        ]
        .into_iter()
        .map(|(pid, name)| ProcessEntry {
            pid,
            name: name.into(),
        })
        .collect())
    }

    fn attach(&self, pid: u32) -> anyhow::Result<Arc<dyn MemorySource>> {
        anyhow::ensure!(
            pid == DEMO_PID,
            "demo mode can only attach to Hollowfield.exe ({DEMO_PID})"
        );
        Ok(Arc::new(DemoSource::new()))
    }
}

struct Memory {
    pages: HashMap<u64, Box<[u8; 4096]>>,
    modules: Vec<ModuleEntry>,
    ticks: u64,
    rng: u64,
}

impl Memory {
    fn readable(&self, a: u64) -> bool {
        (a >= HEAP.0 && a < HEAP.1)
            || self
                .modules
                .iter()
                .any(|m| a >= m.base && a < m.base + m.size)
    }

    fn page(&mut self, n: u64) -> &mut [u8; 4096] {
        self.pages.entry(n).or_insert_with(|| {
            // Deterministic noise: the heap is mostly zero, images are dense.
            let mut p = Box::new([0u8; 4096]);
            let heap = n * 4096 >= HEAP.0 && n * 4096 < HEAP.1;
            let mut s = (n as u32).wrapping_mul(2_654_435_761).max(1);
            for byte in p.iter_mut() {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                *byte = if heap {
                    if (s & 0xFF) < 200 {
                        0
                    } else {
                        (s >> 8) as u8
                    }
                } else if s & 3 == 0 {
                    0
                } else {
                    (s >> 8) as u8
                };
            }
            p
        })
    }

    fn read(&mut self, addr: u64, buf: &mut [u8]) -> bool {
        if buf.is_empty() {
            return true;
        }
        if !self.readable(addr) || !self.readable(addr + buf.len() as u64 - 1) {
            return false;
        }
        for (i, b) in buf.iter_mut().enumerate() {
            let a = addr + i as u64;
            *b = self.page(a / 4096)[(a % 4096) as usize];
        }
        true
    }

    fn write(&mut self, addr: u64, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            let a = addr + i as u64;
            self.page(a / 4096)[(a % 4096) as usize] = *b;
        }
    }

    fn w16(&mut self, a: u64, v: u16) {
        self.write(a, &v.to_le_bytes());
    }
    fn w32(&mut self, a: u64, v: u32) {
        self.write(a, &v.to_le_bytes());
    }
    fn wi32(&mut self, a: u64, v: i32) {
        self.write(a, &v.to_le_bytes());
    }
    fn wf(&mut self, a: u64, v: f32) {
        self.write(a, &v.to_le_bytes());
    }
    fn w64(&mut self, a: u64, v: u64) {
        self.write(a, &v.to_le_bytes());
    }
    fn wvec(&mut self, a: u64, v: &[f32]) {
        for (i, x) in v.iter().enumerate() {
            self.wf(a + i as u64 * 4, *x);
        }
    }
    fn wstr(&mut self, a: u64, s: &str, pad: usize) {
        let mut b = s.as_bytes().to_vec();
        b.resize(pad.max(s.len() + 1), 0);
        self.write(a, &b);
    }
    fn rf(&mut self, a: u64) -> f32 {
        let mut b = [0u8; 4];
        self.read(a, &mut b);
        f32::from_le_bytes(b)
    }
    fn ri32(&mut self, a: u64) -> i32 {
        let mut b = [0u8; 4];
        self.read(a, &mut b);
        i32::from_le_bytes(b)
    }
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn seed(&mut self) {
        self.w64(GWORLD_GLOBAL, WORLD);
        // GameWorld
        self.w64(WORLD, EXE_BASE + 0x58_E4F0);
        self.w64(WORLD + 0x08, player(0));
        self.w64(WORLD + 0x10, ENTITY_LIST);
        self.wi32(WORLD + 0x18, PLAYERS.len() as i32);
        self.w32(WORLD + 0x1C, 184_220);
        self.wf(WORLD + 0x20, 1.0);
        self.w32(WORLD + 0x24, 0);
        self.wstr(WORLD + 0x28, "de_hollow_ridge", 32);
        self.w64(WORLD + 0x48, CAMERA ^ ENC_KEY);
        self.wf(WORLD + 0x50, -9.81);
        self.w32(WORLD + 0x54, 0);
        self.w64(WORLD + 0x58, EXE_BASE + 0x7C_10A0);
        // EntityList
        self.wi32(ENTITY_LIST, PLAYERS.len() as i32);
        self.wi32(ENTITY_LIST + 4, 8);
        for i in 0..8 {
            let p = if (i as usize) < PLAYERS.len() {
                player(i)
            } else {
                0
            };
            self.w64(ENTITY_LIST + 8 + i * 8, p);
        }
        // Players and weapons
        let mut strings = STRINGS;
        let mut alloc = |m: &mut Self, s: &str| {
            let a = strings;
            m.wstr(a, s, 0);
            strings += (s.len() as u64 + 16) & !15;
            a
        };
        for (i, p) in PLAYERS.iter().enumerate() {
            let (a, w, f) = (player(i as u64), weapon(i as u64), i as f32);
            self.w64(a, EXE_BASE + 0x59_1A08);
            let name = alloc(self, p.name);
            self.w64(a + 0x08, name);
            self.wvec(a + 0x10, &[120.0 + f * 40.0, 64.0 - f * 12.0, 12.5]);
            self.w32(a + 0x1C, 0);
            self.wvec(a + 0x20, &[0.0, 0.0, 0.0]);
            self.wf(a + 0x2C, 100.0 - f * 13.0);
            self.wf(a + 0x30, 100.0);
            self.wf(a + 0x34, [50.0, 100.0, 0.0, 75.0, 25.0, 100.0][i]);
            self.w32(a + 0x38, p.team);
            self.w32(a + 0x3C, 1 | if i % 3 == 0 { 4 } else { 0 });
            self.w64(a + 0x40, w);
            self.w64(a + 0x48, WORLD);
            self.wi32(a + 0x50, [14, 22, 9, 17, 0, 11][i]);
            self.wi32(a + 0x54, [6, 9, 12, 8, 21, 7][i]);
            self.w16(a + 0x58, [23, 41, 18, 67, 0, 35][i]);
            self.w16(a + 0x5A, 0);
            self.w32(a + 0x5C, 0x1000 + i as u32);
            self.w64(a + 0x60, 0);
            self.w64(a + 0x68, 0);
            self.w64(w, EXE_BASE + 0x59_C2E8);
            let wname = alloc(self, p.weapon);
            self.w64(w + 0x08, wname);
            self.wi32(w + 0x10, p.ammo);
            self.wi32(w + 0x14, p.max_ammo);
            self.wf(w + 0x18, [0.1, 1.4, 0.9, 0.1, 0.25, 0.07][i]);
            self.w32(w + 0x1C, p.weapon_type);
            self.wf(w + 0x20, [24.0, 95.0, 12.0, 24.0, 18.0, 16.0][i]);
            self.wi32(w + 0x24, 90);
            self.w64(w + 0x28, 0);
        }
        // Camera
        self.w64(CAMERA, EXE_BASE + 0x5A_0010);
        self.wvec(CAMERA + 0x08, &[120.0, 64.0, 76.5]);
        self.wvec(CAMERA + 0x14, &[-4.5, 87.2, 0.0]);
        self.wf(CAMERA + 0x20, 90.0);
        self.wf(CAMERA + 0x24, 0.1);
        self.wf(CAMERA + 0x28, 5000.0);
        self.w32(CAMERA + 0x2C, 0);
        for (i, row) in [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-120.0, -64.0, -76.5, 1.0],
        ]
        .iter()
        .enumerate()
        {
            self.wvec(CAMERA + 0x30 + i as u64 * 16, row);
        }
    }

    fn tick(&mut self) {
        self.ticks += 1;
        let t = self.ticks as f32;
        for (i, p) in PLAYERS.iter().enumerate() {
            if i == 4 {
                continue; // the bot stands still
            }
            let (a, f) = (player(i as u64), i as f32);
            let ang = t * 0.04 + f;
            let r = 30.0 + f * 6.0;
            let hop = if i == 2 {
                (t * 0.3).sin().abs() * 2.0
            } else {
                0.0
            };
            self.wvec(
                a + 0x10,
                &[
                    120.0 + f * 40.0 + ang.cos() * r,
                    64.0 - f * 12.0 + ang.sin() * r,
                    12.5 + hop,
                ],
            );
            self.wvec(
                a + 0x20,
                &[-ang.sin() * r * 0.16, ang.cos() * r * 0.16, 0.0],
            );
            let mut health = self.rf(a + 0x2C);
            health = if self.random() < 0.05 {
                (health - 5.0 - self.random() * 20.0).max(1.0)
            } else {
                (health + 0.4).min(100.0)
            };
            self.wf(a + 0x2C, health);
            let flags = 1
                | if (ang * 2.0).sin() > 0.3 { 4 } else { 0 }
                | if self.random() < 0.03 { 2 } else { 0 };
            self.w32(a + 0x3C, flags);
            let w = weapon(i as u64);
            let ammo = self.ri32(w + 0x10);
            if self.random() < 0.15 {
                self.wi32(w + 0x10, if ammo <= 0 { p.max_ammo } else { ammo - 1 });
            }
        }
        self.w32(WORLD + 0x1C, 184_220 + self.ticks as u32 * 4);
        let origin = [self.rf(player(0) + 0x10), self.rf(player(0) + 0x14), 76.5];
        self.wvec(CAMERA + 0x08, &origin);
        self.wvec(
            CAMERA + 0x14,
            &[-4.5 + (t * 0.05).sin() * 3.0, (87.2 + t * 1.3) % 360.0, 0.0],
        );
    }
}

pub struct DemoSource {
    memory: Mutex<Memory>,
}

impl DemoSource {
    pub fn new() -> Self {
        let mut m = Memory {
            pages: HashMap::new(),
            modules: modules(),
            ticks: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
        };
        m.seed();
        Self {
            memory: Mutex::new(m),
        }
    }

    /// Writes raw bytes, e.g. to simulate the target rearranging its own memory.
    pub fn poke(&self, address: u64, bytes: &[u8]) {
        self.memory.lock().unwrap().write(address, bytes);
    }
}

impl Default for DemoSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySource for DemoSource {
    fn process(&self) -> ProcessEntry {
        ProcessEntry {
            pid: DEMO_PID,
            name: "Hollowfield.exe".into(),
        }
    }

    fn modules(&self) -> Vec<ModuleEntry> {
        self.memory.lock().unwrap().modules.clone()
    }

    fn read(&self, address: u64, buffer: &mut [u8]) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.memory.lock().unwrap().read(address, buffer),
            "cannot read 0x{address:X}"
        );
        Ok(())
    }

    fn resolve_signature(&self, sig: &SignatureDef) -> anyhow::Result<u64> {
        match sig.name.as_str() {
            "GWorld" => Ok(GWORLD_GLOBAL),
            "LocalPlayerOff" => Ok(0x08),
            "XenuineDecrypt" => Ok(0x7FFB_6A1F_37D6),
            _ => anyhow::bail!("failed to find pattern {}", sig.name),
        }
    }

    fn decrypt(&self, value: u64) -> anyhow::Result<u64> {
        Ok(value ^ ENC_KEY)
    }

    fn tick(&self) {
        self.memory.lock().unwrap().tick();
    }
}

/// The class layout that goes with the demo process, part-way through reversing.
pub fn demo_project() -> (MemoryStructure, Vec<SignatureDef>) {
    fn named(name: &str, t: FieldType) -> FieldDefinition {
        FieldDefinition::new_named(name.into(), t, 0)
    }
    fn hex(t: FieldType) -> FieldDefinition {
        FieldDefinition::new_hex(t, 0)
    }
    fn ptr(name: &str, t: FieldType, target: PointerTarget) -> FieldDefinition {
        let mut f = named(name, t);
        f.pointer_target = Some(target);
        f
    }
    fn class(name: &str, fields: Vec<FieldDefinition>) -> ClassDefinition {
        let mut c = ClassDefinition::new(name.into());
        for f in fields {
            c.add_field(f);
        }
        c
    }
    fn enumeration(name: &str, flags: bool, variants: &[(&str, u32)]) -> EnumDefinition {
        let mut e = EnumDefinition::new(name.into());
        e.is_flags = flags;
        e.variants = variants
            .iter()
            .map(|(n, v)| EnumVariant {
                name: (*n).into(),
                value: *v,
            })
            .collect();
        e
    }
    fn with_enum(mut f: FieldDefinition, id: u64) -> FieldDefinition {
        f.enum_id = Some(id);
        f
    }

    let team = enumeration(
        "ETeam",
        false,
        &[("None", 0), ("Red", 1), ("Blue", 2), ("Spectator", 3)],
    );
    let wtype = enumeration(
        "EWeaponType",
        false,
        &[
            ("Pistol", 0),
            ("Rifle", 1),
            ("Shotgun", 2),
            ("Sniper", 3),
            ("SMG", 4),
        ],
    );
    let flags = enumeration(
        "EPlayerFlags",
        true,
        &[
            ("OnGround", 1),
            ("Crouching", 2),
            ("Sprinting", 4),
            ("Reloading", 8),
        ],
    );

    let weapon = class(
        "Weapon",
        vec![
            hex(FieldType::Hex64),
            named("displayName", FieldType::TextPointer),
            named("ammo", FieldType::Int32),
            named("maxAmmo", FieldType::Int32),
            named("fireRate", FieldType::Float),
            with_enum(named("weaponType", FieldType::Enum), wtype.id),
            hex(FieldType::Hex64),
            hex(FieldType::Hex64),
        ],
    );
    let mut player_fields = vec![
        hex(FieldType::Hex64),
        named("name", FieldType::TextPointer),
        named("position", FieldType::Vector3),
        hex(FieldType::Hex32),
        named("velocity", FieldType::Vector3),
        named("health", FieldType::Float),
        named("maxHealth", FieldType::Float),
        hex(FieldType::Hex32),
        with_enum(named("team", FieldType::Enum), team.id),
        with_enum(named("flags", FieldType::Enum), flags.id),
        ptr(
            "weapon",
            FieldType::Pointer,
            PointerTarget::ClassId(weapon.id),
        ),
    ];
    player_fields.extend(hex_fill(0x28).into_iter().map(hex));
    let player = class("Player", player_fields);
    let camera = class(
        "Camera",
        vec![
            hex(FieldType::Hex64),
            named("origin", FieldType::Vector3),
            named("angles", FieldType::Vector3),
            named("fov", FieldType::Float),
            hex(FieldType::Hex32),
            hex(FieldType::Hex32),
            hex(FieldType::Hex32),
            named("viewRow0", FieldType::Vector4),
            named("viewRow1", FieldType::Vector4),
            named("viewRow2", FieldType::Vector4),
            named("viewRow3", FieldType::Vector4),
        ],
    );
    let mut entities = named("entities", FieldType::Array);
    entities.array_element = Some(PointerTarget::Pointer(Box::new(PointerTarget::ClassId(
        player.id,
    ))));
    entities.array_length = Some(8);
    let entity_list = class(
        "EntityList",
        vec![
            named("count", FieldType::Int32),
            named("capacity", FieldType::Int32),
            entities,
        ],
    );
    let world = class(
        "GameWorld",
        vec![
            hex(FieldType::Hex64),
            ptr(
                "localPlayer",
                FieldType::Pointer,
                PointerTarget::ClassId(player.id),
            ),
            ptr(
                "entityList",
                FieldType::Pointer,
                PointerTarget::ClassId(entity_list.id),
            ),
            named("entityCount", FieldType::Int32),
            named("tickCount", FieldType::UInt32),
            named("timeScale", FieldType::Float),
            named("isPaused", FieldType::Bool),
            hex(FieldType::Hex8),
            hex(FieldType::Hex16),
            named("mapName", FieldType::Text),
            ptr(
                "camera",
                FieldType::EncryptedPointer,
                PointerTarget::ClassId(camera.id),
            ),
            hex(FieldType::Hex32),
            hex(FieldType::Hex32),
            hex(FieldType::Hex64),
        ],
    );

    let mut ms = MemoryStructure::new("root".into(), WORLD, world);
    for c in [
        player,
        weapon,
        entity_list,
        camera,
        class(
            "UnknownClass",
            hex_fill(0x20).into_iter().map(hex).collect(),
        ),
    ] {
        ms.class_registry.register(c);
    }
    for e in [team, wtype, flags] {
        ms.enum_registry.register(e);
    }
    let sig = |name: &str, module: &str, pattern: &str, offset, rel: Option<u64>| SignatureDef {
        name: name.into(),
        module: module.into(),
        pattern: pattern.into(),
        offset,
        is_relative: rel.is_some(),
        rel_inst_len: rel.unwrap_or(0),
    };
    let signatures = vec![
        sig(
            "GWorld",
            "Hollowfield.exe",
            "48 8B 05 ?? ?? ?? ?? 48 85 C0 74 ?? 48 8B 40 18",
            3,
            Some(7),
        ),
        sig(
            "LocalPlayerOff",
            "Hollowfield.exe",
            "48 8B 8B ?? ?? ?? ?? 48 85 C9 0F 84",
            3,
            None,
        ),
        sig(
            "XenuineDecrypt",
            "GameCore.dll",
            "48 8B 05 ?? ?? ?? ?? FF D0 48 8B C8",
            3,
            Some(7),
        ),
        sig(
            "ViewMatrix",
            "Hollowfield.exe",
            "F3 0F 10 0D ?? ?? ?? ?? 0F 28 C1 E8",
            4,
            Some(8),
        ),
    ];
    (ms, signatures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        decode::Decoder,
        layout::Layout,
    };

    #[test]
    fn demo_project_decodes_against_demo_memory() {
        let (ms, _) = demo_project();
        let src = DemoSource::new();
        let lay = Layout::of(&ms);
        let dec = Decoder::new(&src, lay);
        let world = ms.root_class.class_id;
        assert_eq!(lay.class_size(world), 0x60);
        let def = ms.class_registry.get(world).unwrap();
        let offs = lay.field_offsets(world);
        let row = |name: &str| {
            let i = def
                .fields
                .iter()
                .position(|f| f.name.as_deref() == Some(name))
                .unwrap();
            dec.decode(&def.fields[i], WORLD + offs[i].0)
        };
        assert_eq!(row("mapName").value, "\"de_hollow_ridge\"");
        assert_eq!(row("timeScale").value, "1.0");
        let cam = row("camera");
        assert_eq!(cam.pointer, Some(CAMERA));
        assert!(matches!(
            cam.child,
            Some(crate::decode::Child::Class {
                base: CAMERA,
                via_pointer: true,
                ..
            })
        ));
        let player_id = ms
            .class_registry
            .get_class_ids()
            .into_iter()
            .find(|id| ms.class_registry.get(*id).unwrap().name == "Player")
            .unwrap();
        assert_eq!(lay.class_size(player_id), 0x70);
        src.tick();
        assert_ne!(row("tickCount").value, "184220");
    }
}
