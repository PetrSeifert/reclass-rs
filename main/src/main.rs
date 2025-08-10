use eframe::{
    egui,
    NativeOptions,
};

mod memory;
mod re_class_app;

fn main() -> Result<(), anyhow::Error> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(egui::vec2(1100.0, 750.0))
            .with_min_inner_size(egui::vec2(900.0, 600.0)),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ReClass RS",
        native_options,
        Box::new(|_cc| Box::new(re_class_app::ReClassGui::new().expect("init gui"))),
    );
    match res {
        Ok(()) => Ok(()),
        Err(err) => Err(anyhow::anyhow!(format!("{err}"))),
    }
}
