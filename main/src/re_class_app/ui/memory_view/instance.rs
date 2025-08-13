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
    let is_enum = class_def
        .fields
        .iter()
        .find(|fd| fd.id == field.def_id)
        .map(|fd| fd.field_type == FieldType::Enum)
        .unwrap_or(false);
    if !is_enum {
        return String::new();
    }
    let def = match class_def.fields.iter().find(|fd| fd.id == field.def_id) {
        Some(d) => d,
        None => return String::new(),
    };
    if let Some(eid) = def.enum_id {
        if let Some(ed) = memory.enum_registry.get_by_id(eid) {
            format!(" -> {}", ed.name)
        } else {
            String::from(" -> <enum?>")
        }
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
    let eid = def.enum_id?;
    let edef = memory.enum_registry.get_by_id(eid)?;
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
    fn compute_display_size_for(
        &self,
        field_type: &FieldType,
        class_def: &ClassDefinition,
        field: &crate::memory::MemoryField,
        mem_ptr: *mut MemoryStructure,
    ) -> u64 {
        if matches!(field_type, FieldType::Enum) {
            if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                if let Some(fd) = class_def.fields.iter().find(|fdef| fdef.id == field.def_id) {
                    if let Some(eid) = fd.enum_id {
                        if let Some(ed) = ms.enum_registry.get_by_id(eid) {
                            return ed.default_size as u64;
                        }
                    }
                }
            }
        }
        field_type.get_size()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_field_name_inline_editor(
        &mut self,
        ui: &mut Ui,
        mem_ptr: *mut MemoryStructure,
        instance_class_id: u64,
        instance_address: u64,
        def_id: u64,
        idx: usize,
        current_name: Option<String>,
        schedule_rebuild: bool,
    ) {
        let key = FieldKey {
            instance_address,
            field_def_id: def_id,
        };
        let mut fname = self
            .field_name_buffers
            .get(&key)
            .cloned()
            .unwrap_or_else(|| current_name.unwrap_or_default());
        let resp = text_edit_autowidth(ui, &mut fname);
        if resp.changed() {
            self.field_name_buffers.insert(key, fname.clone());
        }
        let enter_on_this =
            ui.input(|i| i.key_pressed(egui::Key::Enter)) && ui.memory(|m| m.has_focus(resp.id));
        if resp.lost_focus() || enter_on_this {
            let ms = unsafe { &mut *mem_ptr };
            if let Some(def) = ms.class_registry.get_mut(instance_class_id) {
                if let Some(fd) = def.fields.get_mut(idx) {
                    fd.name = Some(fname);
                }
                if schedule_rebuild {
                    self.schedule_rebuild();
                } else {
                    self.needs_rebuild = true;
                }
            }
            self.field_name_buffers.remove(&key);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_row_and_handle_selection(
        &mut self,
        ui: &mut Ui,
        rect: egui::Rect,
        idx: usize,
        id_prefix: &str,
        def_id: u64,
        path: &[usize],
        instance_address: u64,
        def_ids: &[u64],
        ctx: FieldCtx,
    ) {
        let row_bg = if idx % 2 == 0 {
            Color32::from_black_alpha(12)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect.expand2(egui::vec2(4.0, 2.0)), 4.0, row_bg);
        let id = ui.id().with((id_prefix, def_id, path.to_owned(), idx));
        let resp = ui.interact(rect, id, egui::Sense::click());
        let key = FieldKey {
            instance_address,
            field_def_id: def_id,
        };
        if self.selected_fields.contains(&key) {
            ui.painter().rect_filled(
                rect.expand2(egui::vec2(4.0, 2.0)),
                4.0,
                Color32::from_white_alpha(18),
            );
            ui.painter().rect_stroke(
                rect.expand2(egui::vec2(4.0, 2.0)),
                4.0,
                egui::Stroke::new(1.5, Color32::from_rgb(100, 160, 255)),
            );
        }
        if resp.hovered() {
            ui.painter().rect_stroke(
                rect.expand2(egui::vec2(4.0, 2.0)),
                4.0,
                egui::Stroke::new(1.0, Color32::from_white_alpha(12)),
            );
        }
        if resp.clicked() {
            self.update_selection_for_click(ui, instance_address, idx, def_ids, def_id);
        }
        self.context_menu_for_field(&resp, ctx);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_pointer_field(
        &mut self,
        ui: &mut Ui,
        instance_address: u64,
        instance_class_id: u64,
        handle: Option<Arc<AppHandle>>,
        mem_ptr: *mut MemoryStructure,
        path: &mut Vec<usize>,
        idx: usize,
        field: &mut crate::memory::MemoryField,
        class_def: &ClassDefinition,
        def_ids: &[u64],
    ) {
        let fd_opt = class_def.fields.get(idx);
        let def_id = *def_ids.get(idx).unwrap_or(&0);
        let ptr_target = fd_opt.and_then(|fd| fd.pointer_target.clone());
        if matches!(ptr_target, Some(PointerTarget::ClassId(_))) {
            let offset_from_class = field.address.saturating_sub(instance_address);
            let mut header = format!(
                "+0x{:04X}  0x{:08X}    {}: Pointer",
                offset_from_class,
                field.address,
                fd_opt.and_then(|fd| fd.name.clone()).unwrap_or_default()
            );
            if let Some(PointerTarget::ClassId(cid)) = &ptr_target {
                let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                    if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                        cd.name.clone()
                    } else {
                        format!("#{}", cid)
                    }
                } else {
                    format!("#{}", cid)
                };
                header.push_str(&format!(" -> {}", label));
            }
            if let Some(h) = &handle {
                if let Ok(ptr) = h.read_sized::<u64>(field.address) {
                    header.push_str(&format!(" (-> 0x{ptr:016X})"));
                    if ptr != 0 {
                        match &ptr_target {
                            Some(PointerTarget::ClassId(cid)) => {
                                let ms = unsafe { &mut *mem_ptr };
                                if let Some(class_def) = ms.class_registry.get_by_id(*cid).cloned()
                                {
                                    let mut nested = ClassInstance::new(
                                        fd_opt.and_then(|fd| fd.name.clone()).unwrap_or_default(),
                                        ptr,
                                        class_def,
                                    );
                                    ms.bind_nested_for_instance(&mut nested);
                                    field.nested_instance = Some(nested);
                                } else {
                                    field.nested_instance = None;
                                }
                            }
                            _ => {
                                field.nested_instance = None;
                            }
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
                        self.render_field_name_inline_editor(
                            ui,
                            mem_ptr,
                            instance_class_id,
                            instance_address,
                            def_id,
                            idx,
                            fd_opt.and_then(|fd| fd.name.clone()),
                            true,
                        );
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
                owner_class_id: instance_class_id,
                field_index: idx,
                instance_address,
                address: field.address,
                value_preview: None,
            };
            if collapsing.header_response.clicked() {
                self.update_selection_for_click(ui, instance_address, idx, def_ids, def_id);
            }
            self.context_menu_for_field(&collapsing.header_response, ctx);
        } else if matches!(ptr_target, Some(PointerTarget::Array { .. })) {
            let mut header = {
                let offset_from_class = field.address.saturating_sub(instance_address);
                let mut h = format!(
                    "+0x{:04X}  0x{:08X}    {}: Pointer -> Array",
                    offset_from_class,
                    field.address,
                    fd_opt.and_then(|fd| fd.name.clone()).unwrap_or_default()
                );
                if let Some(hd) = &handle {
                    if let Ok(ptr) = hd.read_sized::<u64>(field.address) {
                        h.push_str(&format!(" (-> 0x{ptr:016X})"));
                    }
                }
                h
            };
            if let Some(PointerTarget::Array { element, length }) = &ptr_target {
                let desc = match element.as_ref() {
                    PointerTarget::FieldType(t) => format!("{}", t),
                    PointerTarget::EnumId(eid) => {
                        if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                            if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                ed.name.clone()
                            } else {
                                format!("#{}", eid)
                            }
                        } else {
                            format!("#{}", eid)
                        }
                    }
                    PointerTarget::ClassId(cid) => {
                        if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                            if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                                cd.name.clone()
                            } else {
                                format!("#{}", cid)
                            }
                        } else {
                            format!("#{}", cid)
                        }
                    }
                    PointerTarget::Array { .. } => String::from("Array"),
                };
                header.push_str(&format!(" [{}] {}", length, desc));
            }
            let collapsing = egui::CollapsingHeader::new(header)
                .default_open(false)
                .id_source(("ptr_arr_field", def_id, path.clone()))
                .show(ui, |ui| {
                    if let (Some(hd), Some(PointerTarget::Array { element, length })) =
                        (handle.as_ref(), &ptr_target)
                    {
                        if let Ok(ptr) = hd.read_sized::<u64>(field.address) {
                            if ptr != 0 {
                                let len = *length as usize;
                                match element.as_ref() {
                                    PointerTarget::FieldType(t) => {
                                        let elem_size = t.get_size();
                                        for i in 0..len {
                                            let elem_addr = ptr + (i as u64) * elem_size;
                                            let val = match t {
                                                FieldType::Hex64 => hd
                                                    .read_sized::<u64>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("0x{v:016X}")),
                                                FieldType::Hex32 => hd
                                                    .read_sized::<u32>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("0x{v:08X}")),
                                                FieldType::Hex16 => hd
                                                    .read_sized::<u16>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("0x{v:04X}")),
                                                FieldType::Hex8 => hd
                                                    .read_sized::<u8>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("0x{v:02X}")),
                                                FieldType::UInt64 => hd
                                                    .read_sized::<u64>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::UInt32 => hd
                                                    .read_sized::<u32>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::UInt16 => hd
                                                    .read_sized::<u16>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::UInt8 => hd
                                                    .read_sized::<u8>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::Int64 => hd
                                                    .read_sized::<i64>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::Int32 => hd
                                                    .read_sized::<i32>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::Int16 => hd
                                                    .read_sized::<i16>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::Int8 => hd
                                                    .read_sized::<i8>(elem_addr)
                                                    .ok()
                                                    .map(|v| v.to_string()),
                                                FieldType::Bool => {
                                                    hd.read_sized::<u8>(elem_addr).ok().map(|v| {
                                                        if v != 0 {
                                                            "true".to_string()
                                                        } else {
                                                            "false".to_string()
                                                        }
                                                    })
                                                }
                                                FieldType::Float => hd
                                                    .read_sized::<f32>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("{v}")),
                                                FieldType::Double => hd
                                                    .read_sized::<f64>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("{v}")),
                                                FieldType::Vector2
                                                | FieldType::Vector3
                                                | FieldType::Vector4 => {
                                                    let lenb = t.get_size() as usize;
                                                    let mut buf = vec![0u8; lenb];
                                                    hd.read_slice(elem_addr, buf.as_mut_slice())
                                                        .ok()
                                                        .map(|_| {
                                                            buf.iter()
                                                                .map(|b| format!("{b:02X}"))
                                                                .collect::<Vec<_>>()
                                                                .join(" ")
                                                        })
                                                }
                                                FieldType::Text => {
                                                    hd.read_string(elem_addr, Some(32)).ok()
                                                }
                                                FieldType::TextPointer | FieldType::Pointer => hd
                                                    .read_sized::<u64>(elem_addr)
                                                    .ok()
                                                    .map(|v| format!("0x{v:016X}")),
                                                _ => None,
                                            };
                                            ui.monospace(format!(
                                                "[{}] 0x{:08X}{}",
                                                i,
                                                elem_addr,
                                                val.map(|vv| format!(" = {vv}"))
                                                    .unwrap_or_default()
                                            ));
                                        }
                                    }
                                    PointerTarget::EnumId(eid) => {
                                        if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                            if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                                let sz = ed.default_size;
                                                for i in 0..len {
                                                    let elem_addr = ptr + (i as u64) * (sz as u64);
                                                    let (raw_u64, raw_str) = match sz {
                                                        1 => {
                                                            let v = hd
                                                                .read_sized::<u8>(elem_addr)
                                                                .ok()
                                                                .unwrap_or(0)
                                                                as u64;
                                                            (v, v.to_string())
                                                        }
                                                        2 => {
                                                            let v = hd
                                                                .read_sized::<u16>(elem_addr)
                                                                .ok()
                                                                .unwrap_or(0)
                                                                as u64;
                                                            (v, v.to_string())
                                                        }
                                                        8 => {
                                                            let v = hd
                                                                .read_sized::<u64>(elem_addr)
                                                                .ok()
                                                                .unwrap_or(0);
                                                            (v, v.to_string())
                                                        }
                                                        _ => {
                                                            let v = hd
                                                                .read_sized::<u32>(elem_addr)
                                                                .ok()
                                                                .unwrap_or(0)
                                                                as u64;
                                                            (v, v.to_string())
                                                        }
                                                    };
                                                    let name = ed
                                                        .variants
                                                        .iter()
                                                        .find(|v| (v.value as u64) == raw_u64)
                                                        .map(|v| v.name.clone())
                                                        .unwrap_or(raw_str);
                                                    ui.monospace(format!(
                                                        "[{}] 0x{:08X} = {}",
                                                        i, elem_addr, name
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    PointerTarget::ClassId(cid) => {
                                        if let Some(ms) = unsafe { (mem_ptr).as_mut() } {
                                            if let Some(class_def) =
                                                ms.class_registry.get_by_id(*cid).cloned()
                                            {
                                                let elem_size = class_def.total_size.max(1);
                                                for i in 0..len {
                                                    let elem_addr = ptr + (i as u64) * elem_size;
                                                    let mut nested = ClassInstance::new(
                                                        format!(
                                                            "{}[{}]",
                                                            fd_opt
                                                                .and_then(|fd| fd.name.clone())
                                                                .unwrap_or_default(),
                                                            i
                                                        ),
                                                        elem_addr,
                                                        class_def.clone(),
                                                    );
                                                    ms.bind_nested_for_instance(&mut nested);
                                                    ui.separator();
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "Element [{}] @ 0x{:08X}",
                                                            i, elem_addr
                                                        ))
                                                        .strong(),
                                                    );
                                                    path.push(idx);
                                                    self.render_instance(
                                                        ui,
                                                        &mut nested,
                                                        handle.clone(),
                                                        mem_ptr,
                                                        path,
                                                    );
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
                owner_class_id: instance_class_id,
                field_index: idx,
                instance_address,
                address: field.address,
                value_preview: None,
            };
            if collapsing.header_response.clicked() {
                self.update_selection_for_click(ui, instance_address, idx, def_ids, def_id);
            }
            self.context_menu_for_field(&collapsing.header_response, ctx);
        } else {
            let inner = ui.horizontal(|ui| {
                let offset_from_class = field.address.saturating_sub(instance_address);
                ui.monospace(format!(
                    "+0x{:04X}  0x{:08X}",
                    offset_from_class, field.address
                ));
                if let Some(name) = fd_opt.and_then(|fd| fd.name.clone()) {
                    self.render_field_name_inline_editor(
                        ui,
                        mem_ptr,
                        instance_class_id,
                        instance_address,
                        def_id,
                        idx,
                        Some(name),
                        false,
                    );
                    let ptr_target = fd_opt.and_then(|fd| fd.pointer_target.clone());
                    let type_label = match &ptr_target {
                        Some(PointerTarget::FieldType(t)) => {
                            format!(": {} -> {}", FieldType::Pointer, t)
                        }
                        Some(PointerTarget::ClassId(cid)) => {
                            let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                                    cd.name.clone()
                                } else {
                                    format!("#{}", cid)
                                }
                            } else {
                                format!("#{}", cid)
                            };
                            format!(": {} -> {}", FieldType::Pointer, label)
                        }
                        Some(PointerTarget::EnumId(eid)) => {
                            let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                    ed.name.clone()
                                } else {
                                    format!("#{}", eid)
                                }
                            } else {
                                format!("#{}", eid)
                            };
                            format!(": {} -> {}", FieldType::Pointer, label)
                        }
                        Some(PointerTarget::Array { element, length }) => match element.as_ref() {
                            PointerTarget::FieldType(t) => {
                                format!(": {} -> Array [{}] {}", FieldType::Pointer, length, t)
                            }
                            PointerTarget::EnumId(eid) => {
                                let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                    if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                        ed.name.clone()
                                    } else {
                                        format!("#{}", eid)
                                    }
                                } else {
                                    format!("#{}", eid)
                                };
                                format!(": {} -> Array [{}] {}", FieldType::Pointer, length, label)
                            }
                            PointerTarget::ClassId(cid) => {
                                let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                    if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                                        cd.name.clone()
                                    } else {
                                        format!("#{}", cid)
                                    }
                                } else {
                                    format!("#{}", cid)
                                };
                                format!(": {} -> Array [{}] {}", FieldType::Pointer, length, label)
                            }
                            PointerTarget::Array { .. } => {
                                String::from(": Pointer -> Array [..] Array")
                            }
                        },
                        None => format!(": {}", FieldType::Pointer),
                    };
                    ui.colored_label(Color32::from_rgb(170, 190, 255), type_label);
                } else {
                    let ptr_target = fd_opt.and_then(|fd| fd.pointer_target.clone());
                    let type_label = match &ptr_target {
                        Some(PointerTarget::FieldType(t)) => {
                            format!("{} -> {}", FieldType::Pointer, t)
                        }
                        Some(PointerTarget::ClassId(cid)) => {
                            let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                                    cd.name.clone()
                                } else {
                                    format!("#{}", cid)
                                }
                            } else {
                                format!("#{}", cid)
                            };
                            format!("{} -> {}", FieldType::Pointer, label)
                        }
                        Some(PointerTarget::EnumId(eid)) => {
                            let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                    ed.name.clone()
                                } else {
                                    format!("#{}", eid)
                                }
                            } else {
                                format!("#{}", eid)
                            };
                            format!("{} -> {}", FieldType::Pointer, label)
                        }
                        Some(PointerTarget::Array { element, length }) => match element.as_ref() {
                            PointerTarget::FieldType(t) => {
                                format!("{} -> Array [{}] {}", FieldType::Pointer, length, t)
                            }
                            PointerTarget::EnumId(eid) => {
                                let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                    if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                        ed.name.clone()
                                    } else {
                                        format!("#{}", eid)
                                    }
                                } else {
                                    format!("#{}", eid)
                                };
                                format!("{} -> Array [{}] {}", FieldType::Pointer, length, label)
                            }
                            PointerTarget::ClassId(cid) => {
                                let label = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                                    if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                                        cd.name.clone()
                                    } else {
                                        format!("#{}", cid)
                                    }
                                } else {
                                    format!("#{}", cid)
                                };
                                format!("{} -> Array [{}] {}", FieldType::Pointer, length, label)
                            }
                            PointerTarget::Array { .. } => {
                                String::from("Pointer -> Array [..] Array")
                            }
                        },
                        None => format!("{}", FieldType::Pointer),
                    };
                    ui.colored_label(Color32::from_rgb(170, 190, 255), type_label);
                }
                let display_size = FieldType::Pointer.get_size();
                ui.label(RichText::new(format!(" ({} bytes)", display_size)).weak());
                if let Some(val) = field_value_string(handle.clone(), field, &FieldType::Pointer) {
                    ui.monospace(format!("= {val}"));
                }
            });
            let ctx = FieldCtx {
                mem_ptr,
                owner_class_id: instance_class_id,
                field_index: idx,
                instance_address,
                address: field.address,
                value_preview: field_value_string(handle.clone(), field, &FieldType::Pointer),
            };
            self.paint_row_and_handle_selection(
                ui,
                inner.response.rect,
                idx,
                "row_ptr_prim",
                def_id,
                &path.clone(),
                instance_address,
                def_ids,
                ctx,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_array_field(
        &mut self,
        ui: &mut Ui,
        instance_address: u64,
        instance_class_id: u64,
        handle: Option<Arc<AppHandle>>,
        mem_ptr: *mut MemoryStructure,
        path: &mut Vec<usize>,
        idx: usize,
        field: &mut crate::memory::MemoryField,
        class_def: &ClassDefinition,
        def_ids: &[u64],
    ) {
        let (header_text, len_u32) = if let Some(fd) = class_def.fields.get(idx) {
            let len = fd.array_length.unwrap_or(0);
            let desc = match &fd.array_element {
                Some(PointerTarget::FieldType(t)) => format!("{}", t),
                Some(PointerTarget::EnumId(eid)) => {
                    if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                        if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                            ed.name.clone()
                        } else {
                            format!("#{}", eid)
                        }
                    } else {
                        format!("#{}", eid)
                    }
                }
                Some(PointerTarget::ClassId(cid)) => {
                    if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                        if let Some(cd) = ms.class_registry.get_by_id(*cid) {
                            cd.name.clone()
                        } else {
                            format!("#{}", cid)
                        }
                    } else {
                        format!("#{}", cid)
                    }
                }
                Some(PointerTarget::Array { .. }) => String::from("Array"),
                None => String::from("<elem?>"),
            };
            (
                format!(
                    "0x{:08X}    {}: Array -> [{}] {}",
                    field.address,
                    fd.name.clone().unwrap_or_default(),
                    len,
                    desc
                ),
                len,
            )
        } else {
            (
                format!(
                    "0x{:08X}    {}: Array",
                    field.address,
                    class_def
                        .fields
                        .get(idx)
                        .and_then(|fd| fd.name.clone())
                        .unwrap_or_default()
                ),
                0,
            )
        };

        let def_id = *def_ids.get(idx).unwrap_or(&0);
        let collapsing = egui::CollapsingHeader::new(header_text)
            .default_open(false)
            .id_source(("arr_field", def_id, path.clone()))
            .show(ui, |ui| {
                if let Some(fd) = class_def.fields.get(idx) {
                    let len = len_u32 as usize;
                    match &fd.array_element {
                        Some(PointerTarget::FieldType(t)) => {
                            if let Some(h) = &handle {
                                let elem_size = t.get_size();
                                for i in 0..len {
                                    let elem_addr = field.address + (i as u64) * elem_size;
                                    let offset_from_class =
                                        elem_addr.saturating_sub(instance_address);
                                    let val = match t {
                                        FieldType::Hex64 => h
                                            .read_sized::<u64>(elem_addr)
                                            .ok()
                                            .map(|v| format!("0x{v:016X}")),
                                        FieldType::Hex32 => h
                                            .read_sized::<u32>(elem_addr)
                                            .ok()
                                            .map(|v| format!("0x{v:08X}")),
                                        FieldType::Hex16 => h
                                            .read_sized::<u16>(elem_addr)
                                            .ok()
                                            .map(|v| format!("0x{v:04X}")),
                                        FieldType::Hex8 => h
                                            .read_sized::<u8>(elem_addr)
                                            .ok()
                                            .map(|v| format!("0x{v:02X}")),
                                        FieldType::UInt64 => h
                                            .read_sized::<u64>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::UInt32 => h
                                            .read_sized::<u32>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::UInt16 => h
                                            .read_sized::<u16>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::UInt8 => h
                                            .read_sized::<u8>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::Int64 => h
                                            .read_sized::<i64>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::Int32 => h
                                            .read_sized::<i32>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::Int16 => h
                                            .read_sized::<i16>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::Int8 => h
                                            .read_sized::<i8>(elem_addr)
                                            .ok()
                                            .map(|v| v.to_string()),
                                        FieldType::Bool => {
                                            h.read_sized::<u8>(elem_addr).ok().map(|v| {
                                                if v != 0 {
                                                    "true".to_string()
                                                } else {
                                                    "false".to_string()
                                                }
                                            })
                                        }
                                        FieldType::Float => h
                                            .read_sized::<f32>(elem_addr)
                                            .ok()
                                            .map(|v| format!("{v}")),
                                        FieldType::Double => h
                                            .read_sized::<f64>(elem_addr)
                                            .ok()
                                            .map(|v| format!("{v}")),
                                        FieldType::Vector2
                                        | FieldType::Vector3
                                        | FieldType::Vector4 => {
                                            let lenb = t.get_size() as usize;
                                            let mut buf = vec![0u8; lenb];
                                            h.read_slice(elem_addr, buf.as_mut_slice()).ok().map(
                                                |_| {
                                                    buf.iter()
                                                        .map(|b| format!("{b:02X}"))
                                                        .collect::<Vec<_>>()
                                                        .join(" ")
                                                },
                                            )
                                        }
                                        FieldType::Text => h.read_string(elem_addr, Some(32)).ok(),
                                        FieldType::TextPointer | FieldType::Pointer => h
                                            .read_sized::<u64>(elem_addr)
                                            .ok()
                                            .map(|v| format!("0x{v:016X}")),
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
                        Some(PointerTarget::EnumId(eid)) => {
                            if let (Some(h), Some(ms)) =
                                (handle.as_ref(), unsafe { (mem_ptr).as_ref() })
                            {
                                if let Some(ed) = ms.enum_registry.get_by_id(*eid) {
                                    let sz = ed.default_size;
                                    for i in 0..len {
                                        let elem_addr = field.address + (i as u64) * (sz as u64);
                                        let offset_from_class =
                                            elem_addr.saturating_sub(instance_address);
                                        let (raw_u64, raw_str) = match sz {
                                            1 => {
                                                let v =
                                                    h.read_sized::<u8>(elem_addr).ok().unwrap_or(0)
                                                        as u64;
                                                (v, v.to_string())
                                            }
                                            2 => {
                                                let v = h
                                                    .read_sized::<u16>(elem_addr)
                                                    .ok()
                                                    .unwrap_or(0)
                                                    as u64;
                                                (v, v.to_string())
                                            }
                                            8 => {
                                                let v = h
                                                    .read_sized::<u64>(elem_addr)
                                                    .ok()
                                                    .unwrap_or(0);
                                                (v, v.to_string())
                                            }
                                            _ => {
                                                let v = h
                                                    .read_sized::<u32>(elem_addr)
                                                    .ok()
                                                    .unwrap_or(0)
                                                    as u64;
                                                (v, v.to_string())
                                            }
                                        };
                                        let name = ed
                                            .variants
                                            .iter()
                                            .find(|v| (v.value as u64) == raw_u64)
                                            .map(|v| v.name.clone())
                                            .unwrap_or(raw_str);
                                        ui.monospace(format!(
                                            "+0x{:04X}  0x{:08X}  [{}] = {}",
                                            offset_from_class, elem_addr, i, name
                                        ));
                                    }
                                }
                            }
                        }
                        Some(PointerTarget::Array { .. }) => {
                            ui.monospace("<nested array rendering not supported>");
                        }
                        Some(PointerTarget::ClassId(cid)) => {
                            if let Some(ms) = unsafe { (mem_ptr).as_mut() } {
                                if let Some(class_def) = ms.class_registry.get_by_id(*cid).cloned()
                                {
                                    let elem_size = class_def.total_size.max(1);
                                    for i in 0..len {
                                        let elem_addr = field.address + (i as u64) * elem_size;
                                        let mut nested = ClassInstance::new(
                                            format!("{}[{}]", class_def.name, i),
                                            elem_addr,
                                            class_def.clone(),
                                        );
                                        ms.bind_nested_for_instance(&mut nested);
                                        ui.separator();
                                        ui.label(
                                            RichText::new(format!(
                                                "Element [{}] @ 0x{:08X}",
                                                i, elem_addr
                                            ))
                                            .strong(),
                                        );
                                        path.push(idx);
                                        path.push(i);
                                        self.render_instance(
                                            ui,
                                            &mut nested,
                                            handle.clone(),
                                            mem_ptr,
                                            path,
                                        );
                                        path.pop();
                                        path.pop();
                                    }
                                }
                            }
                        }
                        None => {
                            ui.monospace("<no element type set>");
                        }
                    }
                }
            });

        let ctx = FieldCtx {
            mem_ptr,
            owner_class_id: instance_class_id,
            field_index: idx,
            instance_address,
            address: field.address,
            value_preview: None,
        };
        if collapsing.header_response.clicked() {
            self.update_selection_for_click(ui, instance_address, idx, def_ids, def_id);
        }
        self.context_menu_for_field(&collapsing.header_response, ctx);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_class_instance_field(
        &mut self,
        ui: &mut Ui,
        instance_address: u64,
        instance_class_id: u64,
        handle: Option<Arc<AppHandle>>,
        mem_ptr: *mut MemoryStructure,
        path: &mut Vec<usize>,
        idx: usize,
        field: &mut crate::memory::MemoryField,
        class_def: &ClassDefinition,
        def_ids: &[u64],
    ) {
        let fd_opt = class_def.fields.get(idx);
        let (fname_display, cname_display) = if let Some(nested) = &field.nested_instance {
            (
                fd_opt.and_then(|fd| fd.name.clone()).unwrap_or_default(),
                unsafe { &*mem_ptr }
                    .class_registry
                    .get(nested.class_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| format!("#{}", nested.class_id)),
            )
        } else {
            (
                fd_opt.and_then(|fd| fd.name.clone()).unwrap_or_default(),
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
                    self.render_field_name_inline_editor(
                        ui,
                        mem_ptr,
                        instance_class_id,
                        instance_address,
                        def_id,
                        idx,
                        fd_opt.and_then(|fd| fd.name.clone()),
                        true,
                    );
                    if let Some(nested) = field.nested_instance.as_mut() {
                        ui.label("Type:");
                        let tkey = FieldKey { instance_address, field_def_id: def_id };
                        let current_type = nested.class_id;
                        let available = unsafe { (*mem_ptr).class_registry.get_class_ids() };
                        let mut selected = self.class_type_buffers.get(&tkey).cloned().unwrap_or(current_type);
                        egui::ComboBox::from_id_source(("ci_type_combo", tkey))
                            .selected_text(selected.to_string())
                            .show_ui(ui, |ui| {
                                for id in available { ui.selectable_value(&mut selected, id, id.to_string()); }
                            });
                        if selected != current_type {
                            let ms = unsafe { &mut *mem_ptr };
                            if ms.would_create_cycle(instance_class_id, selected) {
                                self.class_type_buffers.remove(&tkey);
                                self.cycle_error_text = format!("Changing '{current_type}' -> '{selected}' would create a class cycle.");
                                self.cycle_error_open = true;
                            } else if !ms.class_registry.contains(selected) {
                                self.class_type_buffers.remove(&tkey);
                            } else {
                                let selected_cid_opt = ms.class_registry.get_by_id(selected).map(|d| d.id);
                                if let Some(defm) = ms.class_registry.get_mut(instance_class_id) {
                                    if let Some(fd) = defm.fields.iter_mut().find(|fd| fd.id == def_id) {
                                        if let Some(cid) = selected_cid_opt { fd.class_id = Some(cid); }
                                        self.schedule_rebuild();
                                        self.class_type_buffers.remove(&tkey);
                                    }
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
            owner_class_id: instance_class_id,
            field_index: idx,
            instance_address,
            address: field.address,
            value_preview: None,
        };
        if collapsing.header_response.clicked() {
            self.update_selection_for_click(ui, instance_address, idx, def_ids, def_id);
        }
        self.context_menu_for_field(&collapsing.header_response, ctx);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_simple_field(
        &mut self,
        ui: &mut Ui,
        instance_address: u64,
        instance_class_id: u64,
        handle: Option<Arc<AppHandle>>,
        mem_ptr: *mut MemoryStructure,
        path: &mut [usize],
        idx: usize,
        field: &mut crate::memory::MemoryField,
        class_def: &ClassDefinition,
        def_ids: &[u64],
        field_type: &FieldType,
    ) {
        let inner = ui.horizontal(|ui| {
            let offset_from_class = field.address.saturating_sub(instance_address);
            ui.monospace(format!(
                "+0x{:04X}  0x{:08X}",
                offset_from_class, field.address
            ));
            let def_id = class_def.fields.get(idx).map(|fd| fd.id).unwrap_or(0);
            if let Some(name) = class_def.fields.get(idx).and_then(|fd| fd.name.clone()) {
                self.render_field_name_inline_editor(
                    ui,
                    mem_ptr,
                    instance_class_id,
                    instance_address,
                    def_id,
                    idx,
                    Some(name),
                    false,
                );
                let enum_suffix = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                    enum_suffix_for_field(class_def, field, ms)
                } else {
                    String::new()
                };
                ui.colored_label(
                    Color32::from_rgb(170, 190, 255),
                    format!(": {}{}", field_type, enum_suffix),
                );
            } else {
                let enum_suffix = if let Some(ms) = unsafe { (mem_ptr).as_ref() } {
                    enum_suffix_for_field(class_def, field, ms)
                } else {
                    String::new()
                };
                ui.colored_label(
                    Color32::from_rgb(170, 190, 255),
                    format!("{}{}", field_type, enum_suffix),
                );
            }
            let display_size = self.compute_display_size_for(field_type, class_def, field, mem_ptr);
            ui.label(RichText::new(format!(" ({} bytes)", display_size)).weak());
            let value_str = if matches!(field_type, FieldType::Enum) {
                if let (Some(h), Some(ms)) = (handle.as_ref(), unsafe { (mem_ptr).as_ref() }) {
                    enum_value_string(h, class_def, field, ms)
                } else {
                    None
                }
            } else {
                field_value_string(handle.clone(), field, field_type)
            };
            if let Some(val) = value_str {
                ui.monospace(format!("= {val}"));
            }
        });
        let def_id = *def_ids.get(idx).unwrap_or(&0);
        let ctx = FieldCtx {
            mem_ptr,
            owner_class_id: instance_class_id,
            field_index: idx,
            instance_address,
            address: field.address,
            value_preview: field_value_string(handle.clone(), field, field_type),
        };
        self.paint_row_and_handle_selection(
            ui,
            inner.response.rect,
            idx,
            "row_field",
            def_id,
            &path.to_owned(),
            instance_address,
            def_ids,
            ctx,
        );
    }

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
        let class_def = unsafe { &*mem_ptr }
            .class_registry
            .get_by_id(instance.class_id)
            .unwrap();
        let def_ids: Vec<u64> = class_def.fields.iter().map(|fd| fd.id).collect();
        for (idx, field) in instance.fields.iter_mut().enumerate() {
            let fd_opt = class_def.fields.get(idx);
            let field_type = fd_opt
                .map(|fd| fd.field_type.clone())
                .unwrap_or(FieldType::Hex8);
            match field_type {
                FieldType::Pointer => self.render_pointer_field(
                    ui,
                    instance.address,
                    instance.class_id,
                    handle.clone(),
                    mem_ptr,
                    path,
                    idx,
                    field,
                    class_def,
                    &def_ids,
                ),
                FieldType::Array => self.render_array_field(
                    ui,
                    instance.address,
                    instance.class_id,
                    handle.clone(),
                    mem_ptr,
                    path,
                    idx,
                    field,
                    class_def,
                    &def_ids,
                ),
                FieldType::ClassInstance => self.render_class_instance_field(
                    ui,
                    instance.address,
                    instance.class_id,
                    handle.clone(),
                    mem_ptr,
                    path,
                    idx,
                    field,
                    class_def,
                    &def_ids,
                ),
                _ => self.render_simple_field(
                    ui,
                    instance.address,
                    instance.class_id,
                    handle.clone(),
                    mem_ptr,
                    path,
                    idx,
                    field,
                    class_def,
                    &def_ids,
                    &field_type,
                ),
            }
        }
    }
}
