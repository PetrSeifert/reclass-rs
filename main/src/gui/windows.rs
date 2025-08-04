use eframe::egui;

use crate::re_class_app::ReClassApp;

pub struct WindowManager {
    pub show_process_window: bool,
    pub show_modules_window: bool,
    pub process_filter: String,
    pub modules_filter: String,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            show_process_window: false,
            show_modules_window: false,
            process_filter: String::new(),
            modules_filter: String::new(),
        }
    }

    pub fn render_process_window(&mut self, ctx: &egui::Context, app: &mut ReClassApp) {
        if self.show_process_window {
            let mut window_open = true;
            egui::Window::new("Select Process")
                .open(&mut window_open)
                .resizable(true)
                .default_size(egui::vec2(600.0, 400.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Refresh Processes").clicked() {
                            if let Err(e) = app.fetch_processes() {
                                log::error!("Failed to fetch processes: {}", e);
                            }
                        }
                        let total_count = app.get_processes().len();
                        let filtered_count = if self.process_filter.is_empty() {
                            total_count
                        } else {
                            app.get_processes_snapshot()
                                .into_iter()
                                .filter(|(_, name)| {
                                    name.to_lowercase()
                                        .contains(&self.process_filter.to_lowercase())
                                })
                                .count()
                        };
                        if self.process_filter.is_empty() {
                            ui.label(format!("Found {} processes", total_count));
                        } else {
                            ui.label(format!(
                                "Found {} processes ({} filtered)",
                                total_count, filtered_count
                            ));
                        }
                    });

                    ui.separator();

                    // Filter input
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.text_edit_singleline(&mut self.process_filter);
                        if ui.button("Clear").clicked() {
                            self.process_filter.clear();
                        }
                    });

                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_source("process_selection_scroll")
                        .max_height(ui.available_height() - 120.0)
                        .show(ui, |ui| {
                            let processes = app.get_processes_snapshot();
                            let filtered_processes: Vec<_> = if self.process_filter.is_empty() {
                                processes.into_iter().collect()
                            } else {
                                processes
                                    .into_iter()
                                    .filter(|(_, name)| {
                                        name.to_lowercase()
                                            .contains(&self.process_filter.to_lowercase())
                                    })
                                    .collect()
                            };

                            for (pid, name) in filtered_processes {
                                let display_text = format!("PID: {} - {}", pid, name);

                                if ui.button(display_text).clicked() {
                                    if let Some(process) = app.get_process_by_id(pid) {
                                        app.select_process(*process);
                                        if let Err(e) = app.create_handle(pid) {
                                            log::error!("Failed to create handle: {}", e);
                                        } else {
                                            if let Err(e) = app.fetch_modules(pid) {
                                                log::error!("Failed to fetch modules: {}", e);
                                            }
                                            self.show_process_window = false;
                                        }
                                    }
                                }
                            }
                        });
                });
            if !window_open {
                self.show_process_window = false;
            }
        }
    }

    pub fn render_modules_window(&mut self, ctx: &egui::Context, app: &mut ReClassApp) {
        if self.show_modules_window {
            let mut window_open = true;
            let modules = app.get_modules();
            let process_info = app
                .process_state
                .selected_process
                .as_ref()
                .map(|p| (p.process_id, p.get_image_base_name().unwrap_or("Unknown")));
            let mut refresh_clicked = false;

            egui::Window::new("Process Modules")
                .open(&mut window_open)
                .resizable(true)
                .default_size(egui::vec2(700.0, 500.0))
                .show(ctx, |ui| {
                    if let Some((process_id, process_name)) = process_info {
                        ui.heading(format!(
                            "Modules for: {} (PID: {})",
                            process_name, process_id
                        ));

                        ui.separator();

                        if !modules.is_empty() {
                            ui.horizontal(|ui| {
                                let total_count = modules.len();
                                let filtered_count = if self.modules_filter.is_empty() {
                                    total_count
                                } else {
                                    modules
                                        .iter()
                                        .filter(|module| {
                                            module
                                                .get_base_dll_name()
                                                .unwrap_or("")
                                                .to_lowercase()
                                                .contains(&self.modules_filter.to_lowercase())
                                        })
                                        .count()
                                };
                                if self.modules_filter.is_empty() {
                                    ui.label(format!("Found {} modules", total_count));
                                } else {
                                    ui.label(format!(
                                        "Found {} modules ({} filtered)",
                                        total_count, filtered_count
                                    ));
                                }
                                if ui.button("Refresh Modules").clicked() {
                                    refresh_clicked = true;
                                }
                            });

                            // Filter input
                            ui.horizontal(|ui| {
                                ui.label("Filter:");
                                ui.text_edit_singleline(&mut self.modules_filter);
                                if ui.button("Clear").clicked() {
                                    self.modules_filter.clear();
                                }
                            });

                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_source("modules_window_scroll")
                                .max_height(ui.available_height() - 140.0)
                                .show(ui, |ui| {
                                    let filtered_modules: Vec<_> = if self.modules_filter.is_empty()
                                    {
                                        modules.into_iter().collect()
                                    } else {
                                        modules
                                            .into_iter()
                                            .filter(|module| {
                                                module
                                                    .get_base_dll_name()
                                                    .unwrap_or("")
                                                    .to_lowercase()
                                                    .contains(&self.modules_filter.to_lowercase())
                                            })
                                            .collect()
                                    };

                                    for module in filtered_modules {
                                        ui.horizontal(|ui| {
                                            ui.label(format!(
                                                "Module: {}",
                                                module.get_base_dll_name().unwrap_or("Unknown")
                                            ));
                                            ui.label(format!("Base: 0x{:X}", module.base_address));
                                            ui.label(format!("Size: 0x{:X}", module.module_size));
                                        });
                                    }
                                });
                        } else {
                            ui.label("No modules found for this process.");
                            if ui.button("Refresh Modules").clicked() {
                                refresh_clicked = true;
                            }
                        }
                    } else {
                        ui.label("No process attached. Please attach to a process first.");
                    }
                });

            if refresh_clicked {
                if let Some(process) = &app.process_state.selected_process {
                    if let Err(e) = app.fetch_modules(process.process_id) {
                        log::error!("Failed to fetch modules: {}", e);
                    }
                }
            }

            if !window_open {
                self.show_modules_window = false;
            }
        }
    }
}
