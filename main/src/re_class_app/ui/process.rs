use eframe::egui::{
    self,
    Context,
    ScrollArea,
};

use super::ReClassGui;

impl ReClassGui {
    pub(super) fn attach_window(&mut self, ctx: &Context) {
        let mut clicked_pid: Option<u32> = None;
        egui::Window::new("Attach to Process")
            .open(&mut self.attach_window_open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut self.process_filter);
                    if ui.button("Clear").clicked() {
                        self.process_filter.clear();
                    }
                    if ui.button("Refresh").clicked() {
                        let _ = self.app.fetch_processes();
                    }
                });
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("process_list_grid")
                        .num_columns(2)
                        .spacing(egui::vec2(12.0, 6.0))
                        .striped(true)
                        .show(ui, |ui| {
                            for process in self.app.get_processes() {
                                let name = process.get_image_base_name().unwrap_or("Unknown");
                                if !self.process_filter.is_empty()
                                    && !name
                                        .to_lowercase()
                                        .contains(&self.process_filter.to_lowercase())
                                {
                                    continue;
                                }
                                ui.label(format!("{} (PID {})", name, process.process_id));
                                if ui
                                    .add_sized([80.0, 24.0], egui::Button::new("Attach"))
                                    .clicked()
                                {
                                    clicked_pid = Some(process.process_id);
                                }
                                ui.end_row();
                            }
                        });
                });
            });

        if let Some(pid) = clicked_pid {
            if let Some(proc_info) = self.app.get_process_by_id(pid) {
                self.app.select_process(*proc_info);
            }
            let _ = self.app.create_handle(pid);
            let _ = self.app.fetch_modules(pid);
            self.attach_window_open = false;
        }
    }

    pub(super) fn modules_window(&mut self, ctx: &Context) {
        let selected_pid = self
            .app
            .process_state
            .selected_process
            .as_ref()
            .map(|p| p.process_id);

        egui::Window::new("Modules")
            .open(&mut self.modules_window_open)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(pid) = selected_pid {
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.text_edit_singleline(&mut self.modules_filter);
                        if ui.button("Clear").clicked() {
                            self.modules_filter.clear();
                        }
                        if ui.button("Refresh").clicked() {
                            let _ = self.app.fetch_modules(pid);
                        }
                    });
                    ui.separator();
                    ScrollArea::vertical().show(ui, |ui| {
                        let needle = self.modules_filter.to_lowercase();
                        for m in self.app.get_modules() {
                            let name = m.get_base_dll_name().unwrap_or("Unknown");
                            if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                                continue;
                            }
                            ui.label(format!(
                                "{} @ 0x{:X} ({} KB)",
                                name,
                                m.base_address,
                                m.module_size / 1024
                            ));
                        }
                    });
                } else {
                    ui.label("Not attached to a process");
                }
            });
    }
}
