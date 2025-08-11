use eframe::egui;

use crate::{
    memory::{
        ClassDefinition,
        FieldType,
        MemoryStructure,
        PointerTarget,
    },
    re_class_app::ReClassGui,
};

pub(super) struct FieldCtx {
    pub mem_ptr: *mut MemoryStructure,
    pub owner_class_name: String,
    pub field_index: usize,
    pub address: u64,
    pub value_preview: Option<String>,
}

impl ReClassGui {
    pub(super) fn context_menu_for_field(&mut self, response: &egui::Response, ctx: FieldCtx) {
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
                for &(label, n) in &[
                    ("4 bytes", 4usize),
                    ("8 bytes", 8),
                    ("64 bytes", 64),
                    ("256 bytes", 256),
                    ("1024 bytes", 1024),
                    ("2048 bytes", 2048),
                    ("4096 bytes", 4096),
                ] {
                    if ui.button(label).clicked() {
                        self.add_n_bytes_at_end(&ctx, n);
                        ui.close_menu();
                    }
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
                for &(label, n) in &[
                    ("4 bytes", 4usize),
                    ("8 bytes", 8),
                    ("64 bytes", 64),
                    ("256 bytes", 256),
                    ("1024 bytes", 1024),
                    ("2048 bytes", 2048),
                    ("4096 bytes", 4096),
                ] {
                    if ui.button(label).clicked() {
                        self.insert_n_bytes_here(&ctx, n);
                        ui.close_menu();
                    }
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
                    FieldType::Vector2,
                    FieldType::Vector3,
                    FieldType::Vector4,
                    FieldType::Text,
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
                                        let unique_name = {
                                            let base = base_name;
                                            let mut name = base.to_string();
                                            let mut idx: usize = 1;
                                            while ms.class_registry.contains(&name) {
                                                name = format!("{base}_{idx}");
                                                idx += 1;
                                            }
                                            name
                                        };
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
                let unique_name = {
                    let base = base_name;
                    let mut name = base.to_string();
                    let mut idx: usize = 1;
                    while ms.class_registry.contains(&name) {
                        name = format!("{base}_{idx}");
                        idx += 1;
                    }
                    name
                };
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
}
