use crate::gui::gui::ReClassGui;
use crate::memory::example::create_example_memory_structure;

mod gui;
mod re_class_app;
mod memory;

fn main() -> Result<(), eframe::Error> {
    ReClassGui::run_gui()
}
