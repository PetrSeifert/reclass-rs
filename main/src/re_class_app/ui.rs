use std::{
    collections::{
        HashMap,
        HashSet,
    },
    sync::Arc,
};

use eframe::egui::{
    self,
    CentralPanel,
    Color32,
    Context,
    Layout,
    ScrollArea,
    SidePanel,
    TextEdit,
    TextStyle,
    TopBottomPanel,
    Ui,
};
use handle::AppHandle;
use rfd::FileDialog;

use super::ReClassApp;
use crate::memory::{
    ClassDefinition,
    ClassDefinitionRegistry,
    ClassInstance,
    FieldType,
    MemoryField,
    MemoryStructure,
};

pub struct ReClassGui {
    app: ReClassApp,
    attach_window_open: bool,
    process_filter: String,
    modules_window_open: bool,
    needs_rebuild: bool,
    field_name_buffers: HashMap<FieldKey, String>,
    class_type_buffers: HashMap<FieldKey, String>,
    root_class_type_buffer: Option<String>,
    root_address_buffer: Option<String>,
    cycle_error_open: bool,
    cycle_error_text: String,
    rename_dialog_open: bool,
    rename_target_name: String,
    rename_buffer: String,
    rename_error_text: Option<String>,
}

impl ReClassGui {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            app: ReClassApp::new()?,
            attach_window_open: false,
            process_filter: String::new(),
            modules_window_open: false,
            needs_rebuild: false,
            field_name_buffers: HashMap::new(),
            class_type_buffers: HashMap::new(),
            root_class_type_buffer: None,
            root_address_buffer: None,
            cycle_error_open: false,
            cycle_error_text: String::new(),
            rename_dialog_open: false,
            rename_target_name: String::new(),
            rename_buffer: String::new(),
            rename_error_text: None,
        })
    }

    fn header_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("Attach to Process").clicked() {
                self.attach_window_open = true;
                let _ = self.app.fetch_processes();
            }

            if let Some(selected) = &self.app.process_state.selected_process {
                ui.label(format!(
                    "Attached: {} (PID {})",
                    selected.get_image_base_name().unwrap_or("Unknown"),
                    selected.process_id
                ));
                if ui.button("Modules").clicked() {
                    let _ = self.app.fetch_modules(selected.process_id);
                    self.modules_window_open = true;
                }
            } else {
                ui.label("Not attached");
            }
        });
    }

    fn memory_structure_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Memory Structure");
            ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                if ui.button("Load").clicked() {
                    if let Some(path) = FileDialog::new().add_filter("JSON", &["json"]).pick_file()
                    {
                        if let Ok(text) = std::fs::read_to_string(&path) {
                            if let Ok(mut ms) = serde_json::from_str::<MemoryStructure>(&text) {
                                // After deserialization, reseed id counters to avoid collisions with future creations
                                ms.class_registry.normalize_ids();
                                ms.create_nested_instances();
                                self.app.set_memory_structure(ms);
                            }
                        }
                    }
                }
                if ui.button("Save").clicked() {
                    if let Some(ms) = self.app.get_memory_structure() {
                        if let Some(path) = FileDialog::new()
                            .set_file_name("memory_structure.json")
                            .save_file()
                        {
                            if let Ok(text) = serde_json::to_string_pretty(ms) {
                                let _ = std::fs::write(path, text);
                            }
                        }
                    }
                }
                if ui.button("New").clicked() {
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

    fn render_memory_structure_impl(
        &mut self,
        ui: &mut Ui,
        memory: &mut MemoryStructure,
        handle: Option<Arc<AppHandle>>,
    ) {
        let header = format!(
            "{} @ 0x{:X} (size {} bytes)",
            memory.root_class.class_definition.name,
            memory.root_class.address,
            memory.root_class.get_size()
        );

        let mem_ptr: *mut MemoryStructure = memory as *mut _;
        egui::CollapsingHeader::new(header)
            .default_open(false)
            .id_source("root")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Class:");
                    let mut root_class_name = self
                        .root_class_type_buffer
                        .clone()
                        .unwrap_or_else(|| memory.root_class.class_definition.name.clone());
                    let resp_name = text_edit_autowidth(ui, &mut root_class_name);
                    if resp_name.changed() {
                        self.root_class_type_buffer = Some(root_class_name.clone());
                    }
                    let enter_on_this = ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && ui.memory(|m| m.has_focus(resp_name.id));
                    if (resp_name.lost_focus() || enter_on_this)
                        && root_class_name != memory.root_class.class_definition.name
                    {
                        let old = memory.root_class.class_definition.name.clone();
                        // Only rename if target doesn't already exist to prevent stack overflow
                        if !memory.class_registry.contains(&root_class_name) {
                            memory.rename_class(&old, &root_class_name);
                            self.needs_rebuild = true;
                            self.root_class_type_buffer = None;
                        } else {
                            // Revert buffer to current model value when commit is invalid (name already exists)
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
                        if let Some(addr) = parse_hex_u64(&base_hex) {
                            memory.set_root_address(addr);
                        }
                        self.root_address_buffer = None;
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

    fn render_instance(
        &mut self,
        ui: &mut Ui,
        instance: &mut ClassInstance,
        handle: Option<Arc<AppHandle>>,
        mem_ptr: *mut MemoryStructure,
        path: &mut Vec<usize>,
    ) {
        for (idx, field) in instance.fields.iter_mut().enumerate() {
            match field.field_type {
                FieldType::ClassInstance => {
                    // Header label for the collapsible
                    let (fname_display, cname_display) =
                        if let Some(nested) = &field.nested_instance {
                            (
                                field.name.clone().unwrap_or_default(),
                                nested.class_definition.name.clone(),
                            )
                        } else {
                            (
                                field.name.clone().unwrap_or_default(),
                                "ClassInstance".to_string(),
                            )
                        };
                    let header = format!(
                        "0x{:08X}  {}: {}  [ClassInstance]",
                        field.address, fname_display, cname_display
                    );
                    // Stable id based on owning class definition field id (independent of label text)
                    let def_id = instance
                        .class_definition
                        .fields
                        .get(idx)
                        .map(|fd| fd.id)
                        .unwrap_or(0);
                    let collapsing = egui::CollapsingHeader::new(header)
                        .default_open(false)
                        .id_source(("ci_field", def_id, path.clone()))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Name:");
                                let def_id = instance
                                    .class_definition
                                    .fields
                                    .get(idx)
                                    .map(|fd| fd.id)
                                    .unwrap_or(0);
                                let key = FieldKey {
                                    instance_address: instance.address,
                                    field_def_id: def_id,
                                };
                                let mut fname = self
                                    .field_name_buffers
                                    .get(&key)
                                    .cloned()
                                    .unwrap_or_else(|| field.name.clone().unwrap_or_default());
                                let resp = text_edit_autowidth(ui, &mut fname);
                                if resp.changed() {
                                    self.field_name_buffers.insert(key, fname.clone());
                                }
                                let enter_on_this = ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    && ui.memory(|m| m.has_focus(resp.id));
                                if resp.lost_focus() || enter_on_this {
                                    field.name = Some(fname.clone());
                                    if let Some(nested) = field.nested_instance.as_mut() {
                                        nested.name = fname.clone();
                                    }
                                    let ms = unsafe { &mut *mem_ptr };
                                    let class_name = instance.class_definition.name.clone();
                                    if let Some(def) = ms.class_registry.get_mut(&class_name) {
                                        if let Some(fd) = def.fields.get_mut(idx) {
                                            fd.name = Some(fname.clone());
                                            if let Some(nested) = field.nested_instance.as_mut() {
                                                nested.name = fname;
                                            }
                                        }
                                        self.schedule_rebuild();
                                    }
                                    self.field_name_buffers.remove(&key);
                                }
                                if let Some(nested) = field.nested_instance.as_mut() {
                                    ui.label("Type:");
                                    let tkey = FieldKey {
                                        instance_address: instance.address,
                                        field_def_id: def_id,
                                    };
                                    let current_type = nested.class_definition.name.clone();
                                    let available =
                                        unsafe { (*mem_ptr).class_registry.get_class_names() };
                                    let mut selected = self
                                        .class_type_buffers
                                        .get(&tkey)
                                        .cloned()
                                        .unwrap_or_else(|| current_type.clone());
                                    egui::ComboBox::from_id_source(("ci_type_combo", tkey))
                                        .selected_text(&selected)
                                        .show_ui(ui, |ui| {
                                            for name in &available {
                                                ui.selectable_value(
                                                    &mut selected,
                                                    name.clone(),
                                                    name,
                                                );
                                            }
                                        });
                                    if selected != current_type {
                                        let ms = unsafe { &mut *mem_ptr };
                                        if ms.would_create_cycle(
                                            &instance.class_definition.name,
                                            &selected,
                                        ) {
                                            self.class_type_buffers.remove(&tkey);
                                            self.cycle_error_text = format!(
                                                "Changing '{}' -> '{}' would create a class cycle.",
                                                current_type, selected
                                            );
                                            self.cycle_error_open = true;
                                        } else if !ms.class_registry.contains(&selected) {
                                            self.class_type_buffers.remove(&tkey);
                                        } else if let Some(def) = ms
                                            .class_registry
                                            .get_mut(&instance.class_definition.name)
                                        {
                                            if let Some(fd) =
                                                def.fields.iter_mut().find(|fd| fd.id == def_id)
                                            {
                                                fd.class_name = Some(selected.clone());
                                                self.schedule_rebuild();
                                                self.class_type_buffers.remove(&tkey);
                                            }
                                        }
                                    } else {
                                        self.class_type_buffers.insert(tkey, selected);
                                    }
                                }
                            });
                            if let Some(nested) = field.nested_instance.as_mut() {
                                ui.separator();
                                path.push(idx);
                                self.render_instance(ui, nested, handle.clone(), mem_ptr, path);
                                path.pop();
                            }
                        });
                    let ctx = FieldCtx {
                        mem_ptr,
                        owner_class_name: instance.class_definition.name.clone(),
                        field_index: idx,
                        address: field.address,
                        value_preview: None,
                    };
                    self.context_menu_for_field(&collapsing.header_response, ctx);
                }
                _ => {
                    let inner = ui.horizontal(|ui| {
                        ui.label(format!("0x{:08X}", field.address));
                        if let Some(name) = field.name.clone() {
                            let def_id = instance
                                .class_definition
                                .fields
                                .get(idx)
                                .map(|fd| fd.id)
                                .unwrap_or(0);
                            let key = FieldKey {
                                instance_address: instance.address,
                                field_def_id: def_id,
                            };
                            let mut fname =
                                self.field_name_buffers.get(&key).cloned().unwrap_or(name);
                            let resp = text_edit_autowidth(ui, &mut fname);
                            if resp.changed() {
                                self.field_name_buffers.insert(key, fname.clone());
                            }
                            let enter_on_this = ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && ui.memory(|m| m.has_focus(resp.id));
                            if resp.lost_focus() || enter_on_this {
                                field.name = Some(fname.clone());
                                // Propagate to definition
                                let ms = unsafe { &mut *mem_ptr };
                                let class_name = instance.class_definition.name.clone();
                                if let Some(def) = ms.class_registry.get_mut(&class_name) {
                                    if let Some(fd) = def.fields.get_mut(idx) {
                                        fd.name = Some(fname);
                                    }
                                    self.needs_rebuild = true;
                                }
                                self.field_name_buffers.remove(&key);
                            }
                            ui.label(format!(": {}", field.field_type));
                        } else {
                            ui.label(format!("{}", field.field_type));
                        }
                        ui.label(format!(" ({} bytes)", field.get_size()));
                        if let Some(val) = field_value_string(handle.clone(), field) {
                            ui.label(format!("= {}", val));
                        }
                    });
                    let ctx = FieldCtx {
                        mem_ptr,
                        owner_class_name: instance.class_definition.name.clone(),
                        field_index: idx,
                        address: field.address,
                        value_preview: field_value_string(handle.clone(), field),
                    };
                    let def_id = instance
                        .class_definition
                        .fields
                        .get(idx)
                        .map(|fd| fd.id)
                        .unwrap_or(0);
                    let id = ui.id().with(("row_field", def_id, path.clone(), idx));
                    let resp = ui.interact(inner.response.rect, id, egui::Sense::click());
                    self.context_menu_for_field(&resp, ctx);
                }
            }
        }
    }

    fn context_menu_for_field(&mut self, response: &egui::Response, ctx: FieldCtx) {
        response.context_menu(|ui| {
            if ui.button("Copy address").clicked() {
                let _ = arboard::Clipboard::new()
                    .and_then(|mut cb| cb.set_text(format!("0x{:X}", ctx.address)));
                ui.close_menu();
            }
            if let Some(val) = ctx.value_preview.clone() {
                if ui.button("Copy value").clicked() {
                    let _ = arboard::Clipboard::new().and_then(|mut cb| cb.set_text(val));
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Add Hex64 at end").clicked() {
                let ms = unsafe { &mut *ctx.mem_ptr };
                if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                    def.add_hex_field(FieldType::Hex64);
                    self.schedule_rebuild();
                }
                ui.close_menu();
            }
            if ui.button("Insert Hex64 here").clicked() {
                let ms = unsafe { &mut *ctx.mem_ptr };
                if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                    def.insert_hex_field_at(ctx.field_index, FieldType::Hex64);
                    self.schedule_rebuild();
                }
                ui.close_menu();
            }
            {
                let can_remove = unsafe {
                    (*ctx.mem_ptr)
                        .class_registry
                        .get(&ctx.owner_class_name)
                        .map(|d| d.fields.len() > 1)
                        .unwrap_or(false)
                };
                let resp = ui.add_enabled(can_remove, egui::Button::new("Remove field"));
                if resp.clicked() {
                    let ms = unsafe { &mut *ctx.mem_ptr };
                    if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                        def.remove_field_at(ctx.field_index);
                        self.schedule_rebuild();
                    }
                    ui.close_menu();
                }
            }
            ui.menu_button("Change type", |ui| {
                for t in [
                    FieldType::Hex8,
                    FieldType::Hex16,
                    FieldType::Hex32,
                    FieldType::Hex64,
                    FieldType::Int8,
                    FieldType::Int16,
                    FieldType::Int32,
                    FieldType::Int64,
                    FieldType::UInt8,
                    FieldType::UInt16,
                    FieldType::UInt32,
                    FieldType::UInt64,
                    FieldType::Bool,
                    FieldType::Float,
                    FieldType::Double,
                    FieldType::TextPointer,
                ] {
                    let label = format!("{:?}", t);
                    if ui.button(label).clicked() {
                        let ms = unsafe { &mut *ctx.mem_ptr };
                        if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                            def.set_field_type_at(ctx.field_index, t.clone());
                            self.schedule_rebuild();
                        }
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            if ui.button("Create class from field").clicked() {
                let ms = unsafe { &mut *ctx.mem_ptr };
                let base_name = "NewClass";
                {
                    let unique_name = generate_unique_class_name(&ms.class_registry, base_name);
                    let mut new_def = ClassDefinition::new(unique_name.clone());
                    new_def.add_hex_field(FieldType::Hex64);
                    ms.class_registry.register(new_def.clone());
                    if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                        def.set_field_type_at(ctx.field_index, FieldType::ClassInstance);
                        if let Some(fd) = def.fields.get_mut(ctx.field_index) {
                            fd.class_name = Some(unique_name);
                        }
                        self.schedule_rebuild();
                    }
                }
                ui.close_menu();
            }
        });
    }
}

// Helper to clone ProcessInfo when available; returns None if cloning isn't supported at compile time
#[inline]
fn maybe_clone_process<T: Clone>(p: &T) -> Option<T> {
    Some(p.clone())
}

impl ReClassGui {
    fn attach_window(&mut self, ctx: &Context) {
        let mut clicked_pid: Option<u32> = None;
        egui::Window::new("Attach to Process")
            .open(&mut self.attach_window_open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut self.process_filter);
                    if ui.button("Clear").clicked() {
                        self.process_filter.clear();
                    }
                    if ui.button("Refresh").clicked() {
                        let _ = self.app.fetch_processes();
                    }
                });
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    for process in self.app.get_processes() {
                        let name = process.get_image_base_name().unwrap_or("Unknown");
                        if !self.process_filter.is_empty()
                            && !name
                                .to_lowercase()
                                .contains(&self.process_filter.to_lowercase())
                        {
                            continue;
                        }

                        ui.horizontal(|ui| {
                            ui.label(format!("{} (PID {})", name, process.process_id));
                            if ui.button("Attach").clicked() {
                                clicked_pid = Some(process.process_id);
                            }
                        });
                    }
                });
            });

        if let Some(pid) = clicked_pid {
            if let Some(proc_info) = self.app.get_process_by_id(pid) {
                if let Some(cloned) = maybe_clone_process(proc_info) {
                    self.app.select_process(cloned);
                }
            }
            let _ = self.app.create_handle(pid);
            let _ = self.app.fetch_modules(pid);
            self.attach_window_open = false;
        }
    }
}

impl ReClassGui {
    fn modules_window(&mut self, ctx: &Context) {
        egui::Window::new("Modules")
            .open(&mut self.modules_window_open)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(proc_) = &self.app.process_state.selected_process {
                    if ui.button("Refresh").clicked() {
                        let _ = self.app.fetch_modules(proc_.process_id);
                    }
                    ui.separator();
                    ScrollArea::vertical().show(ui, |ui| {
                        for m in self.app.get_modules() {
                            let name = m.get_base_dll_name().unwrap_or("Unknown");
                            ui.label(format!(
                                "{} @ 0x{:X} ({} KB)",
                                name,
                                m.base_address,
                                m.module_size / 1024
                            ));
                        }
                    });
                } else {
                    ui.label("Not attached to a process");
                }
            });
    }
}

fn field_value_string(handle: Option<Arc<AppHandle>>, field: &MemoryField) -> Option<String> {
    let handle = handle.as_ref()?;
    let addr = field.address;
    match field.field_type {
        FieldType::Hex64 => handle
            .read_sized::<u64>(addr)
            .ok()
            .map(|v| format!("0x{v:016X}")),
        FieldType::Hex32 => handle
            .read_sized::<u32>(addr)
            .ok()
            .map(|v| format!("0x{v:08X}")),
        FieldType::Hex16 => handle
            .read_sized::<u16>(addr)
            .ok()
            .map(|v| format!("0x{v:04X}")),
        FieldType::Hex8 => handle
            .read_sized::<u8>(addr)
            .ok()
            .map(|v| format!("0x{v:02X}")),

        FieldType::UInt64 => handle.read_sized::<u64>(addr).ok().map(|v| v.to_string()),
        FieldType::UInt32 => handle.read_sized::<u32>(addr).ok().map(|v| v.to_string()),
        FieldType::UInt16 => handle.read_sized::<u16>(addr).ok().map(|v| v.to_string()),
        FieldType::UInt8 => handle.read_sized::<u8>(addr).ok().map(|v| v.to_string()),

        FieldType::Int64 => handle.read_sized::<i64>(addr).ok().map(|v| v.to_string()),
        FieldType::Int32 => handle.read_sized::<i32>(addr).ok().map(|v| v.to_string()),
        FieldType::Int16 => handle.read_sized::<i16>(addr).ok().map(|v| v.to_string()),
        FieldType::Int8 => handle.read_sized::<i8>(addr).ok().map(|v| v.to_string()),

        FieldType::Bool => handle.read_sized::<u8>(addr).ok().map(|v| {
            if v != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }),
        FieldType::Float => handle.read_sized::<f32>(addr).ok().map(|v| format!("{v}")),
        FieldType::Double => handle.read_sized::<f64>(addr).ok().map(|v| format!("{v}")),

        FieldType::Vector3 | FieldType::Vector4 | FieldType::Vector2 => {
            // Fallback: show raw bytes in hex for now
            let len = field.get_size() as usize;
            let mut buf = vec![0u8; len];
            (handle.read_slice(addr, buf.as_mut_slice()).ok()).map(|_| {
                buf.iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        }

        FieldType::Text => handle.read_string(addr, Some(32)).ok(),
        FieldType::TextPointer => {
            if let Ok(ptr) = handle.read_sized::<u64>(addr) {
                if ptr != 0 {
                    handle.read_string(ptr, None).ok()
                } else {
                    Some(String::from("(null)"))
                }
            } else {
                None
            }
        }

        FieldType::ClassInstance => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FieldKey {
    instance_address: u64,
    field_def_id: u64,
}

struct FieldCtx {
    mem_ptr: *mut MemoryStructure,
    owner_class_name: String,
    field_index: usize,
    address: u64,
    value_preview: Option<String>,
}

fn generate_unique_class_name(registry: &ClassDefinitionRegistry, base: &str) -> String {
    if !registry.contains(base) {
        return base.to_string();
    }
    let mut counter: usize = 1;
    loop {
        let candidate = format!("{}_{}", base, counter);
        if !registry.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

impl ReClassGui {
    fn schedule_rebuild(&mut self) {
        self.needs_rebuild = true;
    }
}

impl eframe::App for ReClassGui {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        TopBottomPanel::top("top").show(ctx, |ui| {
            self.header_bar(ui);
        });

        SidePanel::left("class_defs_panel").resizable(true).default_width(220.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Class Definitions");
            });
            ui.separator();
            // Snapshot: names, referenced set across all definitions, root name, unused (default-only and unreferenced)
            let snapshot = self.app.get_memory_structure().map(|ms| {
                let names = ms.class_registry.get_class_names();
                let root_name = ms.root_class.class_definition.name.clone();
                let mut referenced: HashSet<String> = HashSet::new();
                for cname in &names {
                    if let Some(def) = ms.class_registry.get(cname) {
                        for f in &def.fields {
                            if f.field_type == FieldType::ClassInstance {
                                if let Some(ref cn) = f.class_name { referenced.insert(cn.clone()); }
                            }
                        }
                    }
                }
                // Unused = not referenced anywhere AND has only the default field (single Hex64)
                let unused: Vec<String> = names
                    .iter()
                    .filter(|n| {
                        if *n == &root_name { return false; }
                        if referenced.contains(*n) { return false; }
                        if let Some(def) = ms.class_registry.get(n) {
                            if def.fields.len() == 1 {
                                let f = &def.fields[0];
                                return f.field_type == FieldType::Hex64 && f.name.is_none();
                            }
                        }
                        false
                    })
                    .cloned()
                    .collect();
                (names, root_name, referenced, unused)
            });

            if let Some((names, root_name, _referenced, unused)) = snapshot {
                if ui.add_enabled(!unused.is_empty(), egui::Button::new("Delete unused"))
                    .on_hover_text("Delete class definitions that have only the default field and are not referenced anywhere (excluding current root)")
                    .clicked()
                {
                    if let Some(ms_mut) = self.app.get_memory_structure_mut() {
                        for cname in &unused { ms_mut.class_registry.remove(cname); }
                        self.needs_rebuild = true;
                    }
                }
                ui.separator();
                ScrollArea::vertical().id_source("class_defs_scroll").show(ui, |ui| {
                    let active = root_name.clone();
                    for cname in names {
                        let mut button = egui::Button::new(&cname);
                        if active == cname {
                            button = button.fill(egui::Color32::from_rgb(40, 80, 160));
                        }
                        let resp = ui.add(button);
                        if resp.double_clicked() {
                            if let Some(ms_mut) = self.app.get_memory_structure_mut() {
                                if ms_mut.set_root_class_by_name(&cname) {
                                    self.needs_rebuild = true;
                                }
                            }
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                self.rename_dialog_open = true;
                                self.rename_target_name = cname.clone();
                                self.rename_buffer = cname.clone();
                                self.rename_error_text = None;
                                ui.close_menu();
                            }
                        });
                    }
                });
            } else {
                ui.label("No structure loaded");
            }
        });

        CentralPanel::default().show(ctx, |ui| {
            self.memory_structure_panel(ui);
        });

        // Error dialog for cycle prevention
        if self.cycle_error_open {
            let msg = self.cycle_error_text.clone();
            let mut should_close = false;
            egui::Window::new("Invalid Operation")
                .open(&mut self.cycle_error_open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(msg);
                    if ui.button("OK").clicked() {
                        should_close = true;
                    }
                });
            if should_close {
                self.cycle_error_open = false;
            }
        }

        // Rename class definition dialog
        if self.rename_dialog_open {
            let error_text = self.rename_error_text.clone();
            let mut should_close = false;
            egui::Window::new("Rename Class Definition")
                .open(&mut self.rename_dialog_open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(format!("Current: {}", self.rename_target_name));
                    let resp = ui.text_edit_singleline(&mut self.rename_buffer);
                    if let Some(err) = &error_text {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.rename_buffer.clear();
                            self.rename_error_text = None;
                            should_close = true;
                        }
                        if ui.button("OK").clicked()
                            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            let new_name = self.rename_buffer.trim().to_string();
                            if new_name.is_empty() || new_name == self.rename_target_name {
                                should_close = true;
                            } else if let Some(ms) = self.app.get_memory_structure_mut() {
                                if ms.class_registry.contains(&new_name) {
                                    self.rename_error_text =
                                        Some("A class with this name already exists.".to_string());
                                } else {
                                    let ok = ms.rename_class(&self.rename_target_name, &new_name);
                                    if ok {
                                        self.needs_rebuild = true;
                                        should_close = true;
                                        self.rename_error_text = None;
                                    } else {
                                        self.rename_error_text = Some("Rename failed.".to_string());
                                    }
                                }
                            }
                        }
                    });
                });
            if should_close {
                self.rename_dialog_open = false;
            }
        }

        // Apply deferred rebuilds after UI frame to avoid mid-frame tree resets
        if self.needs_rebuild {
            if let Some(ms) = self.app.get_memory_structure_mut() {
                ms.rebuild_root_from_registry();
                ms.create_nested_instances();
            }
            self.needs_rebuild = false;
        }

        if self.attach_window_open {
            self.attach_window(ctx);
        }
        if self.modules_window_open {
            self.modules_window(ctx);
        }
    }
}

fn parse_hex_u64(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(stripped) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).ok()
    } else {
        t.parse::<u64>().ok()
    }
}

fn text_edit_autowidth(ui: &mut Ui, text: &mut String) -> egui::Response {
    let display = if text.is_empty() {
        " ".to_string()
    } else {
        text.clone()
    };
    let galley =
        ui.painter()
            .layout_no_wrap(display, TextStyle::Body.resolve(ui.style()), Color32::WHITE);
    let width = galley.rect.width() + 12.0; // padding
    ui.add_sized(
        [width, ui.text_style_height(&TextStyle::Body)],
        TextEdit::singleline(text),
    )
}
