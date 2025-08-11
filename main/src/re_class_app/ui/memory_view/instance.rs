use std::sync::Arc;

use eframe::egui::{
    self,
    Color32,
    RichText,
    Ui,
};
use handle::AppHandle;

use super::{
    context_menu::FieldCtx,
    util::{
        field_value_string,
        text_edit_autowidth,
        FieldKey,
    },
};
use crate::memory::{
    ClassDefinition,
    ClassInstance,
    FieldType,
    MemoryStructure,
    MemoryStructure as MSForSig,
    PointerTarget,
};

fn enum_suffix_for_field(
    class_def: &ClassDefinition,
    field: &crate::memory::MemoryField,
    memory: &MSForSig,
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

fn enum_value_string(
    handle: &AppHandle,
    class_def: &ClassDefinition,
    field: &crate::memory::MemoryField,
    memory: &MSForSig,
) -> Option<String> {
    let def = class_def.fields.iter().find(|fd| fd.id == field.def_id)?;
    let ename = def.enum_name.as_ref()?;
    let edef = memory.enum_registry.get(ename)?;
    let size = edef.default_size;
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
use crate::re_class_app::ReClassGui;

impl ReClassGui {
    pub(super) fn render_instance(
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
}
