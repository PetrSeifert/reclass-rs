use std::time::{
    Duration,
    Instant,
};

use eframe::egui;

use crate::{
    gui::{
        memory_elements::{
            ClassElement,
            FieldType,
            MemoryElement,
        },
        rendering::{
            ContextMenuAction,
            RenderingManager,
        },
        selection::SelectionManager,
        windows::WindowManager,
    },
    re_class_app::ReClassApp,
};

pub struct ReClassGui {
    app: ReClassApp,
    window_manager: WindowManager,
    memory_address_input: String,
    memory_elements: Vec<MemoryElement>,
    last_read_time: Instant,
    rendering_manager: RenderingManager,
    selection_manager: SelectionManager,
    field_counter: u32,
}

impl Default for ReClassGui {
    fn default() -> Self {
        Self {
            app: ReClassApp::default(),
            window_manager: WindowManager::new(),
            memory_address_input: String::new(),
            memory_elements: vec![
                MemoryElement::new_class(0x0, 0x8, "MyClass".to_string(), ClassElement::Root),
                MemoryElement::new_field(0x0, "".to_string(), FieldType::Hex64),
            ],
            last_read_time: Instant::now(),
            rendering_manager: RenderingManager::new(),
            selection_manager: SelectionManager::new(),
            field_counter: 0,
        }
    }
}

impl eframe::App for ReClassGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_read_time.elapsed() >= Duration::from_secs(1) {
            self.auto_read_all_elements();
            self.last_read_time = Instant::now();
        }

        if let Some((field_index, ctrl_pressed, shift_pressed)) =
            self.selection_manager.pending_field_click
        {
            self.selection_manager.handle_field_selection(
                &self.memory_elements,
                field_index,
                ctrl_pressed,
                shift_pressed,
            );
            self.selection_manager.pending_field_click = None; // Clear the pending click
        }

        // Top menu bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("ReClass-rs Memory Browser");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("List Modules").clicked() {
                        self.window_manager.show_modules_window = true;
                    }
                    if ui.button("Attach to Process").clicked() {
                        self.window_manager.show_process_window = true;
                        // Fetch processes when opening the window
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

                // Memory Structure Address Input
                ui.heading("Memory Structure");
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

                // Memory Structure Viewer
                let handle = self.app.handle.as_ref().map(|h| h.as_ref());
                self.rendering_manager.render_memory_structure(
                    ui,
                    &mut self.memory_elements,
                    &mut self.selection_manager,
                    handle,
                );

                // Context menu
                if let Some(action) = self.rendering_manager.render_context_menu(
                    ctx,
                    &mut self.memory_elements,
                    &self.selection_manager,
                ) {
                    match action {
                        ContextMenuAction::AddElementAfter(index) => self.add_element_after(index),
                        ContextMenuAction::DeleteElement(index) => self.delete_element(index),
                        ContextMenuAction::DeleteMultipleElements(indices) => {
                            self.delete_multiple_elements(indices)
                        }
                        ContextMenuAction::ChangeType(index) => self.change_field_type(index),
                        ContextMenuAction::ChangeMultipleTypes(indices) => {
                            self.change_multiple_field_types(indices)
                        }
                        ContextMenuAction::FollowPointer(index) => self.follow_pointer(index),
                        ContextMenuAction::NewClass(address, name) => {
                            self.add_root_class(address, name)
                        }
                        ContextMenuAction::CopyAddress(index) => {
                            if let Some(element) = self.memory_elements.get(index) {
                                let address_str = format!("0x{:X}", element.address);
                                if let Err(e) = arboard::Clipboard::new()
                                    .and_then(|mut clipboard| clipboard.set_text(address_str))
                                {
                                    log::error!("Failed to copy address to clipboard: {}", e);
                                }
                            }
                        }
                        ContextMenuAction::CopyData(index) => {
                            if let Some(element) = self.memory_elements.get(index) {
                                if let Some(_data) = &element.data {
                                    let data_str = element.get_formatted_value(
                                        self.app.handle.as_ref().map(|h| h.as_ref()),
                                    );
                                    if let Err(e) = arboard::Clipboard::new()
                                        .and_then(|mut clipboard| clipboard.set_text(data_str))
                                    {
                                        log::error!("Failed to copy data to clipboard: {}", e);
                                    }
                                }
                            }
                        }
                        ContextMenuAction::SetAsPointer(_) => {
                            // TODO: Implement pointer functionality
                        }
                    }
                }

                // Type selection window
                if let Some(selected_type) = self
                    .rendering_manager
                    .render_type_selection_window(ctx, &mut self.memory_elements)
                {
                    if !self.rendering_manager.type_selection_indices.is_empty() {
                        self.change_field_types_to(
                            self.rendering_manager.type_selection_indices.clone(),
                            selected_type,
                        );
                        self.rendering_manager.type_selection_indices.clear();
                    }
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
    fn auto_read_all_elements(&mut self) {
        if self.app.handle.is_none() {
            return;
        }

        for element in &mut self.memory_elements {
            if let Some(ClassElement::Root) | Some(ClassElement::Pointer) = &element.class_type {
                continue;
            }

            element.error = None;

            if let Some(handle) = &self.app.handle {
                let mut buffer = vec![0u8; element.size as usize];
                match handle.read_slice(element.address, &mut buffer) {
                    Ok(_) => {
                        element.data = Some(buffer);
                    }
                    Err(e) => {
                        element.error = Some(format!("Failed to read memory: {}", e));
                    }
                }
            }
        }
    }

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
            if let Some(first_element) = self.memory_elements.first_mut() {
                first_element.address = address;

                for i in 1..self.memory_elements.len() {
                    let prev_element = &self.memory_elements[i - 1];

                    let current_address = if let Some(ClassElement::Root)
                    | Some(ClassElement::Pointer) =
                        &prev_element.class_type
                    {
                        prev_element.address
                    } else {
                        prev_element.address + prev_element.size
                    };

                    self.memory_elements[i].address = current_address;
                }
            }
        }
    }

    fn add_element_after(&mut self, index: usize) {
        if index < self.memory_elements.len() {
            let current_element = &self.memory_elements[index];

            let mut class_start_index = 0;
            let mut class_end_index = self.memory_elements.len();

            if let Some(ClassElement::Root) | Some(ClassElement::Pointer) =
                &current_element.class_type
            {
                for i in (index + 1)..self.memory_elements.len() {
                    if let Some(ClassElement::Field) = &self.memory_elements[i].class_type {
                        class_end_index = i + 1;
                    } else {
                        class_end_index = i;
                        break;
                    }
                }
                class_start_index = index;
            } else {
                for i in (0..index).rev() {
                    if let Some(ClassElement::Root) | Some(ClassElement::Pointer) =
                        &self.memory_elements[i].class_type
                    {
                        class_start_index = i;
                        for j in (i + 1)..self.memory_elements.len() {
                            if let Some(ClassElement::Field) = &self.memory_elements[j].class_type {
                                class_end_index = j + 1;
                            } else {
                                class_end_index = j;
                                break;
                            }
                        }
                        break;
                    }
                }
            }

            let mut last_field_index = None;
            for i in class_start_index..class_end_index {
                if let Some(ClassElement::Field) = &self.memory_elements[i].class_type {
                    last_field_index = Some(i);
                }
            }

            let new_address = if let Some(last_field_idx) = last_field_index {
                let last_field = &self.memory_elements[last_field_idx];
                last_field.address + last_field.size
            } else {
                let class_element = &self.memory_elements[class_start_index];
                class_element.address
            };

            let new_element = if current_element.class_type.is_some() {
                MemoryElement::new_field(new_address, "".to_string(), FieldType::Hex64)
            } else {
                MemoryElement::new(new_address, 0x8, format!("Element_{}", self.field_counter))
            };

            self.memory_elements.insert(class_end_index, new_element);
        }
    }

    fn delete_element(&mut self, index: usize) {
        if self.memory_elements.len() <= 1 {
            return;
        }

        if index < self.memory_elements.len() {
            let element = &self.memory_elements[index];

            if let Some(ClassElement::Root) | Some(ClassElement::Pointer) = &element.class_type {
                let mut field_count = 0;
                for i in (index + 1)..self.memory_elements.len() {
                    if let Some(ClassElement::Field) = &self.memory_elements[i].class_type {
                        field_count += 1;
                    } else {
                        break;
                    }
                }

                if field_count == 0 {
                    return;
                }
            }

            if let Some(ClassElement::Field) = &element.class_type {
                let mut class_index = None;
                for i in (0..index).rev() {
                    if let Some(ClassElement::Root) | Some(ClassElement::Pointer) =
                        &self.memory_elements[i].class_type
                    {
                        class_index = Some(i);
                        break;
                    }
                }

                if let Some(class_idx) = class_index {
                    let mut remaining_fields = 0;
                    for i in (class_idx + 1)..self.memory_elements.len() {
                        if i != index
                            && matches!(
                                &self.memory_elements[i].class_type,
                                Some(ClassElement::Field)
                            )
                        {
                            remaining_fields += 1;
                        }
                    }

                    if remaining_fields == 0 {
                        return;
                    }
                }
            }

            self.memory_elements.remove(index);
            self.recalculate_addresses_after_deletion(index);
        }
    }

    fn delete_multiple_elements(&mut self, indices: Vec<usize>) {
        let sorted_indices = indices
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let mut to_delete = Vec::new();

        for i in (0..self.memory_elements.len()).rev() {
            if sorted_indices.contains(&i) {
                to_delete.push(i);
            }
        }

        for index in to_delete {
            self.delete_element(index);
        }

        self.selection_manager.clear_selections();
    }

    fn recalculate_addresses_after_deletion(&mut self, deleted_index: usize) {
        for i in deleted_index..self.memory_elements.len() {
            if i == 0 {
                continue;
            }

            let prev_element = &self.memory_elements[i - 1];

            let current_address = if let Some(ClassElement::Root) | Some(ClassElement::Pointer) =
                &prev_element.class_type
            {
                prev_element.address
            } else {
                prev_element.address + prev_element.size
            };

            self.memory_elements[i].address = current_address;
        }
    }

    fn follow_pointer(&mut self, index: usize) {
        if let Some(element) = self.memory_elements.get(index) {
            if let Some(data) = &element.data {
                if data.len() >= 8 {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&data[0..8]);
                    let pointer_address = u64::from_le_bytes(bytes);

                    self.add_pointer_element(
                        pointer_address,
                        format!("Pointer_{}", self.field_counter),
                    );
                    self.field_counter += 1;
                }
            }
        }
    }

    fn add_pointer_element(&mut self, address: u64, name: String) {
        let new_pointer = MemoryElement::new_class(address, 0x8, name, ClassElement::Pointer);
        let new_field = MemoryElement::new_field(address, "".to_string(), FieldType::Hex64);
        self.memory_elements.push(new_pointer);
        self.memory_elements.push(new_field);
    }

    fn add_root_class(&mut self, address: u64, name: String) {
        let new_class = MemoryElement::new_class(address, 0x8, name, ClassElement::Root);
        let new_field = MemoryElement::new_field(address, "".to_string(), FieldType::Hex64);
        self.memory_elements.insert(0, new_class);
        self.memory_elements.insert(1, new_field);
    }

    fn change_field_type(&mut self, index: usize) {
        if index < self.memory_elements.len() {
            self.rendering_manager.show_type_selection = true;
            self.rendering_manager.type_selection_indices = vec![index];
        }
    }

    fn change_multiple_field_types(&mut self, indices: Vec<usize>) {
        self.rendering_manager.show_type_selection = true;
        self.rendering_manager.type_selection_indices = indices;
    }

    fn change_field_types_to(&mut self, indices: Vec<usize>, new_type: FieldType) {
        for &index in &indices {
            if index < self.memory_elements.len() {
                let element = &mut self.memory_elements[index];

                if let Some(old_type) = &element.field_type {
                    if old_type.is_hex_type() && !new_type.is_hex_type() {
                        self.field_counter += 1;
                    }
                }

                element.field_type = Some(new_type.clone());
                element.size = new_type.get_size();

                if new_type.is_hex_type() {
                    element.name = "".to_string();
                } else {
                    element.name = format!("F{:08X}", self.field_counter);
                }
            }
        }

        self.recalculate_addresses_after_deletion(0);
    }

    pub fn run_gui() -> Result<(), eframe::Error> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(800.0, 600.0)),
            ..Default::default()
        };

        eframe::run_native(
            "ReClass-rs Memory Browser",
            options,
            Box::new(|_cc| Box::<ReClassGui>::default()),
        )
    }
}
