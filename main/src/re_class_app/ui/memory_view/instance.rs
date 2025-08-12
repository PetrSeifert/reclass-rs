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
    fn update_selection_for_click(
        &mut self,
        ui: &mut Ui,
        instance_address: u64,
        clicked_index: usize,
        def_ids: &[u64],
        def_id: u64,
    ) {
        let mods = ui.input(|i| i.modifiers);
        let ctrl = mods.command || mods.ctrl;
        let shift = mods.shift;

        let key = FieldKey {
            instance_address,
            field_def_id: def_id,
        };

        // Enforce single-instance selection
        if self
            .selected_instance_address
            .map(|addr| addr != instance_address)
            .unwrap_or(false)
        {
            self.selected_fields.clear();
            self.selection_anchor = None;
            self.selected_instance_address = Some(instance_address);
        }

        if shift {
            match self.selection_anchor {
                Some((anchor_addr, anchor_idx)) if anchor_addr == instance_address => {
                    let (start, end) = if anchor_idx <= clicked_index {
                        (anchor_idx, clicked_index)
                    } else {
                        (clicked_index, anchor_idx)
                    };
                    // Select the whole range
                    for idx in start..=end {
                        if let Some(&fid) = def_ids.get(idx) {
                            let k = FieldKey {
                                instance_address,
                                field_def_id: fid,
                            };
                            self.selected_fields.insert(k);
                        }
                    }
                    self.selected_instance_address = Some(instance_address);
                }
                _ => {
                    // No valid anchor: treat as single select and set anchor
                    self.selected_fields.clear();
                    self.selected_fields.insert(key);
                    self.selection_anchor = Some((instance_address, clicked_index));
                    self.selected_instance_address = Some(instance_address);
                }
            }
        } else if ctrl {
            // Toggle selection
            if self.selected_fields.contains(&key) {
                self.selected_fields.remove(&key);
            } else {
                if self
                    .selected_instance_address
                    .map(|addr| addr == instance_address)
                    .unwrap_or(true)
                {
                    self.selected_fields.insert(key);
                } else {
                    // Start selection in this instance
                    self.selected_fields.clear();
                    self.selected_fields.insert(key);
                    self.selected_instance_address = Some(instance_address);
                }
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some((instance_address, clicked_index));
                }
            }
            if self.selected_fields.is_empty() {
                self.selection_anchor = None;
                self.selected_instance_address = None;
            } else {
                self.selected_instance_address = Some(instance_address);
            }
        } else {
            // Basic click: single select and set anchor
            self.selected_fields.clear();
            self.selected_fields.insert(key);
            self.selection_anchor = Some((instance_address, clicked_index));
            self.selected_instance_address = Some(instance_address);
        }
    }

    pub(super) fn render_instance(
        &mut self,
        ui: &mut Ui,
        instance: &mut ClassInstance,
        handle: Option<Arc<AppHandle>>,
        mem_ptr: *mut MemoryStructure,
        path: &mut Vec<usize>,
    ) {
        let instance_address = instance.address;
        let def_ids: Vec<u64> = instance
            .class_definition
            .fields
            .iter()
            .map(|fd| fd.id)
            .collect();
        for (idx, field) in instance.fields.iter_mut().enumerate() {
            match field.field_type {
                FieldType::Pointer => {
                    let def_id = *def_ids.get(idx).unwrap_or(&0);
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
                                            let mut nested = ClassInstance::new(
                                                field.name.clone().unwrap_or_default(),
                                                ptr,
                                                class_def,
                                            );
                                            ms.bind_nested_for_instance(&mut nested);
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
                            instance_address,
                            address: field.address,
                            value_preview: None,
                        };
                        // Selection on header click for this field
                        if collapsing.header_response.clicked() {
                            self.update_selection_for_click(
                                ui,
                                instance_address,
                                idx,
                                &def_ids,
                                def_id,
                            );
                        }
                        self.context_menu_for_field(&collapsing.header_response, ctx);
                    } else if matches!(field.pointer_target, Some(PointerTarget::Array { .. })) {
                        // Pointer to Array: show a header with element info and expand to elements
                        let def_id = *def_ids.get(idx).unwrap_or(&0);
                        let mut header = {
                            let offset_from_class = field.address.saturating_sub(instance.address);
                            let mut h = format!(
                                "+0x{:04X}  0x{:08X}    {}: Pointer -> Array",
                                offset_from_class,
                                field.address,
                                field.name.clone().unwrap_or_default()
                            );
                            if let Some(hd) = &handle {
                                if let Ok(ptr) = hd.read_sized::<u64>(field.address) {
                                    h.push_str(&format!(" (-> 0x{ptr:016X})"));
                                }
                            }
                            h
                        };
                        if let Some(PointerTarget::Array { element, length }) = &field.pointer_target {
                            let desc = match element.as_ref() {
                                PointerTarget::FieldType(t) => format!("{}", t),
                                PointerTarget::EnumName(en) => en.clone(),
                                PointerTarget::ClassName(cn) => cn.clone(),
                                PointerTarget::Array { .. } => String::from("Array"),
                            };
                            header.push_str(&format!(" [{}] {}", length, desc));
                        }
                        let collapsing = egui::CollapsingHeader::new(header)
                            .default_open(false)
                            .id_source(("ptr_arr_field", def_id, path.clone()))
                            .show(ui, |ui| {
                                if let (Some(hd), Some(PointerTarget::Array { element, length })) = (handle.as_ref(), &field.pointer_target) {
                                    if let Ok(ptr) = hd.read_sized::<u64>(field.address) {
                                        if ptr != 0 {
                                            let len = *length as usize;
                                            match element.as_ref() {
                                                PointerTarget::FieldType(t) => {
                                                    let elem_size = t.get_size();
                                                    for i in 0..len {
                                                        let elem_addr = ptr + (i as u64) * elem_size;
                                                        let val = match t {
                                                            FieldType::Hex64 => hd.read_sized::<u64>(elem_addr).ok().map(|v| format!("0x{v:016X}")),
                                                            FieldType::Hex32 => hd.read_sized::<u32>(elem_addr).ok().map(|v| format!("0x{v:08X}")),
                                                            FieldType::Hex16 => hd.read_sized::<u16>(elem_addr).ok().map(|v| format!("0x{v:04X}")),
                                                            FieldType::Hex8 => hd.read_sized::<u8>(elem_addr).ok().map(|v| format!("0x{v:02X}")),
                                                            FieldType::UInt64 => hd.read_sized::<u64>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::UInt32 => hd.read_sized::<u32>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::UInt16 => hd.read_sized::<u16>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::UInt8 => hd.read_sized::<u8>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::Int64 => hd.read_sized::<i64>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::Int32 => hd.read_sized::<i32>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::Int16 => hd.read_sized::<i16>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::Int8 => hd.read_sized::<i8>(elem_addr).ok().map(|v| v.to_string()),
                                                            FieldType::Bool => hd.read_sized::<u8>(elem_addr).ok().map(|v| if v != 0 { "true".to_string() } else { "false".to_string() }),
                                                            FieldType::Float => hd.read_sized::<f32>(elem_addr).ok().map(|v| format!("{v}")),
                                                            FieldType::Double => hd.read_sized::<f64>(elem_addr).ok().map(|v| format!("{v}")),
                                                            FieldType::Vector2 | FieldType::Vector3 | FieldType::Vector4 => {
                                                                let lenb = t.get_size() as usize;
                                                                let mut buf = vec![0u8; lenb];
                                                                hd.read_slice(elem_addr, buf.as_mut_slice()).ok().map(|_|
                                                                    buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
                                                                )
                                                            }
                                                            FieldType::Text => hd.read_string(elem_addr, Some(32)).ok(),
                                                            FieldType::TextPointer | FieldType::Pointer => {
                                                                hd.read_sized::<u64>(elem_addr).ok().map(|v| format!("0x{v:016X}"))
                                                            }
                                                            _ => None,
                                                        };
                                                        ui.monospace(format!("[{}] 0x{:08X}{}", i, elem_addr, val.map(|vv| format!(" = {vv}")).unwrap_or_default()));
                                                    }
                                                }
                                                PointerTarget::EnumName(en) => {
                                                    if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                                        if let Some(ed) = ms.enum_registry.get(en) {
                                                            let sz = ed.default_size;
                                                            for i in 0..len {
                                                                let elem_addr = ptr + (i as u64) * (sz as u64);
                                                                let (raw_u64, raw_str) = match sz {
                                                                    1 => { let v = hd.read_sized::<u8>(elem_addr).ok().unwrap_or(0) as u64; (v, v.to_string()) }
                                                                    2 => { let v = hd.read_sized::<u16>(elem_addr).ok().unwrap_or(0) as u64; (v, v.to_string()) }
                                                                    8 => { let v = hd.read_sized::<u64>(elem_addr).ok().unwrap_or(0); (v, v.to_string()) }
                                                                    _ => { let v = hd.read_sized::<u32>(elem_addr).ok().unwrap_or(0) as u64; (v, v.to_string()) }
                                                                };
                                                                let name = ed.variants.iter().find(|v| (v.value as u64)==raw_u64).map(|v| v.name.clone()).unwrap_or(raw_str);
                                                                ui.monospace(format!("[{}] 0x{:08X} = {}", i, elem_addr, name));
                                                            }
                                                        }
                                                    }
                                                }
                                                PointerTarget::ClassName(cn) => {
                                                    if let Some(ms) = unsafe { (mem_ptr).as_mut() } {
                                                        if let Some(class_def) = ms.class_registry.get(cn).cloned() {
                                                            let elem_size = class_def.total_size.max(1);
                                                            for i in 0..len {
                                                                let elem_addr = ptr + (i as u64) * elem_size;
                                                                let mut nested = ClassInstance::new(
                                                                    format!("{}[{}]", field.name.clone().unwrap_or_default(), i),
                                                                    elem_addr,
                                                                    class_def.clone(),
                                                                );
                                                                ms.bind_nested_for_instance(&mut nested);
                                                                ui.separator();
                                                                ui.label(RichText::new(format!("Element [{}] @ 0x{:08X}", i, elem_addr)).strong());
                                                                path.push(idx);
                                                                //path.push(i);
                                                                self.render_instance(ui, &mut nested, handle.clone(), mem_ptr, path);
                                                                //path.pop();
                                                                path.pop();
                                                            }
                                                        }
                                                    }
                                                }
                                                PointerTarget::Array { .. } => {}
                                            }
                                        }
                                    }
                                }
                            });
                        let ctx = FieldCtx {
                            mem_ptr,
                            owner_class_name: instance.class_definition.name.clone(),
                            field_index: idx,
                            instance_address,
                            address: field.address,
                            value_preview: None,
                        };
                        if collapsing.header_response.clicked() {
                            self.update_selection_for_click(
                                ui,
                                instance_address,
                                idx,
                                &def_ids,
                                def_id,
                            );
                        }
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
                                    Some(PointerTarget::Array { element, length }) => {
                                        match element.as_ref() {
                                            PointerTarget::FieldType(t) => format!(": {} -> Array [{}] {}", field.field_type, length, t),
                                            PointerTarget::EnumName(en) => format!(": {} -> Array [{}] {}", field.field_type, length, en),
                                            PointerTarget::ClassName(cn) => format!(": {} -> Array [{}] {}", field.field_type, length, cn),
                                            PointerTarget::Array { .. } => String::from(": Pointer -> Array [..] Array"),
                                        }
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
                                    Some(PointerTarget::Array { element, length }) => {
                                        match element.as_ref() {
                                            PointerTarget::FieldType(t) => format!("{} -> Array [{}] {}", field.field_type, length, t),
                                            PointerTarget::EnumName(en) => format!("{} -> Array [{}] {}", field.field_type, length, en),
                                            PointerTarget::ClassName(cn) => format!("{} -> Array [{}] {}", field.field_type, length, cn),
                                            PointerTarget::Array { .. } => String::from("Pointer -> Array [..] Array"),
                                        }
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
                            instance_address,
                            address: field.address,
                            value_preview: field_value_string(handle.clone(), field),
                        };
                        let id = ui.id().with(("row_ptr_prim", def_id, path.clone(), idx));
                        let resp = ui.interact(inner.response.rect, id, egui::Sense::click());
                        // Draw selection highlight
                        let key = FieldKey {
                            instance_address,
                            field_def_id: def_id,
                        };
                        if self.selected_fields.contains(&key) {
                            ui.painter().rect_filled(
                                inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                                4.0,
                                Color32::from_white_alpha(18),
                            );
                            ui.painter().rect_stroke(
                                inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                                4.0,
                                egui::Stroke::new(1.5, Color32::from_rgb(100, 160, 255)),
                            );
                        }
                        if resp.hovered() {
                            ui.painter().rect_stroke(
                                inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                                4.0,
                                egui::Stroke::new(1.0, Color32::from_white_alpha(12)),
                            );
                        }
                        if resp.clicked() {
                            self.update_selection_for_click(
                                ui,
                                instance_address,
                                idx,
                                &def_ids,
                                def_id,
                            );
                        }
                        self.context_menu_for_field(&resp, ctx);
                    }
                }
                FieldType::Array => {
                        let (header_text, len_u32) = if let Some(fd) = instance
                        .class_definition
                        .fields
                        .get(idx)
                    {
                        let len = fd.array_length.unwrap_or(0);
                        let desc = match &fd.array_element {
                            Some(PointerTarget::FieldType(t)) => format!("{}", t),
                            Some(PointerTarget::EnumName(en)) => en.clone(),
                            Some(PointerTarget::ClassName(cn)) => cn.clone(),
                            Some(PointerTarget::Array { .. }) => String::from("Array"),
                            None => String::from("<elem?>"),
                        };
                        (format!(
                            "0x{:08X}    {}: Array -> [{}] {}",
                            field.address,
                            field.name.clone().unwrap_or_default(),
                            len,
                            desc
                        ), len)
                    } else {
                        (format!("0x{:08X}    {}: Array", field.address, field.name.clone().unwrap_or_default()), 0)
                    };

                    let def_id = *def_ids.get(idx).unwrap_or(&0);
                    let collapsing = egui::CollapsingHeader::new(header_text)
                        .default_open(false)
                        .id_source(("arr_field", def_id, path.clone()))
                        .show(ui, |ui| {
                            if let Some(fd) = instance.class_definition.fields.get(idx) {
                                let len = len_u32 as usize;
                                match &fd.array_element {
                                    Some(PointerTarget::FieldType(t)) => {
                                        if let Some(h) = &handle {
                                            let elem_size = t.get_size();
                                            for i in 0..len {
                                                let elem_addr = field.address + (i as u64) * elem_size;
                                                let offset_from_class = elem_addr.saturating_sub(instance.address);
                                                let val = match t {
                                                    FieldType::Hex64 => h.read_sized::<u64>(elem_addr).ok().map(|v| format!("0x{v:016X}")),
                                                    FieldType::Hex32 => h.read_sized::<u32>(elem_addr).ok().map(|v| format!("0x{v:08X}")),
                                                    FieldType::Hex16 => h.read_sized::<u16>(elem_addr).ok().map(|v| format!("0x{v:04X}")),
                                                    FieldType::Hex8 => h.read_sized::<u8>(elem_addr).ok().map(|v| format!("0x{v:02X}")),
                                                    FieldType::UInt64 => h.read_sized::<u64>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::UInt32 => h.read_sized::<u32>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::UInt16 => h.read_sized::<u16>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::UInt8 => h.read_sized::<u8>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::Int64 => h.read_sized::<i64>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::Int32 => h.read_sized::<i32>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::Int16 => h.read_sized::<i16>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::Int8 => h.read_sized::<i8>(elem_addr).ok().map(|v| v.to_string()),
                                                    FieldType::Bool => h.read_sized::<u8>(elem_addr).ok().map(|v| if v != 0 { "true".to_string() } else { "false".to_string() }),
                                                    FieldType::Float => h.read_sized::<f32>(elem_addr).ok().map(|v| format!("{v}")),
                                                    FieldType::Double => h.read_sized::<f64>(elem_addr).ok().map(|v| format!("{v}")),
                                                    FieldType::Vector2 | FieldType::Vector3 | FieldType::Vector4 => {
                                                        let lenb = t.get_size() as usize;
                                                        let mut buf = vec![0u8; lenb];
                                                        h.read_slice(elem_addr, buf.as_mut_slice()).ok().map(|_|
                                                            buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
                                                        )
                                                    }
                                                    FieldType::Text => h.read_string(elem_addr, Some(32)).ok(),
                                                    FieldType::TextPointer | FieldType::Pointer => {
                                                        h.read_sized::<u64>(elem_addr).ok().map(|v| format!("0x{v:016X}"))
                                                    }
                                                    _ => None,
                                                };
                                                ui.monospace(format!(
                                                    "+0x{:04X}  0x{:08X}  [{}]{}",
                                                    offset_from_class,
                                                    elem_addr,
                                                    i,
                                                    val.map(|vv| format!(" = {vv}")).unwrap_or_default()
                                                ));
                                            }
                                        }
                                    }
                                    Some(PointerTarget::EnumName(en)) => {
                                        if let (Some(h), Some(ms)) = (handle.as_ref(), unsafe { (mem_ptr).as_ref() }) {
                                            if let Some(ed) = ms.enum_registry.get(en) {
                                                let sz = ed.default_size;
                                                for i in 0..len {
                                                    let elem_addr = field.address + (i as u64) * (sz as u64);
                                                    let offset_from_class = elem_addr.saturating_sub(instance.address);
                                                    let (raw_u64, raw_str) = match sz {
                                                        1 => { let v = h.read_sized::<u8>(elem_addr).ok().unwrap_or(0) as u64; (v, v.to_string()) }
                                                        2 => { let v = h.read_sized::<u16>(elem_addr).ok().unwrap_or(0) as u64; (v, v.to_string()) }
                                                        8 => { let v = h.read_sized::<u64>(elem_addr).ok().unwrap_or(0); (v, v.to_string()) }
                                                        _ => { let v = h.read_sized::<u32>(elem_addr).ok().unwrap_or(0) as u64; (v, v.to_string()) }
                                                    };
                                                    let name = ed.variants.iter().find(|v| (v.value as u64)==raw_u64).map(|v| v.name.clone()).unwrap_or(raw_str);
                                                    ui.monospace(format!("+0x{:04X}  0x{:08X}  [{}] = {}", offset_from_class, elem_addr, i, name));
                                                }
                                            }
                                        }
                                    }
                                    Some(PointerTarget::ClassName(cn)) => {
                                        if let Some(ms) = unsafe { (mem_ptr).as_mut() } {
                                            if let Some(class_def) = ms.class_registry.get(cn).cloned() {
                                                let elem_size = class_def.total_size.max(1);
                                                for i in 0..len {
                                                    let elem_addr = field.address + (i as u64) * elem_size;
                                                    let mut nested = ClassInstance::new(
                                                        format!("{}[{}]", field.name.clone().unwrap_or_default(), i),
                                                        elem_addr,
                                                        class_def.clone(),
                                                    );
                                                    ms.bind_nested_for_instance(&mut nested);
                                                    ui.separator();
                                                    ui.label(RichText::new(format!("Element [{}] @ 0x{:08X}", i, elem_addr)).strong());
                                                    path.push(idx);
                                                    path.push(i);
                                                    self.render_instance(ui, &mut nested, handle.clone(), mem_ptr, path);
                                                    path.pop();
                                                    path.pop();
                                                }
                                            }
                                        }
                                    }
                                    Some(PointerTarget::Array { .. }) => {
                                        ui.monospace("<nested array rendering not supported>");
                                    }
                                    None => {
                                        ui.monospace("<no element type set>");
                                    }
                                }
                            }
                        });

                    let ctx = FieldCtx { mem_ptr, owner_class_name: instance.class_definition.name.clone(), field_index: idx, instance_address, address: field.address, value_preview: None };
                    if collapsing.header_response.clicked() {
                        self.update_selection_for_click(ui, instance_address, idx, &def_ids, def_id);
                    }
                    self.context_menu_for_field(&collapsing.header_response, ctx);
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
                    let def_id = *def_ids.get(idx).unwrap_or(&0);
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
                        instance_address,
                        address: field.address,
                        value_preview: None,
                    };
                    // Selection on header click for this field
                    if collapsing.header_response.clicked() {
                        self.update_selection_for_click(
                            ui,
                            instance_address,
                            idx,
                            &def_ids,
                            def_id,
                        );
                    }
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
                        instance_address,
                        address: field.address,
                        value_preview: field_value_string(handle.clone(), field),
                    };
                    let def_id = *def_ids.get(idx).unwrap_or(&0);
                    let id = ui.id().with(("row_field", def_id, path.clone(), idx));
                    let resp = ui.interact(inner.response.rect, id, egui::Sense::click());
                    // Draw selection highlight
                    let key = FieldKey {
                        instance_address,
                        field_def_id: def_id,
                    };
                    if self.selected_fields.contains(&key) {
                        ui.painter().rect_filled(
                            inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                            4.0,
                            Color32::from_white_alpha(18),
                        );
                        ui.painter().rect_stroke(
                            inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                            4.0,
                            egui::Stroke::new(1.5, Color32::from_rgb(100, 160, 255)),
                        );
                    }
                    if resp.hovered() {
                        ui.painter().rect_stroke(
                            inner.response.rect.expand2(egui::vec2(4.0, 2.0)),
                            4.0,
                            egui::Stroke::new(1.0, Color32::from_white_alpha(12)),
                        );
                    }
                    if resp.clicked() {
                        self.update_selection_for_click(
                            ui,
                            instance_address,
                            idx,
                            &def_ids,
                            def_id,
                        );
                    }
                    self.context_menu_for_field(&resp, ctx);
                }
            }
        }
    }
}
