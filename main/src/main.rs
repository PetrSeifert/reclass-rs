use eframe::NativeOptions;

mod memory;
mod re_class_app;

fn main() -> Result<(), anyhow::Error> {
    let native_options = NativeOptions::default();
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
