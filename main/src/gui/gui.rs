use std::time::{
    Duration,
    Instant,
};

use eframe::egui;

use crate::{
    gui::{
        windows::WindowManager,
        memory_display::{MemoryStructureDisplay, MemoryDetailsPanel, MemoryNode},
    },
    re_class_app::ReClassApp,
    memory::example::create_example_memory_structure,
};

pub struct ReClassGui {
    app: ReClassApp,
    window_manager: WindowManager,
    memory_address_input: String,
    memory_display: MemoryStructureDisplay,
    memory_details: MemoryDetailsPanel,
    selected_class: Option<String>,
}

impl Default for ReClassGui {
    fn default() -> Self {
        Self {
            app: ReClassApp::default(),
            window_manager: WindowManager::new(),
            memory_address_input: String::new(),
            memory_display: MemoryStructureDisplay::new(),
            memory_details: MemoryDetailsPanel::new(),
            selected_class: None,
        }
    }
}

impl eframe::App for ReClassGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top menu bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("ReClass-rs");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("List Modules").clicked() {
                        self.window_manager.show_modules_window = true;
                    }
                    if ui.button("Attach to Process").clicked() {
                        self.window_manager.show_process_window = true;
                        if let Err(e) = self.app.fetch_processes() {
                            log::error!("Failed to fetch processes: {}", e);
                        }
                    }
                });
            });
        });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(selected) = &self.app.process_state.selected_process {
                ui.heading(format!(
                    "Attached to: {} (PID: {})",
                    selected.get_image_base_name().unwrap_or("Unknown"),
                    selected.process_id
                ));

                ui.separator();

                // Memory Structure Controls
                ui.horizontal(|ui| {
                    ui.heading("Memory Structure");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Load Example").clicked() {
                            let example_structure = create_example_memory_structure();
                            self.app.set_memory_structure(example_structure);
                        }
                        if ui.button("Clear").clicked() {
                            self.app.clear_memory_structure();
                            self.memory_display.clear_selection();
                        }
                    });
                });

                // Memory Structure Address Input
                ui.horizontal(|ui| {
                    ui.label("Starting Address (hex):");
                    let response = ui.text_edit_singleline(&mut self.memory_address_input);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.update_memory_structure_address();
                    }
                    if ui.button("Set Address").clicked() {
                        self.update_memory_structure_address();
                    }
                });
                ui.label("Enter starting address in hex format (e.g., 0x12345678 or 12345678)");

                ui.separator();

                // Memory Structure Display
                if let Some(memory_structure) = self.app.get_memory_structure() {
                    // Class selection buttons
                    self.memory_display.render_class_buttons(ui, memory_structure, &mut self.selected_class);
                    
                    // Handle class selection
                    if let Some(selected_class) = &self.selected_class {
                        if ui.button("Create Instance from Selected Class").clicked() {
                            let address = self.memory_address_input.trim()
                                .strip_prefix("0x")
                                .unwrap_or(self.memory_address_input.trim());
                            
                            if let Ok(addr) = u64::from_str_radix(address, 16) {
                                let class_name = selected_class.clone();
                                // Release the immutable borrow before calling mutable method
                                let _ = memory_structure;
                                self.app.create_memory_structure_from_class(&class_name, addr);
                            }
                        }
                    }
                    
                    ui.separator();
                    
                    // Re-get the memory structure after potential changes
                    if let Some(memory_structure) = self.app.get_memory_structure() {
                        // Create a horizontal split for the memory display and details
                        ui.horizontal(|ui| {
                            // Left side - Memory structure tree
                            ui.vertical(|ui| {
                                self.memory_display.render(ui, memory_structure);
                            });
                            
                            // Right side - Details panel
                            ui.separator();
                            ui.vertical(|ui| {
                                self.memory_details.render(ui);
                            });
                        });
                        
                        // Render field editing dialog
                        if let Some(mut memory_structure_mut) = self.app.get_memory_structure_mut() {
                            self.memory_display.render_field_editing(ui, memory_structure_mut);
                        }
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.heading("No Memory Structure Loaded");
                        ui.label("Click 'Load Example' to see a sample memory structure.");
                        ui.label("Or create your own memory structure programmatically.");
                    });
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.heading("Welcome to ReClass-rs");
                    ui.label("Click 'Attach to Process' to begin analyzing a process.");
                });
            }
        });

        self.window_manager
            .render_process_window(ctx, &mut self.app);
        self.window_manager
            .render_modules_window(ctx, &mut self.app);
    }
}

impl ReClassGui {
    fn update_memory_structure_address(&mut self) {
        let address_str = self.memory_address_input.trim();
        if address_str.is_empty() {
            return;
        }

        let clean_address = if let Some(stripped) = address_str.strip_prefix("0x") {
            stripped
        } else {
            address_str
        };

        if let Ok(address) = u64::from_str_radix(clean_address, 16) {
            // Create a simple memory structure at the specified address
            let example_structure = create_example_memory_structure();
            self.app.set_memory_structure(example_structure);
        }
    }
    
    pub fn run_gui() -> Result<(), eframe::Error> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(800.0, 600.0)),
            ..Default::default()
        };

        eframe::run_native(
            "ReClass-rs",
            options,
            Box::new(|_cc| Box::<ReClassGui>::default()),
        )
    }
}
