use crate::gui::gui::ReClassGui;

mod gui;
mod re_class_app;

fn main() -> Result<(), eframe::Error> {
    ReClassGui::run_gui()
}
