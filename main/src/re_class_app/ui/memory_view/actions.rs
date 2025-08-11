use std::collections::HashSet;

use super::context_menu::FieldCtx;
use crate::{
    memory::{
        ClassDefinition,
        FieldType,
        MemoryStructure,
        PointerTarget,
    },
    re_class_app::ReClassGui,
};

impl ReClassGui {
    pub(crate) fn add_n_bytes_at_end(&mut self, ctx: &FieldCtx, num_bytes: usize) {
        if num_bytes == 0 {
            return;
        }
        if let Some(ms) = self.app.get_memory_structure_mut() {
            if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                let mut remaining = num_bytes;
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
    }

    pub(crate) fn insert_n_bytes_here(&mut self, ctx: &FieldCtx, num_bytes: usize) {
        if num_bytes == 0 {
            return;
        }
        if let Some(ms) = self.app.get_memory_structure_mut() {
            if let Some(def) = ms.class_registry.get_mut(&ctx.owner_class_name) {
                let mut remaining = num_bytes;
                let mut insert_index = ctx.field_index;
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

    pub(crate) fn remove_selected_fields(
        &mut self,
        mem_ptr: *mut MemoryStructure,
        owner_class_name: &str,
        selected_field_ids: &HashSet<u64>,
    ) {
        if selected_field_ids.is_empty() {
            return;
        }
        let ms = unsafe { &mut *mem_ptr };
        if let Some(def) = ms.class_registry.get_mut(owner_class_name) {
            let total = def.fields.len();
            let mut indices: Vec<usize> = def
                .fields
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    if selected_field_ids.contains(&f.id) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
            // Ensure we don't remove all fields
            if indices.len() >= total {
                return;
            }
            indices.sort_unstable_by(|a, b| b.cmp(a));
            for idx in indices {
                def.remove_field_at(idx);
            }
            self.schedule_rebuild();
        }
        // Clear selection after operation
        self.selected_fields
            .retain(|k| !selected_field_ids.contains(&k.field_def_id));
        if self.selected_fields.is_empty() {
            self.selected_instance_address = None;
            self.selection_anchor = None;
        }
    }

    pub(crate) fn change_selected_fields_type(
        &mut self,
        mem_ptr: *mut MemoryStructure,
        owner_class_name: &str,
        selected_field_ids: &HashSet<u64>,
        new_type: FieldType,
    ) {
        let ms = unsafe { &mut *mem_ptr };
        if let Some(def) = ms.class_registry.get_mut(owner_class_name) {
            // Map ids to indices each pass since set_field_type_at may update structure but keeps order
            let indices: Vec<usize> = def
                .fields
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    if selected_field_ids.contains(&f.id) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
            for idx in indices {
                def.set_field_type_at(idx, new_type.clone());
                if new_type == FieldType::Pointer {
                    if let Some(fd) = def.fields.get_mut(idx) {
                        fd.pointer_target = Some(PointerTarget::FieldType(FieldType::Hex64));
                    }
                } else if new_type == FieldType::Enum {
                    if let Some(fd) = def.fields.get_mut(idx) {
                        let names = ms.enum_registry.get_enum_names();
                        if let Some(first) = names.into_iter().next() {
                            fd.enum_name = Some(first);
                        } else {
                            fd.enum_name = None;
                        }
                    }
                }
            }
            self.schedule_rebuild();
        }
    }

    pub(crate) fn create_class_instances_for_selected(
        &mut self,
        mem_ptr: *mut MemoryStructure,
        owner_class_name: &str,
        selected_field_ids: &HashSet<u64>,
    ) {
        let ms = unsafe { &mut *mem_ptr };
        // Collect indices with immutable borrow first
        let indices: Vec<usize> = if let Some(def_ref) = ms.class_registry.get(owner_class_name) {
            def_ref
                .fields
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    if selected_field_ids.contains(&f.id) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            return;
        };

        // Plan unique names and new class defs without holding mutable borrows
        let mut existing: std::collections::HashSet<String> =
            ms.class_registry.get_class_names().into_iter().collect();
        let mut planned: Vec<(usize, String, ClassDefinition)> = Vec::with_capacity(indices.len());
        for idx in indices.into_iter() {
            let base = "NewClass";
            let mut name = base.to_string();
            let mut counter: usize = 1;
            while existing.contains(&name) {
                name = format!("{base}_{counter}");
                counter += 1;
            }
            existing.insert(name.clone());
            let mut new_def = ClassDefinition::new(name.clone());
            new_def.add_hex_field(FieldType::Hex64);
            planned.push((idx, name, new_def));
        }

        // Register all new class definitions
        for (_, _, defn) in planned.iter().cloned() {
            ms.class_registry.register(defn);
        }

        // Now update owner definition fields
        if let Some(def_mut) = ms.class_registry.get_mut(owner_class_name) {
            for (idx, cname, _) in planned.into_iter() {
                def_mut.set_field_type_at(idx, FieldType::ClassInstance);
                if let Some(fd) = def_mut.fields.get_mut(idx) {
                    fd.class_name = Some(cname);
                }
            }
        }
        self.schedule_rebuild();
    }
}
