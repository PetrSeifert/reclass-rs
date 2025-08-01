use eframe::egui;

use crate::re_class_app::ReClassApp;

pub struct WindowManager {
    pub show_process_window: bool,
    pub show_modules_window: bool,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            show_process_window: false,
            show_modules_window: false,
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
                        ui.label(format!("Found {} processes", app.get_processes().len()));
                    });

                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_source("process_selection_scroll")
                        .max_height(ui.available_height() - 80.0)
                        .show(ui, |ui| {
                            let processes = app.get_processes_snapshot();
                            for (pid, name) in processes {
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
                                ui.label(format!("Found {} modules", modules.len()));
                                if ui.button("Refresh Modules").clicked() {
                                    refresh_clicked = true;
                                }
                            });

                            egui::ScrollArea::vertical()
                                .id_source("modules_window_scroll")
                                .max_height(ui.available_height() - 100.0)
                                .show(ui, |ui| {
                                    for module in modules {
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
