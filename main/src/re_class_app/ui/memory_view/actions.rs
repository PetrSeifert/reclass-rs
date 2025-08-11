use super::context_menu::FieldCtx;
use crate::{
    memory::FieldType,
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
}
