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

    pub(super) fn render_memory_structure_impl(
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
}
