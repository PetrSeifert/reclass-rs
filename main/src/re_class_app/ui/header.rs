use eframe::egui::{
    self,
    Layout,
    RichText,
    TextStyle,
    Ui,
};

use super::ReClassGui;

impl ReClassGui {
    pub(super) fn header_bar(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("Attach to Process").min_size(egui::vec2(140.0, 0.0)))
                .on_hover_text("Open the process list and attach by PID")
                .clicked()
            {
                self.attach_window_open = true;
                let _ = self.app.fetch_processes();
            }

            if let Some(selected) = &self.app.process_state.selected_process {
                let txt = RichText::new(format!(
                    "Attached: {}  (PID {})",
                    selected.get_image_base_name().unwrap_or("Unknown"),
                    selected.process_id
                ))
                .strong()
                .text_style(TextStyle::Button);
                ui.label(txt);
                if ui
                    .add(egui::Button::new("Modules").min_size(egui::vec2(84.0, 0.0)))
                    .on_hover_text("View loaded modules for the attached process")
                    .clicked()
                {
                    let _ = self.app.fetch_modules(selected.process_id);
                    self.modules_window_open = true;
                }
                if ui
                    .add(egui::Button::new("Signatures").min_size(egui::vec2(100.0, 0.0)))
                    .on_hover_text("Define and resolve signatures to entry offsets")
                    .clicked()
                {
                    self.signatures_window_open = true;
                }
            } else {
                ui.label(
                    RichText::new("Not attached")
                        .weak()
                        .text_style(TextStyle::Button),
                );
            }

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{}%", (self.ui_scale * 100.0).round()))
                        .weak()
                        .text_style(TextStyle::Button),
                );
                if ui
                    .add(egui::Button::new("A+").min_size(egui::vec2(28.0, 0.0)))
                    .on_hover_text("Increase UI scale")
                    .clicked()
                {
                    self.ui_scale = (self.ui_scale + 0.05).clamp(0.8, 1.8);
                    ui.ctx().set_pixels_per_point(self.ui_scale);
                }
                if ui
                    .add(egui::Button::new("A-").min_size(egui::vec2(28.0, 0.0)))
                    .on_hover_text("Decrease UI scale")
                    .clicked()
                {
                    self.ui_scale = (self.ui_scale - 0.05).clamp(0.8, 1.8);
                    ui.ctx().set_pixels_per_point(self.ui_scale);
                }
            });
        });
    }
}
