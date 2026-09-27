//! Everything the server holds for its clients: the attached process, the
//! project (definitions + signatures), the root expression and the card graph.
//! Every client (web UI, future CLI) drives it through `handle`.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

use reclass_core::{
    decode::{
        element_field,
        type_label,
        Decoder,
    },
    edit::{
        self,
        TypeSpec,
    },
    encode::encode,
    expr::{
        self,
        ExprContext,
    },
    layout::Layout,
    memory::{
        ClassDefinition,
        FieldType,
        MemoryStructure,
        PointerTarget,
    },
    project::ProjectFile,
    signature::SignatureDef,
    source::{
        MemorySource,
        ModuleEntry,
        ProcessProvider,
    },
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use serde_json::{
    json,
    Value,
};

use crate::{
    canvas::{
        hex,
        Canvas,
        Resolved,
        ROOT,
    },
    inspect::Inspector,
    scanning::{
        Pinned,
        Target,
    },
    watches::{
        self,
        Saved,
        Watch,
    },
};

/// What changed as a result of a command, so the caller knows what to broadcast.
#[derive(Default, Clone, Copy)]
pub struct Changed {
    pub defs: bool,
    pub session: bool,
    pub watches: bool,
}

impl Changed {
    const DEFS: Self = Self {
        defs: true,
        session: true,
        watches: true,
    };
    const SESSION: Self = Self {
        defs: false,
        session: true,
        watches: false,
    };
    const NONE: Self = Self {
        defs: false,
        session: false,
        watches: false,
    };
    /// Watches changed, and with them the project (the session's dirty flag).
    const WATCHES: Self = Self {
        defs: false,
        session: true,
        watches: true,
    };
}

pub struct Workspace {
    provider: Arc<dyn ProcessProvider>,
    source: Option<Arc<dyn MemorySource>>,
    memory: MemoryStructure,
    signatures: Vec<SignatureDef>,
    sig_values: HashMap<String, Result<u64, String>>,
    root_expr: String,
    canvas: Canvas,
    resolved: HashMap<u64, Resolved>,
    pub live: bool,
    project_path: Option<PathBuf>,
    dirty: bool,
    demo: bool,
    seq: u64,
    watches: Vec<Watch>,
    next_watch: u64,
}

struct EvalCtx<'a> {
    source: &'a dyn MemorySource,
    modules: Vec<ModuleEntry>,
    sigs: &'a HashMap<String, Result<u64, String>>,
    pointer_size: u64,
}

impl ExprContext for EvalCtx<'_> {
    fn module_base(&self, name: &str) -> Option<u64> {
        self.modules
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .map(|m| m.base)
    }

    fn signature(&self, name: &str) -> Result<u64, String> {
        match self.sigs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
            Some((_, r)) => r.clone(),
            None => Err("unknown signature".into()),
        }
    }

    fn read_pointer(&self, address: u64) -> Option<u64> {
        self.source.read_pointer(address, self.pointer_size)
    }
}

fn arg<T: DeserializeOwned>(params: &Value) -> Result<T, String> {
    serde_json::from_value(params.clone()).map_err(|e| format!("bad params: {e}"))
}

fn blank_project() -> ProjectFile {
    let mut root = ClassDefinition::new("Root".into());
    for t in reclass_core::layout::hex_fill(0x40, 8) {
        root.add_hex_field(t);
    }
    ProjectFile {
        memory: MemoryStructure::new("root".into(), 0, root),
        signatures: vec![],
        web: None,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldRef {
    class_id: u64,
    field_id: u64,
}

impl Workspace {
    pub fn new(
        provider: Arc<dyn ProcessProvider>,
        project: Option<ProjectFile>,
        project_path: Option<PathBuf>,
        demo: bool,
    ) -> Self {
        let mut ws = Self {
            provider,
            source: None,
            memory: blank_project().memory,
            signatures: vec![],
            sig_values: HashMap::new(),
            root_expr: "0".into(),
            canvas: Canvas::default(),
            resolved: HashMap::new(),
            live: true,
            project_path,
            dirty: false,
            demo,
            seq: 0,
            watches: Vec::new(),
            next_watch: 1,
        };
        ws.load_project(project.unwrap_or_else(blank_project));
        ws
    }

    fn load_project(&mut self, p: ProjectFile) {
        self.memory = p.memory;
        self.signatures = p.signatures;
        let web = p.web.unwrap_or(Value::Null);
        self.root_expr = web
            .get("rootExpr")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("0x{:X}", self.memory.root_class.address));
        self.canvas = web
            .get("canvas")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();
        let saved: Vec<Saved> = web
            .get("watches")
            .and_then(|w| serde_json::from_value(w.clone()).ok())
            .unwrap_or_default();
        self.watches.clear();
        for w in saved {
            // A type this version does not know is dropped rather than failing the load.
            if let Ok(ty) = watches::parse_type(&w.ty) {
                self.watches
                    .push(Watch::new(self.next_watch, w.label, w.expr, ty));
                self.next_watch += 1;
            }
        }
        self.resolved.clear();
        self.dirty = false;
        self.match_target();
        self.rescan();
    }

    fn project(&self) -> ProjectFile {
        let mut memory = self.memory.clone();
        // The egui app stores the root address as a number.
        memory.root_class.address = self.root_address().unwrap_or(memory.root_class.address);
        ProjectFile {
            memory,
            signatures: self.signatures.clone(),
            web: Some(json!({
                "rootExpr": self.root_expr,
                "canvas": self.canvas,
                "watches": self.watches.iter().map(Watch::saved).collect::<Vec<_>>(),
            })),
        }
    }

    pub fn attach(&mut self, pid: u32) -> Result<(), String> {
        self.source = Some(self.provider.attach(pid).map_err(|e| e.to_string())?);
        self.resolved.clear();
        self.match_target();
        self.rescan();
        Ok(())
    }

    /// Converts the project to the attached process's pointer size, since
    /// definitions laid out for another size misread every pointer.
    fn match_target(&mut self) {
        let Some(size) = self.source.as_ref().and_then(|s| s.detect_pointer_size()) else {
            return;
        };
        if size == self.memory.pointer_size {
            return;
        }
        match edit::set_pointer_size(&mut self.memory, size) {
            Ok(()) => {
                log::info!(
                    "project converted to {}-bit pointers to match the process",
                    size * 8
                );
                self.resolved.clear();
                self.dirty = true;
            }
            Err(e) => log::warn!("cannot convert project to {size}-byte pointers: {e}"),
        }
    }

    fn rescan(&mut self) {
        self.sig_values = self
            .signatures
            .iter()
            .map(|s| {
                let r = match &self.source {
                    Some(src) => src.resolve_signature(s).map_err(|e| e.to_string()),
                    None => Err("not attached".into()),
                };
                (s.name.clone(), r)
            })
            .collect();
    }

    fn eval(&self, expr: &str) -> Result<u64, String> {
        let src = self.source.as_deref().ok_or("not attached")?;
        expr::eval(
            expr,
            &EvalCtx {
                source: src,
                modules: src.modules(),
                sigs: &self.sig_values,
                pointer_size: self.memory.pointer_size,
            },
        )
    }

    fn root_address(&self) -> Result<u64, String> {
        self.eval(&self.root_expr)
    }

    fn root_class(&self) -> u64 {
        self.memory.root_class.class_id
    }

    pub fn tick(&mut self) {
        if let Some(src) = &self.source {
            src.tick();
        }
    }

    /// What a scan should read: the process, and the module or the `start` and
    /// `end` expressions the params name, if any. `None` when detached.
    pub fn scan_target(&self, p: &Value) -> Result<Option<Target>, String> {
        let Some(source) = self.source.clone() else {
            return Ok(None);
        };
        let text = |k: &str| p.get(k).and_then(Value::as_str);
        let within = match (text("module"), text("start"), text("end")) {
            (Some(name), None, None) => {
                let m = source
                    .module_by_name(name)
                    .ok_or_else(|| format!("no module {name}"))?;
                Some((m.base, m.base + m.size))
            }
            (None, Some(start), Some(end)) => {
                let (start, end) = (self.eval(start)?, self.eval(end)?);
                if end <= start {
                    return Err("the range ends before it starts".into());
                }
                // Probing costs about a second per 4 GiB of address space.
                if end - start > 64 << 30 {
                    return Err("the range is larger than 64 GiB".into());
                }
                Some((start, end))
            }
            (None, None, None) => None,
            _ => return Err("pass a module, or both start and end".into()),
        };
        Ok(Some(Target {
            source,
            pointer_size: self.memory.pointer_size,
            within,
        }))
    }

    pub fn has_watches(&self) -> bool {
        !self.watches.is_empty()
    }

    /// Re-reads every watch, writing frozen ones back first.
    pub fn sample_watches(&mut self) {
        let Some(src) = self.source.clone() else {
            return;
        };
        for i in 0..self.watches.len() {
            let address = self.eval(&self.watches[i].expr);
            self.watches[i].sample(&*src, address);
        }
    }

    pub fn watches(&self) -> Value {
        json!({ "type": "watches", "watches": self.watches.iter().map(Watch::view).collect::<Vec<_>>() })
    }

    fn watch_mut(&mut self, id: u64) -> Result<&mut Watch, String> {
        self.watches
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| format!("no watch #{id}"))
    }

    /// The watch a related scan follows: where it is now and its type.
    pub fn watch_pin(&self, id: u64) -> Result<Pinned, String> {
        let w = self
            .watches
            .iter()
            .find(|w| w.id == id)
            .ok_or_else(|| format!("no watch #{id}"))?;
        Ok(Pinned {
            label: w.label.clone(),
            address: self.eval(&w.expr)?,
            ty: w.ty,
        })
    }

    fn writer(&self) -> Result<Arc<dyn MemorySource>, String> {
        let src = self.source.clone().ok_or("not attached")?;
        if !src.can_write() {
            return Err("the driver cannot write memory".into());
        }
        Ok(src)
    }

    pub fn is_attached(&self) -> bool {
        self.source.is_some()
    }

    /// Re-reads memory and re-resolves all cards.
    fn resolve(&mut self) -> Option<Result<u64, String>> {
        let src = self.source.clone()?;
        let root = self.root_address();
        let dec = Decoder::new(&*src, Layout::of(&self.memory));
        self.resolved = self
            .canvas
            .resolve(&dec, self.root_class(), root.as_ref().ok().copied());
        Some(root)
    }

    pub fn frame(&mut self) -> Value {
        self.seq += 1;
        let root = self.resolve();
        let cards = match &self.source {
            Some(src) => self.canvas.view(
                &Decoder::new(&**src, Layout::of(&self.memory)),
                &self.resolved,
            ),
            None => vec![],
        };
        let (address, error) = match root {
            Some(Ok(a)) => (Some(hex(a)), None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, Some("not attached".to_string())),
        };
        json!({ "type": "frame", "seq": self.seq, "rootAddress": address, "rootError": error, "cards": cards })
    }

    pub fn session(&self) -> Value {
        json!({
            "type": "session",
            "attached": self.source.as_ref().map(|s| s.process()),
            "live": self.live,
            "rootExpr": self.root_expr,
            "rootClassId": self.root_class(),
            "share": self.canvas.share,
            "projectPath": self.project_path,
            "dirty": self.dirty,
            "demo": self.demo,
            "pointerSize": self.memory.pointer_size,
            "canWrite": self.source.as_ref().is_some_and(|s| s.can_write()),
        })
    }

    pub fn defs(&self) -> Value {
        let lay = Layout::of(&self.memory);
        let mut ids = self.memory.class_registry.get_class_ids();
        ids.sort_unstable();
        let classes: Vec<Value> = ids
            .iter()
            .map(|id| {
                let c = self.memory.class_registry.get(*id).unwrap();
                let offsets = lay.field_offsets(*id);
                json!({
                    "id": c.id,
                    "name": c.name,
                    "size": lay.class_size(*id),
                    "refs": edit::class_refs(&self.memory, *id),
                    "fields": c.fields.iter().zip(offsets).map(|(f, (offset, size))| json!({
                        "id": f.id, "name": f.name, "ty": f.field_type, "typeLabel": type_label(&lay, f), "offset": offset, "size": size,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let mut enum_ids = self.memory.enum_registry.get_enum_ids();
        enum_ids.sort_unstable();
        let enums: Vec<Value> = enum_ids
            .iter()
            .map(|id| {
                let e = self.memory.enum_registry.get(*id).unwrap();
                json!({ "id": e.id, "name": e.name, "isFlags": e.is_flags, "size": e.default_size, "refs": edit::enum_refs(&self.memory, *id), "variants": e.variants.iter().map(|v| json!([v.name, v.value])).collect::<Vec<_>>() })
            })
            .collect();
        let signatures: Vec<Value> = self
            .signatures
            .iter()
            .map(|s| {
                let r = self.sig_values.get(&s.name);
                json!({
                    "name": s.name, "module": s.module, "pattern": s.pattern, "offset": s.offset,
                    "isRelative": s.is_relative, "relInstLen": s.rel_inst_len,
                    "value": r.and_then(|r| r.as_ref().ok()).map(|v| hex(*v)),
                    "error": r.and_then(|r| r.as_ref().err()),
                })
            })
            .collect();
        json!({ "type": "defs", "classes": classes, "enums": enums, "signatures": signatures })
    }

    fn ensure_resolved(&mut self) {
        if self.resolved.is_empty() {
            self.resolve();
        }
    }

    /// Runs one command. Returns the result plus what needs re-broadcasting;
    /// the caller always sends a fresh frame afterwards.
    pub fn handle(&mut self, method: &str, p: &Value) -> Result<(Value, Changed), String> {
        let ok = |c: Changed| Ok((Value::Null, c));
        let edited = |ws: &mut Self, r: Result<(), String>| {
            r?;
            ws.dirty = true;
            Ok((Value::Null, Changed::DEFS))
        };
        match method {
            // ---- process
            "processes" => Ok((
                json!(self.provider.list_processes().map_err(|e| e.to_string())?),
                Changed::NONE,
            )),
            "attach" => {
                #[derive(Deserialize)]
                struct A {
                    pid: u32,
                }
                let a: A = arg(p)?;
                self.attach(a.pid)?;
                ok(Changed::DEFS)
            }
            "setPointerSize" => {
                let size = p
                    .get("size")
                    .and_then(Value::as_u64)
                    .ok_or("bad params: size")?;
                let r = edit::set_pointer_size(&mut self.memory, size);
                self.resolved.clear();
                edited(self, r)
            }
            "detach" => {
                self.source = None;
                self.resolved.clear();
                self.rescan();
                ok(Changed::DEFS)
            }
            "modules" => {
                let src = self.source.as_ref().ok_or("not attached")?;
                let mods: Vec<Value> = src
                    .modules()
                    .iter()
                    .map(|m| json!({ "name": m.name, "base": hex(m.base), "size": m.size }))
                    .collect();
                Ok((json!(mods), Changed::NONE))
            }
            "setLive" => {
                self.live = p
                    .get("on")
                    .and_then(Value::as_bool)
                    .ok_or("bad params: on")?;
                ok(Changed::SESSION)
            }
            // ---- root & reads
            "setRoot" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    expr: Option<String>,
                    class_id: Option<u64>,
                }
                let a: A = arg(p)?;
                if let Some(e) = a.expr {
                    self.root_expr = e;
                }
                if let Some(cid) = a.class_id {
                    if cid != self.root_class() {
                        if !self.memory.set_root_class_by_id(cid) {
                            return Err(format!("no class #{cid}"));
                        }
                        self.canvas.close_all();
                        if let Some(root) = self.canvas.cards.iter_mut().find(|c| c.id == ROOT) {
                            root.expanded.clear();
                        }
                    }
                }
                self.dirty = true;
                ok(Changed::DEFS)
            }
            "eval" => {
                let e = p
                    .get("expr")
                    .and_then(Value::as_str)
                    .ok_or("bad params: expr")?;
                Ok((json!(hex(self.eval(e)?)), Changed::NONE))
            }
            "read" => {
                #[derive(Deserialize)]
                struct A {
                    address: String,
                    len: usize,
                }
                let a: A = arg(p)?;
                let addr = self.eval(&a.address)?;
                let src = self.source.as_deref().ok_or("not attached")?;
                let bytes = src
                    .read_vec(addr, a.len.min(0x10000))
                    .ok_or_else(|| format!("cannot read 0x{addr:X}"))?;
                Ok((
                    json!({ "address": hex(addr), "bytes": bytes.iter().map(|b| format!("{b:02X}")).collect::<String>() }),
                    Changed::NONE,
                ))
            }
            "write" => {
                // A value typed for a card row, or for a type at an address.
                #[derive(Deserialize)]
                struct A {
                    value: String,
                    card: Option<u64>,
                    key: Option<String>,
                    address: Option<String>,
                    ty: Option<FieldType>,
                    target: Option<PointerTarget>,
                }
                let a: A = arg(p)?;
                let src = self.source.clone().ok_or("not attached")?;
                if !src.can_write() {
                    return Err("the driver cannot write memory".into());
                }
                let (fd, address) = match (a.card, a.key, a.address, a.ty) {
                    (Some(card), Some(key), None, None) => {
                        // Where the row is now, not where the last frame saw it.
                        self.resolve();
                        let row = self
                            .resolved
                            .get(&card)
                            .and_then(|r| r.rows.iter().find(|r| r.key == key))
                            .ok_or("unknown row")?;
                        (row.field.clone(), row.address)
                    }
                    (None, None, Some(address), Some(ty)) => {
                        let mut fd = element_field(&PointerTarget::FieldType(ty), 0);
                        match a.target {
                            Some(PointerTarget::EnumId(id)) => fd.enum_id = Some(id),
                            t => fd.pointer_target = t,
                        }
                        (fd, self.eval(&address)?)
                    }
                    _ => return Err("pass card and key, or address and ty".into()),
                };
                let layout = Layout::of(&self.memory);
                let bytes = match encode(&layout, &fd, &a.value) {
                    // Pointers also take an address expression, e.g. `<game.exe>+0x10`.
                    Err(e) if fd.field_type == FieldType::Pointer => {
                        let v = self.eval(&a.value).map_err(|_| e)?;
                        encode(&layout, &fd, &format!("{v:X}"))?
                    }
                    r => r?,
                };
                src.write(address, &bytes).map_err(|e| e.to_string())?;
                Ok((
                    json!({ "address": hex(address), "bytes": bytes.iter().map(|b| format!("{b:02X}")).collect::<String>() }),
                    Changed::NONE,
                ))
            }
            "inspect" => {
                // Decodes an instance without opening cards. With no address it
                // shows the root; with `size` instead of a class, raw hex fields.
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    address: Option<String>,
                    class_id: Option<u64>,
                    size: Option<u64>,
                    #[serde(default)]
                    follow: u32,
                    elements: Option<u32>,
                }
                let a: A = arg(p)?;
                let src = self.source.clone().ok_or("not attached")?;
                let base = match &a.address {
                    Some(e) => self.eval(e)?,
                    None => self.root_address()?,
                };
                let dec = Decoder::new(&*src, Layout::of(&self.memory));
                let mut inspector = Inspector::new(&dec, a.elements.unwrap_or(16).min(4096));
                let mut result = match (a.size, a.class_id, &a.address) {
                    (Some(size), None, _) => {
                        json!({ "size": size.min(0x10000), "rows": inspector.span(size.min(0x10000), base) })
                    }
                    (None, class_id, address) => {
                        let class_id = match (class_id, address) {
                            (Some(id), _) => id,
                            (None, None) => self.root_class(),
                            (None, Some(_)) => return Err("pass classId or size".into()),
                        };
                        let class = self
                            .memory
                            .class_registry
                            .get(class_id)
                            .ok_or_else(|| format!("no class #{class_id}"))?;
                        json!({
                            "classId": class_id,
                            "className": class.name,
                            "size": dec.layout.class_size(class_id),
                            "rows": inspector.class(class_id, base, a.follow.min(8)),
                        })
                    }
                    (Some(_), Some(_), _) => return Err("pass either classId or size".into()),
                };
                result["address"] = json!(hex(base));
                if inspector.truncated() {
                    result["truncated"] = json!(true);
                }
                Ok((result, Changed::NONE))
            }
            // ---- watches
            "watches" => Ok((self.watches(), Changed::NONE)),
            "watchAdd" => {
                #[derive(Deserialize)]
                struct A {
                    expr: String,
                    #[serde(rename = "type")]
                    ty: String,
                    label: Option<String>,
                }
                let a: A = arg(p)?;
                let ty = watches::parse_type(&a.ty)?;
                if self.source.is_some() {
                    self.eval(&a.expr)?;
                }
                let label = a
                    .label
                    .filter(|l| !l.trim().is_empty())
                    .unwrap_or_else(|| a.expr.clone());
                let id = self.next_watch;
                self.next_watch += 1;
                self.watches.push(Watch::new(id, label, a.expr, ty));
                self.sample_watches();
                self.dirty = true;
                Ok((json!(id), Changed::WATCHES))
            }
            "watchUpdate" => {
                #[derive(Deserialize)]
                struct A {
                    id: u64,
                    label: Option<String>,
                    expr: Option<String>,
                    #[serde(rename = "type")]
                    ty: Option<String>,
                }
                let a: A = arg(p)?;
                let ty = a.ty.as_deref().map(watches::parse_type).transpose()?;
                if let (Some(e), true) = (&a.expr, self.source.is_some()) {
                    self.eval(e)?;
                }
                let w = self.watch_mut(a.id)?;
                if let Some(l) = a.label.filter(|l| !l.trim().is_empty()) {
                    w.label = l;
                }
                if a.expr.is_some() || ty.is_some() {
                    w.expr = a.expr.unwrap_or_else(|| w.expr.clone());
                    w.ty = ty.unwrap_or(w.ty);
                    w.frozen = None;
                    w.reset();
                }
                self.sample_watches();
                self.dirty = true;
                ok(Changed::WATCHES)
            }
            "watchRemove" => {
                let id = p
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or("bad params: id")?;
                self.watch_mut(id)?;
                self.watches.retain(|w| w.id != id);
                self.dirty = true;
                ok(Changed::WATCHES)
            }
            "watchFreeze" => {
                // Holds the value given, or the current one.
                #[derive(Deserialize)]
                struct A {
                    id: u64,
                    on: bool,
                    value: Option<String>,
                }
                let a: A = arg(p)?;
                if a.on {
                    self.writer()?;
                }
                let w = self.watch_mut(a.id)?;
                w.frozen = match (a.on, a.value) {
                    (false, _) => None,
                    (true, Some(v)) => Some(w.ty.encode(&v)?),
                    (true, None) => Some(w.value.clone().ok_or("the value cannot be read")?),
                };
                self.sample_watches();
                Ok((
                    Value::Null,
                    Changed {
                        watches: true,
                        ..Changed::NONE
                    },
                ))
            }
            "watchSet" => {
                #[derive(Deserialize)]
                struct A {
                    id: u64,
                    value: String,
                }
                let a: A = arg(p)?;
                let src = self.writer()?;
                let (expr, ty) = {
                    let w = self.watch_mut(a.id)?;
                    (w.expr.clone(), w.ty)
                };
                let bytes = ty.encode(&a.value)?;
                let address = self.eval(&expr)?;
                src.write(address, &bytes).map_err(|e| e.to_string())?;
                // A frozen watch holds the new value from now on.
                let w = self.watch_mut(a.id)?;
                if w.frozen.is_some() {
                    w.frozen = Some(bytes);
                }
                self.sample_watches();
                Ok((
                    Value::Null,
                    Changed {
                        watches: true,
                        ..Changed::NONE
                    },
                ))
            }
            "snapshot" => Ok((self.frame(), Changed::NONE)),
            "state" => Ok((
                json!({ "session": self.session(), "defs": self.defs() }),
                Changed::NONE,
            )),
            // ---- definitions
            "retype" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: u64,
                    #[serde(flatten)]
                    spec: TypeSpec,
                }
                let a: A = arg(p)?;
                let r = edit::retype_field(&mut self.memory, a.class_id, a.field_id, a.spec);
                edited(self, r)
            }
            "defineAt" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    offset: u64,
                    #[serde(flatten)]
                    spec: TypeSpec,
                }
                let a: A = arg(p)?;
                let field_id = edit::define_at(&mut self.memory, a.class_id, a.offset, a.spec)?;
                self.dirty = true;
                Ok((json!(field_id), Changed::DEFS))
            }
            "renameField" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: u64,
                    name: String,
                }
                let a: A = arg(p)?;
                let r = edit::rename_field(&mut self.memory, a.class_id, a.field_id, &a.name);
                edited(self, r)
            }
            "insertBytes" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: Option<u64>,
                    count: u64,
                    #[serde(default)]
                    after: bool,
                }
                let a: A = arg(p)?;
                let r = edit::insert_bytes(
                    &mut self.memory,
                    a.class_id,
                    a.field_id,
                    a.count.min(0x10000),
                    a.after,
                );
                edited(self, r)
            }
            "removeField" => {
                let a: FieldRef = arg(p)?;
                let r = edit::remove_field(&mut self.memory, a.class_id, a.field_id);
                edited(self, r)
            }
            "setPointerTarget" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: u64,
                    target: PointerTarget,
                }
                let a: A = arg(p)?;
                let r =
                    edit::set_pointer_target(&mut self.memory, a.class_id, a.field_id, a.target);
                edited(self, r)
            }
            "setEmbeddedClass" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: u64,
                    target: u64,
                }
                let a: A = arg(p)?;
                let r =
                    edit::set_embedded_class(&mut self.memory, a.class_id, a.field_id, a.target);
                edited(self, r)
            }
            "setEnum" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: u64,
                    enum_id: u64,
                }
                let a: A = arg(p)?;
                let r = edit::set_enum(&mut self.memory, a.class_id, a.field_id, a.enum_id);
                edited(self, r)
            }
            "setArray" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    field_id: u64,
                    element: Option<PointerTarget>,
                    length: Option<u32>,
                }
                let a: A = arg(p)?;
                let r = edit::set_array(
                    &mut self.memory,
                    a.class_id,
                    a.field_id,
                    a.element,
                    a.length,
                );
                edited(self, r)
            }
            "addClass" => {
                let name = p.get("name").and_then(Value::as_str);
                let id = edit::add_class(&mut self.memory, name)?;
                self.dirty = true;
                Ok((json!(id), Changed::DEFS))
            }
            "renameClass" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    class_id: u64,
                    name: String,
                }
                let a: A = arg(p)?;
                let r = edit::rename_class(&mut self.memory, a.class_id, &a.name);
                edited(self, r)
            }
            "deleteClass" => {
                let id = p
                    .get("classId")
                    .and_then(Value::as_u64)
                    .ok_or("bad params: classId")?;
                let r = edit::delete_class(&mut self.memory, id);
                edited(self, r)
            }
            "deleteUnusedClasses" => {
                let unused: Vec<u64> = self
                    .memory
                    .class_registry
                    .get_class_ids()
                    .into_iter()
                    .filter(|id| edit::class_refs(&self.memory, *id) == 0)
                    .collect();
                for id in &unused {
                    edit::delete_class(&mut self.memory, *id)?;
                }
                self.dirty |= !unused.is_empty();
                Ok((json!(unused.len()), Changed::DEFS))
            }
            // ---- enums
            "addEnum" => {
                let name = p.get("name").and_then(Value::as_str);
                let size = p
                    .get("size")
                    .and_then(Value::as_u64)
                    .map(|s| s.min(255) as u8);
                let id = edit::add_enum(&mut self.memory, name, size)?;
                self.dirty = true;
                Ok((json!(id), Changed::DEFS))
            }
            "updateEnum" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct A {
                    enum_id: u64,
                    name: String,
                    is_flags: bool,
                    size: u8,
                    variants: Vec<(String, u32)>,
                }
                let a: A = arg(p)?;
                let r = edit::update_enum(
                    &mut self.memory,
                    a.enum_id,
                    &a.name,
                    a.is_flags,
                    a.size,
                    a.variants,
                );
                edited(self, r)
            }
            "deleteEnum" => {
                let id = p
                    .get("enumId")
                    .and_then(Value::as_u64)
                    .ok_or("bad params: enumId")?;
                let r = edit::delete_enum(&mut self.memory, id);
                edited(self, r)
            }
            // ---- signatures
            "setSignature" => {
                // Adds a signature, or replaces the one named `replace` (or with the same name).
                #[derive(Deserialize)]
                struct A {
                    signature: SignatureDef,
                    replace: Option<String>,
                }
                let a: A = arg(p)?;
                if a.signature.name.trim().is_empty() {
                    return Err("signature needs a name".into());
                }
                let old = a
                    .replace
                    .as_deref()
                    .unwrap_or(&a.signature.name)
                    .to_string();
                match self.signatures.iter_mut().find(|s| s.name == old) {
                    Some(s) => *s = a.signature,
                    None => self.signatures.push(a.signature),
                }
                self.rescan();
                self.dirty = true;
                ok(Changed::DEFS)
            }
            "removeSignature" => {
                let name = p
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("bad params: name")?;
                self.signatures.retain(|s| s.name != name);
                self.rescan();
                self.dirty = true;
                ok(Changed::DEFS)
            }
            "rescan" => {
                self.rescan();
                ok(Changed::DEFS)
            }
            // ---- canvas
            "follow" => {
                #[derive(Deserialize)]
                struct A {
                    card: u64,
                    key: String,
                }
                let a: A = arg(p)?;
                self.ensure_resolved();
                let before: Vec<u64> = self.canvas.cards.iter().map(|c| c.id).collect();
                let target = self.canvas.follow(a.card, &a.key, &self.resolved)?;
                self.dirty = true;
                let existing = target.is_some_and(|t| before.contains(&t));
                Ok((
                    json!({ "card": target, "existing": existing }),
                    Changed::NONE,
                ))
            }
            "closeCard" => {
                let id = p
                    .get("card")
                    .and_then(Value::as_u64)
                    .ok_or("bad params: card")?;
                self.ensure_resolved();
                self.canvas.close(id, &self.resolved)?;
                self.dirty = true;
                ok(Changed::NONE)
            }
            "closeAll" => {
                self.canvas.close_all();
                self.dirty = true;
                ok(Changed::NONE)
            }
            "mergeCard" => {
                let id = p
                    .get("card")
                    .and_then(Value::as_u64)
                    .ok_or("bad params: card")?;
                self.ensure_resolved();
                let into = self.canvas.merge(id, &self.resolved)?;
                self.dirty = true;
                Ok((json!(into), Changed::NONE))
            }
            "toggleExpand" => {
                #[derive(Deserialize)]
                struct A {
                    card: u64,
                    key: String,
                }
                let a: A = arg(p)?;
                self.canvas.toggle_expand(a.card, &a.key)?;
                ok(Changed::NONE)
            }
            "moveCards" => {
                #[derive(Deserialize)]
                struct Move {
                    card: u64,
                    x: f64,
                    y: f64,
                }
                let moves: Vec<Move> = arg(p.get("moves").unwrap_or(&Value::Null))?;
                for m in moves {
                    self.canvas.move_card(m.card, m.x, m.y)?;
                }
                self.dirty = true;
                ok(Changed::NONE)
            }
            "setShare" => {
                self.canvas.share = p
                    .get("on")
                    .and_then(Value::as_bool)
                    .ok_or("bad params: on")?;
                ok(Changed::SESSION)
            }
            // ---- project
            "save" => {
                if let Some(path) = p.get("path").and_then(Value::as_str) {
                    self.project_path = Some(PathBuf::from(path));
                }
                let path = self
                    .project_path
                    .clone()
                    .ok_or("no project path; pass one to save")?;
                self.project().save(&path).map_err(|e| format!("{e:#}"))?;
                self.dirty = false;
                Ok((json!(path), Changed::SESSION))
            }
            "load" => {
                let path = PathBuf::from(
                    p.get("path")
                        .and_then(Value::as_str)
                        .ok_or("bad params: path")?,
                );
                let project = ProjectFile::load(&path).map_err(|e| format!("{e:#}"))?;
                self.project_path = Some(path);
                self.load_project(project);
                ok(Changed::DEFS)
            }
            "newProject" => {
                self.project_path = None;
                self.load_project(blank_project());
                ok(Changed::DEFS)
            }
            _ => Err(format!("unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use reclass_core::demo::{
        demo_project,
        DemoProvider,
        DemoSource,
        DEMO_PID,
    };

    use super::*;

    fn demo() -> Workspace {
        let (memory, signatures) = demo_project();
        let project = ProjectFile {
            memory,
            signatures,
            web: Some(json!({ "rootExpr": "[$GWorld]" })),
        };
        let mut ws = Workspace::new(Arc::new(DemoProvider), Some(project), None, true);
        ws.attach(DEMO_PID).unwrap();
        ws
    }

    fn cards(frame: &Value) -> &Vec<Value> {
        frame["cards"].as_array().unwrap()
    }

    fn row<'a>(card: &'a Value, name: &str) -> &'a Value {
        card["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap_or_else(|| panic!("no row {name}"))
    }

    fn call(ws: &mut Workspace, method: &str, params: Value) -> Value {
        ws.handle(method, &params)
            .unwrap_or_else(|e| panic!("{method}: {e}"))
            .0
    }

    #[test]
    fn unresolved_cards_survive_and_recover() {
        for expr in ["[", "0xDEAD"] {
            let mut ws = demo();
            let f = ws.frame();
            let player = call(
                &mut ws,
                "follow",
                json!({
                    "card": ROOT, "key": row(&cards(&f)[0], "localPlayer")["key"]
                }),
            )["card"]
                .clone();
            let f = ws.frame();
            let weapon = call(
                &mut ws,
                "follow",
                json!({
                    "card": player, "key": row(&cards(&f)[1], "weapon")["key"]
                }),
            )["card"]
                .clone();
            ws.canvas
                .move_card(player.as_u64().unwrap(), 123.0, 456.0)
                .unwrap();
            ws.canvas
                .toggle_expand(player.as_u64().unwrap(), "/saved-expansion")
                .unwrap();
            let healthy = ws.frame();
            let saved = serde_json::to_value(&ws.canvas).unwrap();
            call(&mut ws, "setRoot", json!({ "expr": expr }));
            for _ in 0..2 {
                let failed = ws.frame();
                assert_eq!(
                    serde_json::to_value(&ws.canvas).unwrap(),
                    saved,
                    "resolution failure must not alter the persisted graph"
                );
                assert_eq!(cards(&failed).len(), 3);
                for card in &cards(&failed)[1..] {
                    assert!(card["base"].is_null());
                    assert!(card["error"].as_str().is_some_and(|e| !e.is_empty()));
                    assert_eq!(card["links"][0]["ok"], false);
                }
            }
            // Saving and loading while unresolved must preserve recovery too.
            let project = ws.project();
            let mut restored = Workspace::new(Arc::new(DemoProvider), Some(project), None, true);
            restored.attach(DEMO_PID).unwrap();
            assert_eq!(cards(&restored.frame()).len(), 3);
            for workspace in [&mut ws, &mut restored] {
                call(workspace, "setRoot", json!({ "expr": "[$GWorld]" }));
                let recovered = workspace.frame();
                assert_eq!(cards(&recovered), cards(&healthy));
                call(workspace, "setRoot", json!({ "expr": expr }));
                workspace.frame();
                call(workspace, "closeCard", json!({ "card": player }));
                let closed = workspace.frame();
                assert!(cards(&closed)
                    .iter()
                    .all(|c| c["id"] != player && c["id"] != weapon));
            }
        }
    }

    #[test]
    fn root_resolves_through_signature() {
        let mut ws = demo();
        let f = ws.frame();
        assert_eq!(f["rootAddress"], "1F3A8C40000");
        let root = &cards(&f)[0];
        assert_eq!(root["className"], "GameWorld");
        assert_eq!(row(root, "mapName")["value"], "\"de_hollow_ridge\"");
        assert_eq!(row(root, "localPlayer")["port"]["state"], "closed");
        assert_eq!(row(root, "camera")["port"]["encrypted"], true);
    }

    #[test]
    fn following_shares_cards_and_marks_known_ports() {
        let mut ws = demo();
        let f = ws.frame();
        let key = |f: &Value, card: usize, name: &str| {
            row(&cards(f)[card], name)["key"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let player = call(
            &mut ws,
            "follow",
            json!({ "card": ROOT, "key": key(&f, 0, "localPlayer") }),
        )["card"]
            .clone();
        let list = call(
            &mut ws,
            "follow",
            json!({ "card": ROOT, "key": key(&f, 0, "entityList") }),
        )["card"]
            .clone();
        let f = ws.frame();
        let list_card = cards(&f).iter().find(|c| c["id"] == list).unwrap();
        let entities = row(list_card, "entities")["key"]
            .as_str()
            .unwrap()
            .to_string();
        call(
            &mut ws,
            "toggleExpand",
            json!({ "card": list, "key": entities }),
        );
        let f = ws.frame();
        let list_card = cards(&f).iter().find(|c| c["id"] == list).unwrap();
        let first = row(list_card, "[0]");
        assert_eq!(first["port"]["state"], "known");
        let linked = call(
            &mut ws,
            "follow",
            json!({ "card": list, "key": first["key"] }),
        );
        assert_eq!(
            linked["card"], player,
            "entities[0] links to the existing localPlayer card"
        );
        assert_eq!(linked["existing"], true);
        let f = ws.frame();
        assert_eq!(cards(&f).len(), 3);
        let pcard = cards(&f).iter().find(|c| c["id"] == player).unwrap();
        assert_eq!(pcard["links"].as_array().unwrap().len(), 2);

        // with sharing off, a second card opens
        call(&mut ws, "setShare", json!({ "on": false }));
        let f = ws.frame();
        let list_card = cards(&f).iter().find(|c| c["id"] == list).unwrap();
        let second = row(list_card, "[1]")["key"].clone();
        call(&mut ws, "follow", json!({ "card": list, "key": second }));
        assert_eq!(cards(&ws.frame()).len(), 4);
    }

    #[test]
    fn unlinking_an_anchor_rehomes_the_card_and_churn_marks_aliases_stale() {
        let (memory, signatures) = demo_project();
        let project = ProjectFile {
            memory,
            signatures,
            web: Some(json!({ "rootExpr": "[$GWorld]" })),
        };
        let src = Arc::new(DemoSource::new());
        struct One(Arc<DemoSource>);
        impl ProcessProvider for One {
            fn list_processes(&self) -> anyhow::Result<Vec<reclass_core::source::ProcessEntry>> {
                Ok(vec![])
            }
            fn attach(&self, _: u32) -> anyhow::Result<Arc<dyn MemorySource>> {
                Ok(self.0.clone())
            }
        }
        let mut ws = Workspace::new(Arc::new(One(src.clone())), Some(project), None, true);
        ws.attach(DEMO_PID).unwrap();
        let f = ws.frame();
        let lp = row(&cards(&f)[0], "localPlayer")["key"]
            .as_str()
            .unwrap()
            .to_string();
        let player = call(&mut ws, "follow", json!({ "card": ROOT, "key": lp }))["card"].clone();
        let list = call(
            &mut ws,
            "follow",
            json!({ "card": ROOT, "key": row(&cards(&f)[0], "entityList")["key"] }),
        )["card"]
            .clone();
        let f = ws.frame();
        let lcard = cards(&f).iter().find(|c| c["id"] == list).unwrap();
        let entities = row(lcard, "entities")["key"].as_str().unwrap().to_string();
        call(
            &mut ws,
            "toggleExpand",
            json!({ "card": list, "key": entities }),
        );
        let f = ws.frame();
        let lcard = cards(&f).iter().find(|c| c["id"] == list).unwrap();
        let e0 = row(lcard, "[0]")["key"].as_str().unwrap().to_string();
        call(&mut ws, "follow", json!({ "card": list, "key": e0 }));

        // Unlink localPlayer (the anchor): the entities[0] alias takes over.
        call(&mut ws, "follow", json!({ "card": ROOT, "key": lp }));
        let f = ws.frame();
        let pcard = cards(&f)
            .iter()
            .find(|c| c["id"] == player)
            .expect("card survives");
        assert_eq!(pcard["via"], "[0]");
        assert_eq!(pcard["links"].as_array().unwrap().len(), 1);

        // Relink localPlayer (alias), then move entities[0]: the card follows its anchor...
        call(&mut ws, "follow", json!({ "card": ROOT, "key": lp }));
        src.poke(0x1F3_A8E0_0008, &0x1F3_A8D1_2600u64.to_le_bytes());
        let f = ws.frame();
        let pcard = cards(&f).iter().find(|c| c["id"] == player).unwrap();
        assert_eq!(pcard["base"], "1F3A8D12600");
        // ...and the localPlayer alias goes stale instead of moving anything.
        let alias = pcard["links"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["anchor"] == false)
            .unwrap();
        assert_eq!(alias["ok"], false);
        assert_eq!(row(&cards(&f)[0], "localPlayer")["port"]["state"], "stale");
    }

    #[test]
    fn edits_and_project_round_trip() {
        let mut ws = demo();
        let player = ws
            .memory
            .class_registry
            .get_class_ids()
            .into_iter()
            .find(|id| ws.memory.class_registry.get(*id).unwrap().name == "Player")
            .unwrap();
        let armor = ws.memory.class_registry.get(player).unwrap().fields[7].id;
        call(
            &mut ws,
            "retype",
            json!({ "classId": player, "fieldId": armor, "ty": "Float" }),
        );
        call(
            &mut ws,
            "renameField",
            json!({ "classId": player, "fieldId": armor, "name": "armor" }),
        );
        let f = ws.frame();
        let lp = row(&cards(&f)[0], "localPlayer")["key"].clone();
        call(&mut ws, "follow", json!({ "card": ROOT, "key": lp }));
        let f = ws.frame();
        assert_eq!(row(&cards(&f)[1], "armor")["value"], "50.0");
        assert!(ws
            .handle(
                "renameClass",
                &json!({ "classId": player, "name": "Weapon" })
            )
            .is_err());

        let dir = std::env::temp_dir().join(format!("reclass-ws-test-{}.json", std::process::id()));
        call(&mut ws, "save", json!({ "path": dir.to_str().unwrap() }));
        let mut ws2 = demo();
        call(&mut ws2, "load", json!({ "path": dir.to_str().unwrap() }));
        ws2.attach(DEMO_PID).unwrap();
        let f = ws2.frame();
        assert_eq!(cards(&f).len(), 2, "canvas restored");
        assert_eq!(row(&cards(&f)[1], "armor")["value"], "50.0");
        std::fs::remove_file(dir).ok();
    }

    #[test]
    fn values_are_written_to_rows_and_addresses() {
        let mut ws = demo();
        assert_eq!(ws.session()["canWrite"], true);
        let f = ws.frame();
        let key = row(&cards(&f)[0], "entityCount")["key"].clone();
        call(
            &mut ws,
            "write",
            json!({ "card": ROOT, "key": key, "value": "42" }),
        );
        call(
            &mut ws,
            "write",
            json!({ "card": ROOT, "key": row(&cards(&f)[0], "mapName")["key"], "value": "\"de_dust\"" }),
        );
        let r = call(
            &mut ws,
            "write",
            json!({ "address": "[$GWorld]+0x20", "ty": "Float", "value": "0.5" }),
        );
        assert_eq!(r["bytes"], "0000003F");
        let f = ws.frame();
        assert_eq!(row(&cards(&f)[0], "entityCount")["value"], "42");
        assert_eq!(row(&cards(&f)[0], "mapName")["value"], "\"de_dust\"");
        assert_eq!(row(&cards(&f)[0], "timeScale")["value"], "0.5");

        let err = ws.handle(
            "write",
            &json!({ "card": ROOT, "key": key, "value": "lots" }),
        );
        assert!(err.err().unwrap().contains("not an integer"));
        let err = ws.handle(
            "write",
            &json!({ "address": "0x10", "ty": "UInt8", "value": "1" }),
        );
        assert!(err.err().unwrap().contains("cannot write"));
    }

    #[test]
    fn watches_sample_freeze_and_save_with_the_project() {
        let mut ws = demo();
        let id = call(
            &mut ws,
            "watchAdd",
            json!({ "expr": "[$GWorld]+0x20", "type": "f32", "label": "time scale" }),
        );
        assert!(ws
            .handle("watchAdd", &json!({ "expr": "0x10", "type": "text" }))
            .is_err());
        assert!(ws
            .handle("watchAdd", &json!({ "expr": "[", "type": "i32" }))
            .is_err());
        ws.sample_watches();
        let view = ws.watches();
        let w = &view["watches"][0];
        assert_eq!(
            (&w["label"], &w["value"], &w["type"]),
            (&json!("time scale"), &json!("1.0"), &json!("f32"))
        );
        assert_eq!(w["history"], json!([1.0, 1.0]));

        // Frozen, it holds against the process changing it.
        call(
            &mut ws,
            "watchFreeze",
            json!({ "id": id, "on": true, "value": "0.5" }),
        );
        call(
            &mut ws,
            "write",
            json!({ "address": "[$GWorld]+0x20", "ty": "Float", "value": "3" }),
        );
        ws.sample_watches();
        assert_eq!(ws.watches()["watches"][0]["value"], "0.5");
        call(&mut ws, "watchSet", json!({ "id": id, "value": "2" }));
        assert_eq!(ws.watches()["watches"][0]["frozen"], "2.0");
        call(&mut ws, "watchFreeze", json!({ "id": id, "on": false }));
        assert_eq!(ws.watches()["watches"][0]["frozen"], Value::Null);

        call(
            &mut ws,
            "watchUpdate",
            json!({ "id": id, "expr": "[$GWorld]+0x18", "type": "i32" }),
        );
        assert_eq!(
            ws.watches()["watches"][0]["history"]
                .as_array()
                .unwrap()
                .len(),
            1,
            "a new target starts a new history"
        );
        let saved = ws.project();
        let mut other = Workspace::new(Arc::new(DemoProvider), Some(saved), None, true);
        other.attach(DEMO_PID).unwrap();
        other.sample_watches();
        let w = &other.watches()["watches"][0];
        assert_eq!(
            (&w["expr"], &w["type"]),
            (&json!("[$GWorld]+0x18"), &json!("i32"))
        );

        call(&mut other, "watchRemove", json!({ "id": w["id"] }));
        assert!(!other.has_watches());
        let pin = ws.watch_pin(id.as_u64().unwrap()).unwrap();
        assert_eq!(pin.address, 0x1F3_A8C4_0018);
    }

    #[test]
    fn drivers_without_writes_refuse_them() {
        struct ReadOnly(DemoSource);
        impl MemorySource for ReadOnly {
            fn process(&self) -> reclass_core::source::ProcessEntry {
                self.0.process()
            }
            fn modules(&self) -> Vec<ModuleEntry> {
                self.0.modules()
            }
            fn read(&self, address: u64, buffer: &mut [u8]) -> anyhow::Result<()> {
                self.0.read(address, buffer)
            }
            fn resolve_signature(&self, sig: &SignatureDef) -> anyhow::Result<u64> {
                self.0.resolve_signature(sig)
            }
            fn decrypt(&self, value: u64) -> anyhow::Result<u64> {
                self.0.decrypt(value)
            }
        }
        struct Provider;
        impl ProcessProvider for Provider {
            fn list_processes(&self) -> anyhow::Result<Vec<reclass_core::source::ProcessEntry>> {
                Ok(vec![])
            }
            fn attach(&self, _: u32) -> anyhow::Result<Arc<dyn MemorySource>> {
                Ok(Arc::new(ReadOnly(DemoSource::new())))
            }
        }
        let mut ws = Workspace::new(Arc::new(Provider), None, None, false);
        assert_eq!(ws.session()["canWrite"], false);
        ws.attach(DEMO_PID).unwrap();
        assert_eq!(ws.session()["canWrite"], false);
        let err = ws.handle(
            "write",
            &json!({ "address": "0x1F3A8C40000", "ty": "UInt8", "value": "1" }),
        );
        assert_eq!(err.err().unwrap(), "the driver cannot write memory");
        let id = call(
            &mut ws,
            "watchAdd",
            json!({ "expr": "0x1F3A8C40000", "type": "u8" }),
        );
        let err = ws.handle("watchFreeze", &json!({ "id": id, "on": true }));
        assert_eq!(err.err().unwrap(), "the driver cannot write memory");
    }

    #[test]
    fn attaching_a_32_bit_process_converts_the_project() {
        struct Pe32(Arc<DemoSource>);
        impl ProcessProvider for Pe32 {
            fn list_processes(&self) -> anyhow::Result<Vec<reclass_core::source::ProcessEntry>> {
                Ok(vec![])
            }
            fn attach(&self, _: u32) -> anyhow::Result<Arc<dyn MemorySource>> {
                Ok(self.0.clone())
            }
        }
        let src = Arc::new(DemoSource::new());
        let image = src.modules()[0].base;
        src.poke(image, b"MZ");
        src.poke(image + 0x3C, &0x80u32.to_le_bytes());
        src.poke(image + 0x80, b"PE\0\0");
        src.poke(image + 0x98, &0x10Bu16.to_le_bytes());
        src.poke(0x1F3_A8C4_0100, &0x1122_3344_5566_7788u64.to_le_bytes());

        let offsets = |ws: &Workspace| -> Vec<(String, String, u64)> {
            let defs = ws.defs();
            let mut out = Vec::new();
            for c in defs["classes"].as_array().unwrap() {
                for f in c["fields"].as_array().unwrap() {
                    if let Some(name) = f["name"].as_str() {
                        let class = c["name"].as_str().unwrap().to_string();
                        out.push((class, name.to_string(), f["offset"].as_u64().unwrap()));
                    }
                }
            }
            out
        };
        let (memory, signatures) = demo_project();
        let project = ProjectFile {
            memory,
            signatures,
            web: None,
        };
        let mut ws = Workspace::new(Arc::new(Pe32(src)), Some(project), None, true);
        let before = offsets(&ws);
        assert_eq!(ws.session()["pointerSize"], 8);

        ws.attach(DEMO_PID).unwrap();
        let session = ws.session();
        assert_eq!(session["pointerSize"], 4);
        assert_eq!(session["dirty"], true);
        assert_eq!(offsets(&ws), before, "named fields keep their offsets");
        assert_eq!(
            call(&mut ws, "eval", json!({ "expr": "[0x1F3A8C40100]" })),
            "55667788",
            "derefs read 4 bytes"
        );

        call(&mut ws, "setPointerSize", json!({ "size": 8 }));
        assert_eq!(ws.session()["pointerSize"], 8);
        assert_eq!(offsets(&ws), before);
        assert_eq!(
            call(&mut ws, "eval", json!({ "expr": "[0x1F3A8C40100]" })),
            "1122334455667788"
        );
        assert!(ws.handle("setPointerSize", &json!({ "size": 3 })).is_err());
    }

    #[test]
    fn inspect_decodes_without_touching_the_canvas() {
        let mut ws = demo();
        let canvas = serde_json::to_value(&ws.canvas).unwrap();
        let root = call(&mut ws, "inspect", json!({}));
        assert_eq!(root["className"], "GameWorld");
        assert_eq!(root["address"], "1F3A8C40000");
        let rows = root["rows"].as_array().unwrap();
        let named = |rows: &[Value], name: &str| {
            rows.iter()
                .find(|r| r["name"] == name)
                .unwrap_or_else(|| panic!("no row {name}"))
                .clone()
        };
        assert_eq!(named(rows, "mapName")["value"], "\"de_hollow_ridge\"");
        assert!(named(rows, "localPlayer").get("children").is_none());

        let followed = call(&mut ws, "inspect", json!({ "follow": 1 }));
        let player = named(followed["rows"].as_array().unwrap(), "localPlayer");
        let fields = player["children"].as_array().expect("pointer followed");
        assert!(fields.iter().all(|r| r["classId"].is_u64()));

        let player_class = fields[0]["classId"].clone();
        let at = format!("0x{}", player["value"].as_str().unwrap());
        let direct = call(
            &mut ws,
            "inspect",
            json!({ "address": at, "classId": player_class }),
        );
        assert_eq!(direct["rows"], json!(fields), "same instance, same rows");

        let span = call(
            &mut ws,
            "inspect",
            json!({ "address": "[$GWorld]", "size": 0x14 }),
        );
        let types: Vec<&str> = span["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["type"].as_str().unwrap())
            .collect();
        assert_eq!(types, ["Hex64", "Hex64", "Hex32"]);
        assert!(ws.handle("inspect", &json!({ "address": "0" })).is_err());
        assert_eq!(serde_json::to_value(&ws.canvas).unwrap(), canvas);
    }

    #[test]
    fn define_at_takes_a_target_and_returns_the_field() {
        let mut ws = demo();
        let class = call(&mut ws, "addClass", json!({ "name": "Probe" }));
        let target = call(&mut ws, "addClass", json!({ "name": "Target" }));
        let field = call(
            &mut ws,
            "defineAt",
            json!({ "classId": class, "offset": 8, "ty": "Pointer", "target": { "ClassId": target } }),
        );
        let defs = ws.defs();
        let probe = defs["classes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == class)
            .unwrap();
        let f = probe["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == field)
            .unwrap();
        assert_eq!(f["offset"], 8);
        assert_eq!(f["typeLabel"], "Target*");
    }

    #[test]
    fn enums_can_be_created_edited_and_deleted() {
        let mut ws = demo();
        let id = call(&mut ws, "addEnum", json!({ "name": "EState" }));
        call(
            &mut ws,
            "updateEnum",
            json!({ "enumId": id, "name": "EState", "isFlags": false, "size": 1, "variants": [["Idle", 0], ["Dead", 3]] }),
        );
        let defs = ws.defs();
        let e = defs["enums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap();
        assert_eq!(e["variants"], json!([["Idle", 0], ["Dead", 3]]));
        assert_eq!(e["refs"], 0);
        assert!(ws.handle("updateEnum", &json!({ "enumId": id, "name": "ETeam", "isFlags": false, "size": 4, "variants": [] })).is_err());
        call(&mut ws, "deleteEnum", json!({ "enumId": id }));
        assert!(ws.defs()["enums"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"] != id));
    }
}
