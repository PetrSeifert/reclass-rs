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
    pub instance_address: u64,
    pub address: u64,
    pub value_preview: Option<String>,
}

impl ReClassGui {
    pub(super) fn context_menu_for_field(&mut self, response: &egui::Response, ctx: FieldCtx) {
        response.context_menu(|ui| {
            // If multiple fields are selected in the same instance/class, show only bulk operations
            let multi_in_same_instance = self
                .selected_instance_address
                .map(|addr| addr == ctx.instance_address)
                .unwrap_or(false);
            if multi_in_same_instance && !self.selected_fields.is_empty() {
                let owner = ctx.owner_class_name.clone();
                let selected_ids: std::collections::HashSet<u64> = self
                    .selected_fields
                    .iter()
                    .filter(|k| k.instance_address == ctx.instance_address)
                    .map(|k| k.field_def_id)
                    .collect();
                if selected_ids.len() > 1 {
                    ui.label("Selection actions");
                    if ui.button("Remove fields").clicked() {
                        self.remove_selected_fields(ctx.mem_ptr, &owner, &selected_ids);
                        ui.close_menu();
                        return;
                    }
                    ui.menu_button("Change types", |ui| {
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
                            FieldType::Array,
                        ] {
                            let label = format!("{t:?}");
                            if ui.button(label).clicked() {
                                self.change_selected_fields_type(
                                    ctx.mem_ptr,
                                    &owner,
                                    &selected_ids,
                                    t.clone(),
                                );
                                ui.close_menu();
                            }
                        }
                    });
                    if ui.button("Create class instances").clicked() {
                        self.create_class_instances_for_selected(
                            ctx.mem_ptr,
                            &owner,
                            &selected_ids,
                        );
                        ui.close_menu();
                        return;
                    }
                    // Do not show single-field actions when multi-select is active
                    return;
                }
            }
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
                    FieldType::Array,
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
                            } else if t == FieldType::Array {
                                if let Some(fd) = def.fields.get_mut(ctx.field_index) {
                                    if fd.array_element.is_none() {
                                        fd.array_element =
                                            Some(PointerTarget::FieldType(FieldType::Hex8));
                                    }
                                    if fd.array_length.is_none() {
                                        fd.array_length = Some(1);
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
                // Snapshot current field type and metadata immutably
                let (field_type_opt, current_enum, current_len): (Option<FieldType>, Option<String>, u32) = {
                    if let Some(def_ref) = ms.class_registry.get(&ctx.owner_class_name) {
                        if let Some(fd_ref) = def_ref.fields.get(ctx.field_index) {
                            (Some(fd_ref.field_type.clone()), fd_ref.enum_name.clone(), fd_ref.array_length.unwrap_or(0))
                        } else {
                            (None, None, 0)
                        }
                    } else {
                        (None, None, 0)
                    }
                };
                if matches!(field_type_opt, Some(FieldType::Enum)) {
                            ui.separator();
                            ui.label("Enum:");
                    let mut selected = current_enum.clone().unwrap_or_else(|| "<none>".to_string());
                            egui::ComboBox::from_id_source((
                                "enum_combo",
                                ctx.owner_class_name.clone(),
                                ctx.field_index,
                            ))
                    .selected_text(selected.clone())
                            .show_ui(ui, |ui| {
                                for n in ms.enum_registry.get_enum_names() {
                            ui.selectable_value(&mut selected, n.clone(), n);
                        }
                    });
                    if current_enum.as_deref() != Some(&selected) && ms.enum_registry.contains(&selected) {
                        if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                            if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                fdm.enum_name = Some(selected);
                            }
                        }
                        self.schedule_rebuild();
                    }
                } else if matches!(field_type_opt, Some(FieldType::Array)) {
                    ui.separator();
                    ui.label("Array element type:");
                    ui.menu_button("Select element", |ui| {
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
                                    if t == FieldType::Enum {
                                        let names = ms.enum_registry.get_enum_names();
                                        if let Some(first) = names.into_iter().next() {
                                            if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                                if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                    fdm.array_element = Some(PointerTarget::EnumName(first));
                                                }
                                            }
                                        } else {
                                            if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                                if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                    fdm.array_element = Some(PointerTarget::FieldType(FieldType::UInt32));
                                                }
                                            }
                                        }
                                    } else {
                                        if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                            if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                fdm.array_element = Some(PointerTarget::FieldType(t));
                                            }
                                        }
                                    }
                                    self.schedule_rebuild();
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Enum", |ui| {
                            let names = ms.enum_registry.get_enum_names();
                            for name in names {
                                if ui.button(name.clone()).clicked() {
                                    if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                        if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                            fdm.array_element = Some(PointerTarget::EnumName(name));
                                        }
                                    }
                                    self.schedule_rebuild();
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Class", |ui| {
                            if ui.button("Create new class here").clicked() {
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
                                if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                    if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                        fdm.array_element = Some(PointerTarget::ClassName(unique_name));
                                    }
                                }
                                self.schedule_rebuild();
                                ui.close_menu();
                            }
                            ui.separator();
                            let names = ms.class_registry.get_class_names();
                            for name in names {
                                if ui.button(name.clone()).clicked() {
                                    if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                        if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                            fdm.array_element = Some(PointerTarget::ClassName(name));
                                        }
                                    }
                                    self.schedule_rebuild();
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Length:");
                        let mut len_val: u32 = current_len;
                        let resp = ui.add(egui::DragValue::new(&mut len_val).clamp_range(0..=1_048_576));
                        if resp.changed() {
                            if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                    fdm.array_length = Some(len_val);
                                }
                            }
                            self.schedule_rebuild();
                        }
                    });
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
                                ui.menu_button("Array", |ui| {
                                    // Default to Hex8 x 1
                                    if ui.button("Primitive element").clicked() {
                                        let ms = unsafe { &mut *ctx.mem_ptr };
                                        if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                            if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                fdm.pointer_target = Some(PointerTarget::Array {
                                                    element: Box::new(PointerTarget::FieldType(FieldType::Hex8)),
                                                    length: 1,
                                                });
                                            }
                                        }
                                        self.schedule_rebuild();
                                        ui.close_menu();
                                    }
                                    ui.menu_button("Enum element", |ui| {
                                        let names = unsafe { (*ctx.mem_ptr).enum_registry.get_enum_names() };
                                        for name in names {
                                            if ui.button(name.clone()).clicked() {
                                                let ms = unsafe { &mut *ctx.mem_ptr };
                                                if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                                    if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                        fdm.pointer_target = Some(PointerTarget::Array {
                                                            element: Box::new(PointerTarget::EnumName(name)),
                                                            length: 1,
                                                        });
                                                    }
                                                }
                                                self.schedule_rebuild();
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                    ui.menu_button("Class element", |ui| {
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
                                            if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                                if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                    fdm.pointer_target = Some(PointerTarget::Array {
                                                        element: Box::new(PointerTarget::ClassName(unique_name)),
                                                        length: 1,
                                                    });
                                                }
                                            }
                                            self.schedule_rebuild();
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        let names = unsafe { (*ctx.mem_ptr).class_registry.get_class_names() };
                                        for name in names {
                                            if ui.button(name.clone()).clicked() {
                                                let ms = unsafe { &mut *ctx.mem_ptr };
                                                if let Some(defm) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                                                    if let Some(fdm) = defm.fields.get_mut(ctx.field_index) {
                                                        fdm.pointer_target = Some(PointerTarget::Array {
                                                            element: Box::new(PointerTarget::ClassName(name)),
                                                            length: 1,
                                                        });
                                                    }
                                                }
                                                self.schedule_rebuild();
                                                ui.close_menu();
                                            }
                                        }
                                    });
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
