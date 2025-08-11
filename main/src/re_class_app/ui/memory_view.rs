use std::sync::Arc;

use eframe::egui::{
    self,
    Color32,
    Layout,
    RichText,
    ScrollArea,
    TextEdit,
    TextStyle,
    Ui,
};
use handle::AppHandle;

use super::ReClassGui;
use crate::memory::{
    ClassDefinition,
    ClassDefinitionRegistry,
    ClassInstance,
    FieldType,
    MemoryField,
    MemoryStructure,
    PointerTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldKey {
    pub instance_address: u64,
    pub field_def_id: u64,
}

struct FieldCtx {
    mem_ptr: *mut MemoryStructure,
    owner_class_name: String,
    field_index: usize,
    address: u64,
    value_preview: Option<String>,
}

impl ReClassGui {
    pub(super) fn memory_structure_panel(&mut self, ui: &mut Ui) {
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
                            if let Ok(mut ms) = serde_json::from_str::<MemoryStructure>(&text) {
                                ms.class_registry.normalize_ids();
                                ms.create_nested_instances();
                                self.app.set_memory_structure(ms);
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
                            if let Ok(text) = serde_json::to_string_pretty(ms) {
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
                        if !memory.class_registry.contains(&root_class_name) {
                            memory.rename_class(&old, &root_class_name);
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
                FieldType::Pointer => {
                    let def_id = instance
                        .class_definition
                        .fields
                        .get(idx)
                        .map(|fd| fd.id)
                        .unwrap_or(0);
                    // If pointer targets a class -> collapsible with nested
                    if matches!(field.pointer_target, Some(PointerTarget::ClassName(_))) {
                        let offset_from_class = field.address.saturating_sub(instance.address);
                        let mut header = format!(
                            "+0x{:04X}  0x{:08X}    {}: Pointer",
                            offset_from_class,
                            field.address,
                            field.name.clone().unwrap_or_default()
                        );
                        if let Some(PointerTarget::ClassName(cn)) = &field.pointer_target {
                            header.push_str(&format!(" -> {cn}"));
                        }
                        if let Some(h) = &handle {
                            if let Ok(ptr) = h.read_sized::<u64>(field.address) {
                                header.push_str(&format!(" (-> 0x{ptr:016X})"));
                                if ptr != 0 {
                                    if let Some(PointerTarget::ClassName(cn)) =
                                        &field.pointer_target
                                    {
                                        let ms = unsafe { &mut *mem_ptr };
                                        if let Some(class_def) = ms.class_registry.get(cn).cloned()
                                        {
                                            let nested = ClassInstance::new(
                                                field.name.clone().unwrap_or_default(),
                                                ptr,
                                                class_def,
                                            );
                                            field.nested_instance = Some(nested);
                                        } else {
                                            field.nested_instance = None;
                                        }
                                    } else {
                                        field.nested_instance = None;
                                    }
                                } else {
                                    field.nested_instance = None;
                                }
                            }
                        }
                        let collapsing = egui::CollapsingHeader::new(header)
                            .default_open(false)
                            .id_source(("ptr_field", def_id, path.clone()))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Name:");
                                    let key = FieldKey {
                                        instance_address: instance.address,
                                        field_def_id: def_id,
                                    };
                                    let mut fname =
                                        self.field_name_buffers.get(&key).cloned().unwrap_or_else(
                                            || field.name.clone().unwrap_or_default(),
                                        );
                                    let resp = text_edit_autowidth(ui, &mut fname);
                                    if resp.changed() {
                                        self.field_name_buffers.insert(key, fname.clone());
                                    }
                                    let enter_on_this = ui
                                        .input(|i| i.key_pressed(egui::Key::Enter))
                                        && ui.memory(|m| m.has_focus(resp.id));
                                    if resp.lost_focus() || enter_on_this {
                                        field.name = Some(fname.clone());
                                        let ms = unsafe { &mut *mem_ptr };
                                        let class_name = instance.class_definition.name.clone();
                                        if let Some(def) = ms.class_registry.get_mut(&class_name) {
                                            if let Some(fd) = def.fields.get_mut(idx) {
                                                fd.name = Some(fname);
                                            }
                                            self.schedule_rebuild();
                                        }
                                        self.field_name_buffers.remove(&key);
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
                    } else {
                        // Pointer to primitive -> render simple row (non-collapsible)
                        let inner = ui.horizontal(|ui| {
                            let offset_from_class = field.address.saturating_sub(instance.address);
                            ui.monospace(format!(
                                "+0x{:04X}  0x{:08X}",
                                offset_from_class, field.address
                            ));
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
                                let type_label = match &field.pointer_target {
                                    Some(PointerTarget::FieldType(t)) => {
                                        format!(": {} -> {}", field.field_type, t)
                                    }
                                    Some(PointerTarget::ClassName(cn)) => {
                                        format!(": {} -> {}", field.field_type, cn)
                                    }
                                    Some(PointerTarget::EnumName(en)) => {
                                        format!(": {} -> {}", field.field_type, en)
                                    }
                                    None => format!(": {}", field.field_type),
                                };
                                // Inline enum info if applicable
                                let enum_suffix = if field.field_type == FieldType::Enum {
                                    if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                        if let Some(fd) = instance
                                            .class_definition
                                            .fields
                                            .iter()
                                            .find(|fdef| fdef.id == field.def_id)
                                        {
                                            if let Some(ref en) = fd.enum_name {
                                                if let Some(ed) = ms.enum_registry.get(en) {
                                                    let ty = match ed.default_size {
                                                        1 => "u8",
                                                        2 => "u16",
                                                        8 => "u64",
                                                        _ => "u32",
                                                    };
                                                    format!(
                                                        " -> {} ({} , {} bytes)",
                                                        en, ty, ed.default_size
                                                    )
                                                } else {
                                                    String::from(" -> <enum?>")
                                                }
                                            } else {
                                                String::from(" -> <enum?>")
                                            }
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    }
                                } else {
                                    String::new()
                                };
                                ui.colored_label(
                                    Color32::from_rgb(170, 190, 255),
                                    format!("{type_label}{enum_suffix}"),
                                );
                            } else {
                                let type_label = match &field.pointer_target {
                                    Some(PointerTarget::FieldType(t)) => {
                                        format!("{} -> {}", field.field_type, t)
                                    }
                                    Some(PointerTarget::ClassName(cn)) => {
                                        format!("{} -> {}", field.field_type, cn)
                                    }
                                    Some(PointerTarget::EnumName(en)) => {
                                        format!("{} -> {}", field.field_type, en)
                                    }
                                    None => format!("{}", field.field_type),
                                };
                                // Inline enum info if applicable
                                let enum_suffix = if field.field_type == FieldType::Enum {
                                    if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                        if let Some(fd) = instance
                                            .class_definition
                                            .fields
                                            .iter()
                                            .find(|fdef| fdef.id == field.def_id)
                                        {
                                            if let Some(ref en) = fd.enum_name {
                                                if let Some(ed) = ms.enum_registry.get(en) {
                                                    let ty = match ed.default_size {
                                                        1 => "u8",
                                                        2 => "u16",
                                                        8 => "u64",
                                                        _ => "u32",
                                                    };
                                                    format!(
                                                        " -> {} ({} , {} bytes)",
                                                        en, ty, ed.default_size
                                                    )
                                                } else {
                                                    String::from(" -> <enum?>")
                                                }
                                            } else {
                                                String::from(" -> <enum?>")
                                            }
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    }
                                } else {
                                    String::new()
                                };
                                ui.colored_label(
                                    Color32::from_rgb(170, 190, 255),
                                    format!("{type_label}{enum_suffix}"),
                                );
                            }
                            ui.label(
                                RichText::new(format!(" ({} bytes)", field.get_size())).weak(),
                            );
                            if let Some(val) = field_value_string(handle.clone(), field) {
                                ui.monospace(format!("= {val}"));
                            }
                        });
                        let row_bg = if idx % 2 == 0 {
                            Color32::from_black_alpha(12)
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(
                            inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                            4.0,
                            row_bg,
                        );
                        let ctx = FieldCtx {
                            mem_ptr,
                            owner_class_name: instance.class_definition.name.clone(),
                            field_index: idx,
                            address: field.address,
                            value_preview: field_value_string(handle.clone(), field),
                        };
                        let id = ui.id().with(("row_ptr_prim", def_id, path.clone(), idx));
                        let resp = ui.interact(inner.response.rect, id, egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_stroke(
                                inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                                4.0,
                                egui::Stroke::new(1.0, Color32::from_white_alpha(12)),
                            );
                        }
                        self.context_menu_for_field(&resp, ctx);
                    }
                }
                FieldType::ClassInstance => {
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
                        "0x{:08X}    {}: {}    [ClassInstance]",
                        field.address, fname_display, cname_display
                    );
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
                                                "Changing '{current_type}' -> '{selected}' would create a class cycle."
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
                        let offset_from_class = field.address.saturating_sub(instance.address);
                        ui.monospace(format!(
                            "+0x{:04X}  0x{:08X}",
                            offset_from_class, field.address
                        ));
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
                            // Unified enum suffix for name-present case
                            let enum_suffix = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                enum_suffix_for_field(&instance.class_definition, field, ms)
                            } else {
                                String::new()
                            };
                            ui.colored_label(
                                Color32::from_rgb(170, 190, 255),
                                format!(": {}{}", field.field_type, enum_suffix),
                            );
                        } else {
                            let enum_suffix = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                enum_suffix_for_field(&instance.class_definition, field, ms)
                            } else {
                                String::new()
                            };
                            ui.colored_label(
                                Color32::from_rgb(170, 190, 255),
                                format!("{}{}", field.field_type, enum_suffix),
                            );
                        }
                        // Show size: for enums, use enum default_size instead of FieldType::get_size
                        let display_size: u64 = if field.field_type == FieldType::Enum {
                            if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                if let Some(fd) = instance
                                    .class_definition
                                    .fields
                                    .iter()
                                    .find(|fdef| fdef.id == field.def_id)
                                {
                                    if let Some(ref en) = fd.enum_name {
                                        if let Some(ed) = ms.enum_registry.get(en) {
                                            ed.default_size as u64
                                        } else {
                                            field.get_size()
                                        }
                                    } else {
                                        field.get_size()
                                    }
                                } else {
                                    field.get_size()
                                }
                            } else {
                                field.get_size()
                            }
                        } else {
                            field.get_size()
                        };
                        ui.label(RichText::new(format!(" ({display_size} bytes)")).weak());
                        let value_str = if field.field_type == FieldType::Enum {
                            if let (Some(h), Some(ms)) =
                                (handle.as_ref(), unsafe { (mem_ptr).as_ref() })
                            {
                                // Avoid borrowing entire instance; pass what we need
                                enum_value_string(h, &instance.class_definition, field, ms)
                            } else {
                                None
                            }
                        } else {
                            field_value_string(handle.clone(), field)
                        };
                        if let Some(val) = value_str {
                            ui.monospace(format!("= {val}"));
                        }
                    });
                    let row_bg = if idx % 2 == 0 {
                        Color32::from_black_alpha(12)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(
                        inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                        4.0,
                        row_bg,
                    );
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
                    if resp.hovered() {
                        ui.painter().rect_stroke(
                            inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                            4.0,
                            egui::Stroke::new(1.0, Color32::from_white_alpha(12)),
                        );
                    }
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
            ui.menu_button("Add bytes at end", |ui| {
                if ui.button("4 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 4);
                    ui.close_menu();
                }
                if ui.button("8 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 8);
                    ui.close_menu();
                }
                if ui.button("64 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 64);
                    ui.close_menu();
                }
                if ui.button("256 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 256);
                    ui.close_menu();
                }
                if ui.button("1024 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 1024);
                    ui.close_menu();
                }
                if ui.button("2048 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 2048);
                    ui.close_menu();
                }
                if ui.button("4096 bytes").clicked() {
                    self.add_n_bytes_at_end(&ctx, 4096);
                    ui.close_menu();
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    let buf = &mut self.bytes_custom_buffer;
                    ui.text_edit_singleline(buf);
                    if ui.button("Add").clicked() {
                        if let Ok(n) = buf.trim().parse::<u64>() {
                            self.add_n_bytes_at_end(&ctx, n as usize);
                            self.bytes_custom_buffer.clear();
                            ui.close_menu();
                        }
                    }
                });
            });

            ui.menu_button("Insert bytes here", |ui| {
                if ui.button("4 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 4);
                    ui.close_menu();
                }
                if ui.button("8 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 8);
                    ui.close_menu();
                }
                if ui.button("64 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 64);
                    ui.close_menu();
                }
                if ui.button("256 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 256);
                    ui.close_menu();
                }
                if ui.button("1024 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 1024);
                    ui.close_menu();
                }
                if ui.button("2048 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 2048);
                    ui.close_menu();
                }
                if ui.button("4096 bytes").clicked() {
                    self.insert_n_bytes_here(&ctx, 4096);
                    ui.close_menu();
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    let buf = &mut self.bytes_custom_buffer;
                    ui.text_edit_singleline(buf);
                    if ui.button("Insert").clicked() {
                        if let Ok(n) = buf.trim().parse::<u64>() {
                            self.insert_n_bytes_here(&ctx, n as usize);
                            self.bytes_custom_buffer.clear();
                            ui.close_menu();
                        }
                    }
                });
            });
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
                    FieldType::Pointer,
                    FieldType::Enum,
                ] {
                    let label = format!("{t:?}");
                    if ui.button(label).clicked() {
                        let ms = unsafe { &mut *ctx.mem_ptr };
                        if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                            def.set_field_type_at(ctx.field_index, t.clone());
                            if t == FieldType::Pointer {
                                if let Some(fd) = def.fields.get_mut(ctx.field_index) {
                                    fd.pointer_target =
                                        Some(PointerTarget::FieldType(FieldType::Hex64));
                                }
                            } else if t == FieldType::Enum {
                                if let Some(fd) = def.fields.get_mut(ctx.field_index) {
                                    // Assign a default enum if any exists
                                    let names =
                                        unsafe { (*ctx.mem_ptr).enum_registry.get_enum_names() };
                                    if let Some(first) = names.into_iter().next() {
                                        fd.enum_name = Some(first);
                                    } else {
                                        fd.enum_name = None;
                                    }
                                }
                            }
                            self.schedule_rebuild();
                        }
                        ui.close_menu();
                    }
                }
            });

            // If enum, allow choosing enum definition (size is defined on enum, not per-field)
            if let Some(ms) = unsafe { (ctx.mem_ptr).as_mut() } {
                if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                    if let Some(fd) = def.fields.get_mut(ctx.field_index) {
                        if fd.field_type == FieldType::Enum {
                            ui.separator();
                            ui.label("Enum:");
                            let mut current =
                                fd.enum_name.clone().unwrap_or_else(|| "<none>".to_string());
                            egui::ComboBox::from_id_source((
                                "enum_combo",
                                ctx.owner_class_name.clone(),
                                ctx.field_index,
                            ))
                            .selected_text(current.clone())
                            .show_ui(ui, |ui| {
                                for n in ms.enum_registry.get_enum_names() {
                                    ui.selectable_value(&mut current, n.clone(), n);
                                }
                            });
                            if fd.enum_name.as_deref() != Some(&current)
                                && ms.enum_registry.contains(&current)
                            {
                                fd.enum_name = Some(current);
                                self.schedule_rebuild();
                            }
                        }
                    }
                }
            }

            // Pointer target configuration menu
            if let Some(ms) = unsafe { (ctx.mem_ptr).as_mut() } {
                if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                    if let Some(fd) = def.fields.get(ctx.field_index) {
                        if fd.field_type == FieldType::Pointer {
                            ui.menu_button("Pointer target", |ui| {
                                ui.menu_button("Primitive", |ui| {
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
                                        FieldType::Vector2,
                                        FieldType::Vector3,
                                        FieldType::Vector4,
                                        FieldType::Text,
                                        FieldType::TextPointer,
                                        FieldType::Enum,
                                    ] {
                                        let label = format!("{t:?}");
                                        if ui.button(label).clicked() {
                                            let ms = unsafe { &mut *ctx.mem_ptr };
                                            if let Some(defm) =
                                                ms.class_registry.get_mut(&ctx.owner_class_name)
                                            {
                                                if let Some(fdm) =
                                                    defm.fields.get_mut(ctx.field_index)
                                                {
                                                    if t == FieldType::Enum {
                                                        let names = unsafe {
                                                            (*ctx.mem_ptr)
                                                                .enum_registry
                                                                .get_enum_names()
                                                        };
                                                        if let Some(first) =
                                                            names.into_iter().next()
                                                        {
                                                            fdm.pointer_target = Some(
                                                                PointerTarget::EnumName(first),
                                                            );
                                                        } else {
                                                            fdm.pointer_target =
                                                                Some(PointerTarget::FieldType(
                                                                    FieldType::UInt32,
                                                                ));
                                                        }
                                                    } else {
                                                        fdm.pointer_target =
                                                            Some(PointerTarget::FieldType(t));
                                                    }
                                                }
                                                self.schedule_rebuild();
                                            }
                                            ui.close_menu();
                                        }
                                    }
                                });
                                ui.menu_button("Enum", |ui| {
                                    let names =
                                        unsafe { (*ctx.mem_ptr).enum_registry.get_enum_names() };
                                    for name in names {
                                        if ui.button(name.clone()).clicked() {
                                            let ms = unsafe { &mut *ctx.mem_ptr };
                                            if let Some(defm) =
                                                ms.class_registry.get_mut(&ctx.owner_class_name)
                                            {
                                                if let Some(fdm) =
                                                    defm.fields.get_mut(ctx.field_index)
                                                {
                                                    fdm.pointer_target =
                                                        Some(PointerTarget::EnumName(name));
                                                }
                                            }
                                            self.schedule_rebuild();
                                            ui.close_menu();
                                        }
                                    }
                                });
                                ui.menu_button("Class", |ui| {
                                    if ui.button("Create new class here").clicked() {
                                        let ms = unsafe { &mut *ctx.mem_ptr };
                                        let base_name = "NewClass";
                                        let unique_name = generate_unique_class_name(
                                            &ms.class_registry,
                                            base_name,
                                        );
                                        let mut new_def = ClassDefinition::new(unique_name.clone());
                                        new_def.add_hex_field(FieldType::Hex64);
                                        ms.class_registry.register(new_def);
                                        if let Some(defm) =
                                            ms.class_registry.get_mut(&ctx.owner_class_name)
                                        {
                                            if let Some(fdm) = defm.fields.get_mut(ctx.field_index)
                                            {
                                                fdm.pointer_target =
                                                    Some(PointerTarget::ClassName(unique_name));
                                            }
                                        }
                                        self.schedule_rebuild();
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    let names =
                                        unsafe { (*ctx.mem_ptr).class_registry.get_class_names() };
                                    for name in names {
                                        if ui.button(name.clone()).clicked() {
                                            let ms = unsafe { &mut *ctx.mem_ptr };
                                            if let Some(defm) =
                                                ms.class_registry.get_mut(&ctx.owner_class_name)
                                            {
                                                if let Some(fdm) =
                                                    defm.fields.get_mut(ctx.field_index)
                                                {
                                                    fdm.pointer_target =
                                                        Some(PointerTarget::ClassName(name));
                                                }
                                            }
                                            self.schedule_rebuild();
                                            ui.close_menu();
                                        }
                                    }
                                });
                            });
                        }
                    }
                }
            }
            ui.separator();
            if ui.button("Create class from field").clicked() {
                let ms = unsafe { &mut *ctx.mem_ptr };
                let base_name = "NewClass";
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
                ui.close_menu();
            }
        });
    }

    fn add_n_bytes_at_end(&mut self, ctx: &FieldCtx, num_bytes: usize) {
        if num_bytes == 0 {
            return;
        }
        let ms = unsafe { &mut *ctx.mem_ptr };
        if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
            let mut remaining = num_bytes;
            // Chunk into largest hex fields to minimize count
            while remaining >= 8 {
                def.add_hex_field(FieldType::Hex64);
                remaining -= 8;
            }
            while remaining >= 4 {
                def.add_hex_field(FieldType::Hex32);
                remaining -= 4;
            }
            while remaining >= 2 {
                def.add_hex_field(FieldType::Hex16);
                remaining -= 2;
            }
            while remaining > 0 {
                def.add_hex_field(FieldType::Hex8);
                remaining -= 1;
            }
            self.schedule_rebuild();
        }
    }

    fn insert_n_bytes_here(&mut self, ctx: &FieldCtx, num_bytes: usize) {
        if num_bytes == 0 {
            return;
        }
        let ms = unsafe { &mut *ctx.mem_ptr };
        if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
            let mut remaining = num_bytes;
            let mut insert_index = ctx.field_index;
            // Insert before current field, largest first to maintain order of chunks
            while remaining >= 8 {
                def.insert_hex_field_at(insert_index, FieldType::Hex64);
                insert_index += 1;
                remaining -= 8;
            }
            while remaining >= 4 {
                def.insert_hex_field_at(insert_index, FieldType::Hex32);
                insert_index += 1;
                remaining -= 4;
            }
            while remaining >= 2 {
                def.insert_hex_field_at(insert_index, FieldType::Hex16);
                insert_index += 1;
                remaining -= 2;
            }
            while remaining > 0 {
                def.insert_hex_field_at(insert_index, FieldType::Hex8);
                insert_index += 1;
                remaining -= 1;
            }
            self.schedule_rebuild();
        }
    }
}

fn generate_unique_class_name(registry: &ClassDefinitionRegistry, base: &str) -> String {
    if !registry.contains(base) {
        return base.to_string();
    }
    let mut counter: usize = 1;
    loop {
        let candidate = format!("{base}_{counter}");
        if !registry.contains(&candidate) {
            return candidate;
        }
        counter += 1;
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
    let width = galley.rect.width() + 12.0;
    ui.add_sized(
        [width, ui.text_style_height(&TextStyle::Body)],
        TextEdit::singleline(text),
    )
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
            let len = field.get_size() as usize;
            let mut buf = vec![0u8; len];
            (handle.read_slice(addr, buf.as_mut_slice()).ok()).map(|_| {
                buf.iter()
                    .map(|b| format!("{b:02X}"))
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

        FieldType::Pointer => {
            let ptr = handle.read_sized::<u64>(addr).ok()?;
            let addr_str = format!("-> 0x{ptr:016X}");
            if let Some(PointerTarget::FieldType(ref t)) = field.pointer_target {
                if let Some(val) = read_value_preview_for_type(handle, t, ptr) {
                    Some(format!("{addr_str} = {val}"))
                } else {
                    Some(addr_str)
                }
            } else {
                Some(addr_str)
            }
        }

        FieldType::ClassInstance => None,
        FieldType::Enum => None,
    }
}

fn enum_value_string(
    handle: &AppHandle,
    class_def: &ClassDefinition,
    field: &MemoryField,
    memory: &MemoryStructure,
) -> Option<String> {
    let def = class_def.fields.iter().find(|fd| fd.id == field.def_id)?;
    let ename = def.enum_name.as_ref()?;
    let edef = memory.enum_registry.get(ename)?;
    let size = edef.default_size;
    // Read numeric value according to enum's underlying size
    let (val_u64, val_str) = match size {
        1 => {
            let v = handle.read_sized::<u8>(field.address).ok()? as u64;
            (v, v.to_string())
        }
        2 => {
            let v = handle.read_sized::<u16>(field.address).ok()? as u64;
            (v, v.to_string())
        }
        8 => {
            let v = handle.read_sized::<u64>(field.address).ok()?;
            (v, v.to_string())
        }
        _ => {
            let v = handle.read_sized::<u32>(field.address).ok()? as u64;
            (v, v.to_string())
        }
    };

    // Prefer named variant when available; otherwise fall back to numeric value
    if let Some(variant) = edef
        .variants
        .iter()
        .find(|variant| (variant.value as u64) == val_u64)
    {
        Some(variant.name.clone())
    } else {
        Some(val_str)
    }
}

fn enum_suffix_for_field(
    class_def: &ClassDefinition,
    field: &MemoryField,
    memory: &MemoryStructure,
) -> String {
    if field.field_type != FieldType::Enum {
        return String::new();
    }
    let def = match class_def.fields.iter().find(|fd| fd.id == field.def_id) {
        Some(d) => d,
        None => return String::new(),
    };
    let en = match &def.enum_name {
        Some(n) => n,
        None => return String::from(" -> <enum?>"),
    };
    if let Some(_ed) = memory.enum_registry.get(en) {
        format!(" -> {en}")
    } else {
        String::from(" -> <enum?>")
    }
}

fn read_value_preview_for_type(handle: &AppHandle, t: &FieldType, addr: u64) -> Option<String> {
    match t {
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
            let len = t.get_size() as usize;
            let mut buf = vec![0u8; len];
            (handle.read_slice(addr, buf.as_mut_slice()).ok()).map(|_| {
                buf.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        }

        FieldType::Text => handle.read_string(addr, Some(32)).ok(),
        FieldType::TextPointer => {
            if let Ok(ptr2) = handle.read_sized::<u64>(addr) {
                if ptr2 != 0 {
                    handle.read_string(ptr2, None).ok()
                } else {
                    Some(String::from("(null)"))
                }
            } else {
                None
            }
        }

        // For nested types that shouldn't appear here
        FieldType::ClassInstance | FieldType::Pointer => None,
        FieldType::Enum => handle.read_sized::<u32>(addr).ok().map(|v| v.to_string()),
    }
}
