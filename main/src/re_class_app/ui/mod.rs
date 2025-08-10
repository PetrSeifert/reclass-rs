use std::collections::HashSet;

use eframe::egui::{
    self,
    CentralPanel,
    Color32,
    Context,
    ScrollArea,
    SidePanel,
    TopBottomPanel,
};

use super::ReClassApp;

mod header;
mod memory_view;
mod process;
mod theme;

pub struct ReClassGui {
    app: ReClassApp,
    attach_window_open: bool,
    process_filter: String,
    modules_window_open: bool,
    needs_rebuild: bool,
    field_name_buffers: std::collections::HashMap<memory_view::FieldKey, String>,
    class_type_buffers: std::collections::HashMap<memory_view::FieldKey, String>,
    root_class_type_buffer: Option<String>,
    root_address_buffer: Option<String>,
    cycle_error_open: bool,
    cycle_error_text: String,
    rename_dialog_open: bool,
    rename_target_name: String,
    rename_buffer: String,
    rename_error_text: Option<String>,
    theme_applied: bool,
    ui_scale: f32,
    class_filter: String,
}

impl ReClassGui {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            app: ReClassApp::new()?,
            attach_window_open: false,
            process_filter: String::new(),
            modules_window_open: false,
            needs_rebuild: false,
            field_name_buffers: std::collections::HashMap::new(),
            class_type_buffers: std::collections::HashMap::new(),
            root_class_type_buffer: None,
            root_address_buffer: None,
            cycle_error_open: false,
            cycle_error_text: String::new(),
            rename_dialog_open: false,
            rename_target_name: String::new(),
            rename_buffer: String::new(),
            rename_error_text: None,
            theme_applied: false,
            ui_scale: 1.0,
            class_filter: String::new(),
        })
    }

    fn schedule_rebuild(&mut self) {
        self.needs_rebuild = true;
    }
}

impl eframe::App for ReClassGui {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Apply theme & style once
        self.apply_theme_once(ctx);

        // Top bar
        let top_fill = ctx.style().visuals.faint_bg_color;
        let top_stroke = egui::Stroke::new(1.0, Color32::from_black_alpha(60));
        TopBottomPanel::top("top")
            .frame(
                egui::Frame::default()
                    .fill(top_fill)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .stroke(top_stroke),
            )
            .show(ctx, |ui| {
                self.header_bar(ui);
            });

        // Left: class definitions
        SidePanel::left("class_defs_panel").resizable(true).default_width(240.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Class Definitions");
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.text_edit_singleline(&mut self.class_filter);
                if ui.button("Clear").clicked() {
                    self.class_filter.clear();
                }
            });
            ui.separator();
            let snapshot = self.app.get_memory_structure().map(|ms| {
                let names = ms.class_registry.get_class_names();
                let root_name = ms.root_class.class_definition.name.clone();
                let mut referenced: HashSet<String> = HashSet::new();
                for cname in &names {
                    if let Some(def) = ms.class_registry.get(cname) {
                        for f in &def.fields {
                            if f.field_type == crate::memory::FieldType::ClassInstance {
                                if let Some(ref cn) = f.class_name { referenced.insert(cn.clone()); }
                            }
                        }
                    }
                }
                let unused: Vec<String> = names
                    .iter()
                    .filter(|n| {
                        if *n == &root_name { return false; }
                        if referenced.contains(*n) { return false; }
                        if let Some(def) = ms.class_registry.get(n) {
                            if def.fields.len() == 1 {
                                let f = &def.fields[0];
                                return f.field_type == crate::memory::FieldType::Hex64 && f.name.is_none();
                            }
                        }
                        false
                    })
                    .cloned()
                    .collect();
                (names, root_name, referenced, unused)
            });

            if let Some((mut names, root_name, _referenced, unused)) = snapshot {
                if !self.class_filter.trim().is_empty() {
                    let needle = self.class_filter.to_lowercase();
                    names.retain(|n| n.to_lowercase().contains(&needle));
                }
                if ui
                    .add_enabled(!unused.is_empty(), egui::Button::new("Delete unused"))
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
                        let mut button = egui::Button::new(&cname).min_size(egui::vec2(ui.available_width(), 0.0));
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
                            if ui.button("Set as root").clicked() {
                                if let Some(ms_mut) = self.app.get_memory_structure_mut() {
                                    if ms_mut.set_root_class_by_name(&cname) {
                                        self.needs_rebuild = true;
                                    }
                                }
                                ui.close_menu();
                            }
                        });
                    }
                });
            } else {
                ui.label("No structure loaded");
            }
        });

        // Center
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

        // Apply deferred rebuilds
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
