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
pub mod memory_view;
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
    enum_window_open: bool,
    enum_window_target: Option<String>,
    enum_value_buffers: std::collections::HashMap<(String, usize), String>,
    bytes_custom_buffer: String,
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
            enum_window_open: false,
            enum_window_target: None,
            enum_value_buffers: std::collections::HashMap::new(),
            bytes_custom_buffer: String::new(),
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

        // Left: class and enum definitions
        SidePanel::left("class_defs_panel").resizable(true).default_width(260.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Definitions");
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
                            } else if f.field_type == crate::memory::FieldType::Pointer {
                                if let Some(crate::memory::PointerTarget::ClassName(ref cn)) = f.pointer_target {
                                    referenced.insert(cn.clone());
                                }
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
                let enum_names = ms.enum_registry.get_enum_names();
                (names, root_name, referenced, unused, enum_names)
            });

            if let Some((mut names, root_name, _referenced, unused, enum_names)) = snapshot {
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
                ui.label("Classes");
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
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Enums");
                    if ui.button("New").clicked() {
                        if let Some(ms) = self.app.get_memory_structure_mut() {
                            let base = "NewEnum";
                            let mut name = base.to_string();
                            let mut idx: usize = 1;
                            while ms.enum_registry.contains(&name) {
                                name = format!("{base}{idx}");
                                idx += 1;
                            }
                            ms.enum_registry.register(crate::memory::EnumDefinition::new(name));
                        }
                    }
                });
                ScrollArea::vertical().id_source("enum_defs_scroll").show(ui, |ui| {
                    for ename in enum_names {
                        let mut resp = ui.label(ename.clone());
                        resp = resp.on_hover_text("Right-click to edit");
                        resp.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                // reuse rename dialog for enums
                                self.rename_dialog_open = true;
                                self.rename_target_name = ename.clone();
                                self.rename_buffer = ename.clone();
                                self.rename_error_text = None;
                                ui.close_menu();
                            }
                            if ui.button("Open editor").clicked() {
                                self.enum_window_open = true;
                                self.enum_window_target = Some(ename.clone());
                                ui.close_menu();
                            }
                            // Delete only if not referenced
                            if ui.button("Delete").clicked() {
                                if let Some(ms) = self.app.get_memory_structure_mut() {
                                    if !ms.is_enum_referenced(&ename) {
                                        ms.enum_registry.remove(&ename);
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
            egui::Window::new("Rename Definition")
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
                                // Try class rename first
                                if ms.class_registry.contains(&self.rename_target_name) {
                                    if ms.class_registry.contains(&new_name) {
                                        self.rename_error_text = Some(
                                            "A class with this name already exists.".to_string(),
                                        );
                                    } else {
                                        let ok =
                                            ms.rename_class(&self.rename_target_name, &new_name);
                                        if ok {
                                            self.needs_rebuild = true;
                                            should_close = true;
                                            self.rename_error_text = None;
                                        } else {
                                            self.rename_error_text =
                                                Some("Rename failed.".to_string());
                                        }
                                    }
                                } else if ms.enum_registry.contains(&self.rename_target_name) {
                                    if ms.enum_registry.contains(&new_name) {
                                        self.rename_error_text = Some(
                                            "An enum with this name already exists.".to_string(),
                                        );
                                    } else {
                                        let ok =
                                            ms.rename_enum(&self.rename_target_name, &new_name);
                                        if ok {
                                            self.needs_rebuild = true;
                                            should_close = true;
                                            self.rename_error_text = None;
                                        } else {
                                            self.rename_error_text =
                                                Some("Rename failed.".to_string());
                                        }
                                    }
                                } else {
                                    self.rename_error_text = Some("Rename failed.".to_string());
                                }
                            }
                        }
                    });
                });
            if should_close {
                self.rename_dialog_open = false;
            }
        }

        // Enum editor window
        if self.enum_window_open {
            let target = self.enum_window_target.clone();
            let mut should_close = false;
            egui::Window::new("Enum Editor")
                .open(&mut self.enum_window_open)
                .resizable(true)
                .show(ctx, |ui| {
                    if let (Some(ms), Some(ename)) = (self.app.get_memory_structure_mut(), target) {
                        if let Some(def) = ms.enum_registry.get_mut(&ename) {
                            ui.horizontal(|ui| {
                                ui.label(format!("Enum: {}", def.name));
                                if ui.button("Close").clicked() {
                                    should_close = true;
                                }
                            });
                            ui.separator();
                            egui::Grid::new("enum_variants_grid")
                                .num_columns(3)
                                .spacing(egui::vec2(8.0, 4.0))
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label("Name");
                                    ui.label("Value");
                                    ui.end_row();

                                    let mut delete_index: Option<usize> = None;
                                    for (idx, var) in def.variants.iter_mut().enumerate() {
                                        let key = (def.name.clone(), idx);
                                        // Auto-width name editor
                                        let mut name_buf = var.name.clone();
                                        let display = if name_buf.is_empty() {
                                            " ".to_string()
                                        } else {
                                            name_buf.clone()
                                        };
                                        let galley = ui.painter().layout_no_wrap(
                                            display,
                                            egui::TextStyle::Body.resolve(ui.style()),
                                            egui::Color32::WHITE,
                                        );
                                        let width = galley.rect.width() + 12.0;
                                        let resp_name = ui.add_sized(
                                            [width, ui.text_style_height(&egui::TextStyle::Body)],
                                            egui::TextEdit::singleline(&mut name_buf),
                                        );
                                        if resp_name.lost_focus() || resp_name.changed() {
                                            var.name = name_buf;
                                        }

                                        let val_buf = self
                                            .enum_value_buffers
                                            .entry(key.clone())
                                            .or_insert_with(|| var.value.to_string());
                                        let resp_val = ui.text_edit_singleline(val_buf);
                                        if resp_val.lost_focus()
                                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                        {
                                            if let Ok(parsed) = val_buf.parse::<u32>() {
                                                var.value = parsed;
                                            }
                                        }

                                        if ui.button("Delete").clicked() {
                                            delete_index = Some(idx);
                                        }
                                        ui.end_row();
                                    }
                                    if let Some(di) = delete_index {
                                        def.variants.remove(di);
                                        self.enum_value_buffers.retain(|(n, _), _| n != &def.name);
                                    }
                                });
                            ui.separator();
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("Size:");
                                let mut size = def.default_size;
                                egui::ComboBox::from_id_source(("enum_default_size", def.id))
                                    .selected_text(format!("{size} bytes"))
                                    .show_ui(ui, |ui| {
                                        for s in [1u8, 2, 4, 8] {
                                            ui.selectable_value(&mut size, s, format!("{s} bytes"));
                                        }
                                    });
                                if size != def.default_size {
                                    def.default_size = size;
                                    // Recompute structure layout immediately
                                    self.needs_rebuild = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                let mut flags = def.is_flags;
                                if ui
                                    .checkbox(&mut flags, "Flags")
                                    .on_hover_text(
                                        "When enabled, variant values should be powers of two",
                                    )
                                    .changed()
                                {
                                    def.is_flags = flags;
                                    if def.is_flags {
                                        // Recompute to powers of two from current ordering
                                        let mut v: u32 = 1;
                                        for var in &mut def.variants {
                                            var.value = v;
                                            if v == 0 {
                                                break;
                                            }
                                            v = v.saturating_mul(2);
                                        }
                                    }
                                }
                            });
                            if ui
                                .button("Add value")
                                .on_hover_text("Append a new variant with next id")
                                .clicked()
                            {
                                let next_val = if def.is_flags {
                                    // next power of two
                                    let mut v: u32 = 1;
                                    let used: std::collections::HashSet<u32> =
                                        def.variants.iter().map(|vv| vv.value).collect();
                                    while used.contains(&v) {
                                        if v == 0 {
                                            break;
                                        }
                                        v = v.saturating_mul(2);
                                    }
                                    if v == 0 {
                                        1
                                    } else {
                                        v
                                    }
                                } else {
                                    def.variants
                                        .iter()
                                        .map(|v| v.value)
                                        .max()
                                        .unwrap_or(0)
                                        .saturating_add(1)
                                };
                                def.variants.push(crate::memory::EnumVariant {
                                    name: format!("Value{next_val}"),
                                    value: next_val,
                                });
                            }
                        } else {
                            ui.label("Enum not found");
                        }
                    } else {
                        ui.label("No enum selected");
                    }
                });
            if should_close {
                self.enum_window_open = false;
                self.enum_window_target = None;
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
