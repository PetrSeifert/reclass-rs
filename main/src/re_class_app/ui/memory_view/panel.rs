use std::sync::Arc;

use eframe::egui::{
    self,
    Layout,
    ScrollArea,
    Ui,
};
use handle::AppHandle;

use super::util::{
    parse_hex_u64,
    text_edit_autowidth,
};
use crate::{
    memory::{
        ClassDefinition,
        FieldType,
        MemoryStructure,
    },
    re_class_app::ReClassGui,
};

impl ReClassGui {
    fn eval_address_expr(&self, input: &str) -> Option<u64> {
        // Simple recursive-descent parser supporting:
        // numbers (hex 0x.. or decimal), <module.dll>, $SignatureName, +, -, parentheses (), deref [expr]
        struct Parser<'a> {
            s: &'a [u8],
            i: usize,
            gui: &'a ReClassGui,
        }
        impl<'a> Parser<'a> {
            fn new(gui: &'a ReClassGui, s: &'a str) -> Self {
                Self {
                    s: s.as_bytes(),
                    i: 0,
                    gui,
                }
            }
            fn eof(&self) -> bool {
                self.i >= self.s.len()
            }
            fn peek(&self) -> Option<u8> {
                self.s.get(self.i).copied()
            }
            fn bump(&mut self) {
                self.i += 1;
            }
            fn skip_ws(&mut self) {
                while let Some(b) = self.peek() {
                    if b.is_ascii_whitespace() {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            fn consume(&mut self, ch: u8) -> bool {
                self.skip_ws();
                if self.peek() == Some(ch) {
                    self.bump();
                    true
                } else {
                    false
                }
            }

            fn parse_ident(&mut self) -> Option<&'a str> {
                self.skip_ws();
                let start = self.i;
                while let Some(b) = self.peek() {
                    let c = b as char;
                    if c.is_ascii_alphanumeric() || c == '_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if self.i > start {
                    std::str::from_utf8(&self.s[start..self.i]).ok()
                } else {
                    None
                }
            }

            fn parse_signature_ref(&mut self) -> Option<u64> {
                self.skip_ws();
                if !self.consume(b'$') {
                    return None;
                }
                let name = self.parse_ident()?;
                self.gui.app.resolve_signature_by_name(name)
            }

            fn parse_number(&mut self) -> Option<u64> {
                self.skip_ws();
                let start = self.i;
                if self.peek() == Some(b'0')
                    && self
                        .s
                        .get(self.i + 1)
                        .copied()
                        .map(|c| c == b'x' || c == b'X')
                        .unwrap_or(false)
                {
                    self.i += 2;
                    let hex_start = self.i;
                    while let Some(b) = self.peek() {
                        if (b as char).is_ascii_hexdigit() {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    if self.i == hex_start {
                        return None;
                    }
                    let txt = std::str::from_utf8(&self.s[hex_start..self.i]).ok()?;
                    return u64::from_str_radix(txt, 16).ok();
                }
                while let Some(b) = self.peek() {
                    if (b as char).is_ascii_digit() {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if self.i == start {
                    return None;
                }
                let txt = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                txt.parse::<u64>().ok()
            }

            fn parse_module_ref(&mut self) -> Option<u64> {
                self.skip_ws();
                if !self.consume(b'<') {
                    return None;
                }
                let start = self.i;
                while let Some(b) = self.peek() {
                    if b != b'>' {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if !self.consume(b'>') {
                    return None;
                }
                let name = std::str::from_utf8(&self.s[start.saturating_sub(0)..self.i - 1])
                    .ok()?
                    .trim();
                // lookup module by base name case-insensitive
                let lower = name.to_ascii_lowercase();
                let modules = self.gui.app.get_modules();
                for m in modules {
                    let base = m.base_address;
                    let mname = m.get_base_dll_name().unwrap_or("");
                    if mname.to_ascii_lowercase() == lower {
                        return Some(base);
                    }
                }
                None
            }

            fn parse_factor(&mut self) -> Option<u64> {
                self.skip_ws();
                // Parentheses
                if self.consume(b'(') {
                    let v = self.parse_expr()?;
                    if !self.consume(b')') {
                        return None;
                    }
                    return Some(v);
                }
                // Deref
                if self.consume(b'[') {
                    let addr = self.parse_expr()?;
                    if !self.consume(b']') {
                        return None;
                    }
                    // read pointer-sized value at addr
                    let handle = self.gui.app.handle.as_ref()?;
                    let v = handle.read_sized::<u64>(addr).ok()?;
                    return Some(v);
                }
                // Module ref
                if let Some(v) = self.parse_module_ref() {
                    return Some(v);
                }
                // Signature ref
                if let Some(v) = self.parse_signature_ref() {
                    return Some(v);
                }
                // Number
                self.parse_number()
            }

            fn parse_term(&mut self) -> Option<u64> {
                self.parse_factor()
            }

            fn parse_expr(&mut self) -> Option<u64> {
                let mut acc = self.parse_term()?;
                loop {
                    self.skip_ws();
                    if self.consume(b'+') {
                        let rhs = self.parse_term()?;
                        acc = acc.wrapping_add(rhs);
                    } else if self.consume(b'-') {
                        let rhs = self.parse_term()?;
                        acc = acc.wrapping_sub(rhs);
                    } else {
                        break;
                    }
                }
                Some(acc)
            }
        }
        let mut p = Parser::new(self, input);
        let v = p.parse_expr()?;
        p.skip_ws();
        if p.eof() {
            Some(v)
        } else {
            None
        }
    }
    pub(crate) fn memory_structure_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Memory Structure");
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Load")
                    .on_hover_text("Load a `memory_structure.json` file")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .pick_file()
                    {
                        if let Ok(text) = std::fs::read_to_string(&path) {
                            // Expect a wrapper with memory and signatures
                            #[derive(serde::Deserialize)]
                            struct AppSave {
                                memory: MemoryStructure,
                                #[serde(default)]
                                signatures: Vec<crate::re_class_app::app::AppSignature>,
                            }
                            if let Ok(mut wrapper) = serde_json::from_str::<AppSave>(&text) {
                                wrapper.memory.class_registry.normalize_ids();
                                wrapper.memory.enum_registry.normalize_ids();
                                wrapper.memory.create_nested_instances();
                                self.app.set_memory_structure(wrapper.memory);
                                self.app.signatures = wrapper.signatures;
                            }
                        }
                    }
                }
                if ui
                    .button("Save")
                    .on_hover_text("Save current memory structure to JSON")
                    .clicked()
                {
                    if let Some(ms) = self.app.get_memory_structure() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("memory_structure.json")
                            .save_file()
                        {
                            #[derive(serde::Serialize)]
                            struct AppSave<'a> {
                                memory: &'a MemoryStructure,
                                signatures: &'a Vec<crate::re_class_app::app::AppSignature>,
                            }
                            let wrapper = AppSave {
                                memory: ms,
                                signatures: &self.app.signatures,
                            };
                            if let Ok(text) = serde_json::to_string_pretty(&wrapper) {
                                let _ = std::fs::write(path, text);
                            }
                        }
                    }
                }
                if ui
                    .button("New")
                    .on_hover_text("Create a fresh root class with a Hex64 field")
                    .clicked()
                {
                    let mut root_def = ClassDefinition::new("Root".to_string());
                    root_def.add_hex_field(FieldType::Hex64);
                    let ms = crate::memory::MemoryStructure::new("root".to_string(), 0, root_def);
                    self.app.set_memory_structure(ms);
                }
            });
        });
        ui.separator();

        let handle_arc = self.app.handle.clone();
        if let Some(ms) = self.app.get_memory_structure_mut() {
            let mut_mem_ptr: *mut MemoryStructure = ms as *mut _;
            let ms_mut: &mut MemoryStructure = unsafe { &mut *mut_mem_ptr };
            self.render_memory_structure_impl(ui, ms_mut, handle_arc);
        }
    }

    pub(super) fn render_memory_structure_impl(
        &mut self,
        ui: &mut Ui,
        memory: &mut MemoryStructure,
        handle: Option<Arc<AppHandle>>,
    ) {
        let header = {
            let cname = memory
                .class_registry
                .get(memory.root_class.class_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("#{}", memory.root_class.class_id));
            format!(
                "{} @ 0x{:X} (size {} bytes)",
                cname,
                memory.root_class.address,
                memory.root_class.get_size()
            )
        };

        let mem_ptr: *mut MemoryStructure = memory as *mut _;
        egui::CollapsingHeader::new(header)
            .default_open(false)
            .id_source("root")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Class:");
                    let mut root_class_name =
                        self.root_class_type_buffer.clone().unwrap_or_else(|| {
                            memory
                                .class_registry
                                .get(memory.root_class.class_id)
                                .map(|d| d.name.clone())
                                .unwrap_or_default()
                        });
                    let resp_name = text_edit_autowidth(ui, &mut root_class_name);
                    if resp_name.changed() {
                        self.root_class_type_buffer = Some(root_class_name.clone());
                    }
                    let enter_on_this = ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && ui.memory(|m| m.has_focus(resp_name.id));
                    if (resp_name.lost_focus() || enter_on_this)
                        && memory
                            .class_registry
                            .get(memory.root_class.class_id)
                            .map(|d| d.name.as_str() != root_class_name)
                            .unwrap_or(false)
                    {
                        if !memory.class_registry.contains_name(&root_class_name) {
                            memory.rename_class(memory.root_class.class_id, &root_class_name);
                            self.needs_rebuild = true;
                            self.root_class_type_buffer = None;
                        } else {
                            self.root_class_type_buffer = None;
                        }
                    }
                    ui.label("@");
                    let mut base_hex = self
                        .root_address_buffer
                        .clone()
                        .unwrap_or_else(|| format!("0x{:X}", memory.root_class.address));
                    let resp = text_edit_autowidth(ui, &mut base_hex);
                    if resp.changed() {
                        self.root_address_buffer = Some(base_hex.clone());
                    }
                    let enter_on_this = ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && ui.memory(|m| m.has_focus(resp.id));
                    if resp.lost_focus() || enter_on_this {
                        // Support expressions: arithmetic, <module>, deref []
                        let parsed = self
                            .eval_address_expr(&base_hex)
                            .or_else(|| parse_hex_u64(&base_hex));
                        if let Some(addr) = parsed {
                            memory.set_root_address(addr);
                        }
                    }
                });

                ui.separator();
                ScrollArea::vertical()
                    .id_source("memory_tree_scroll")
                    .show(ui, |ui| {
                        let path: &mut Vec<usize> = &mut Vec::new();
                        self.render_instance(
                            ui,
                            &mut memory.root_class,
                            handle.clone(),
                            mem_ptr,
                            path,
                        );
                    });
            });
    }
}
