use eframe::egui;

use crate::gui::{
    memory_elements::{
        ClassElement,
        FieldType,
        MemoryElement,
    },
    selection::SelectionManager,
};

pub struct RenderingManager {
    pub show_context_menu: bool,
    pub context_menu_element_index: Option<usize>,
    pub context_menu_position: egui::Pos2,
    pub show_type_selection: bool,
    pub type_selection_indices: Vec<usize>,
}

impl RenderingManager {
    pub fn new() -> Self {
        Self {
            show_context_menu: false,
            context_menu_element_index: None,
            context_menu_position: egui::Pos2::ZERO,
            show_type_selection: false,
            type_selection_indices: Vec::new(),
        }
    }

    pub fn render_memory_structure(
        &mut self,
        ui: &mut egui::Ui,
        memory_elements: &mut [MemoryElement],
        selection_manager: &mut SelectionManager,
        handle: Option<&handle::AppHandle>,
    ) {
        let mut class_sizes = Vec::new();
        for (i, element) in memory_elements.iter().enumerate() {
            if let Some(ClassElement::Root) | Some(ClassElement::Pointer) = &element.class_type {
                let mut total_size = 0;
                let mut field_index = i + 1;
                while field_index < memory_elements.len() {
                    let field_element = &memory_elements[field_index];
                    if let Some(ClassElement::Field) = &field_element.class_type {
                        total_size += field_element.size;
                    } else {
                        break;
                    }
                    field_index += 1;
                }
                class_sizes.push(total_size);
            } else {
                class_sizes.push(0);
            }
        }

        egui::ScrollArea::vertical()
            .id_source("memory_structure_scroll")
            .max_height(400.0)
            .show(ui, |ui| {
                let mut i = 0;
                while i < memory_elements.len() {
                    let element = &mut memory_elements[i];

                    let element_id = ui.make_persistent_id(format!("element_{}", i));
                    let element_row_rect = ui.available_rect_before_wrap();
                    let element_row_response =
                        ui.interact(element_row_rect, element_id, egui::Sense::click_and_drag());

                    if element_row_response.clicked() {
                        if self.show_context_menu {
                            self.show_context_menu = false;
                        }

                        let ctrl_pressed = ui.input(|i| i.modifiers.ctrl);
                        let shift_pressed = ui.input(|i| i.modifiers.shift);
                        selection_manager.pending_field_click =
                            Some((i, ctrl_pressed, shift_pressed));
                    }

                    if element_row_response.secondary_clicked() {
                        // Only allow context menu for field and regular elements, not for root/pointer classes
                        if let Some(ClassElement::Field) = &element.class_type {
                            self.show_context_menu = true;
                            self.context_menu_element_index = Some(i);
                            self.context_menu_position =
                                element_row_response.hover_pos().unwrap_or(ui.cursor().min);
                        } else if element.class_type.is_none() {
                            // Regular elements (not part of a class)
                            self.show_context_menu = true;
                            self.context_menu_element_index = Some(i);
                            self.context_menu_position =
                                element_row_response.hover_pos().unwrap_or(ui.cursor().min);
                        }
                        // Root and Pointer classes don't get context menus
                    }

                    if let Some(ClassElement::Root) | Some(ClassElement::Pointer) =
                        &element.class_type
                    {
                        ui.horizontal(|ui| {
                            ui.label("📦");

                            if element.is_editing {
                                let response = ui.text_edit_singleline(&mut element.name);

                                if response.lost_focus()
                                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                                {
                                    element.is_editing = false;
                                }
                            } else {
                                let name_button = egui::Button::new(&element.name)
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE);

                                if ui.add(name_button).double_clicked() {
                                    element.is_editing = true;
                                }
                            }

                            ui.label(":");

                            ui.label(format!("0x{:X}", element.address));
                            ui.label(format!("({} bytes total)", class_sizes[i]));
                        });

                        let mut field_index = i + 1;

                        let ctrl_pressed = ui.input(|i| i.modifiers.ctrl);
                        let shift_pressed = ui.input(|i| i.modifiers.shift);

                        while field_index < memory_elements.len() {
                            let field_element = &mut memory_elements[field_index];

                            if let Some(ClassElement::Field) = &field_element.class_type {
                                let field_id =
                                    ui.make_persistent_id(format!("element_{}", field_index));
                                let field_row_rect = ui.available_rect_before_wrap();
                                let field_row_response = ui.interact(
                                    field_row_rect,
                                    field_id,
                                    egui::Sense::click_and_drag(),
                                );

                                if field_row_response.clicked() {
                                    if self.show_context_menu {
                                        self.show_context_menu = false;
                                    }
                                    selection_manager.pending_field_click =
                                        Some((field_index, ctrl_pressed, shift_pressed));
                                }

                                if field_row_response.secondary_clicked() {
                                    self.show_context_menu = true;
                                    self.context_menu_element_index = Some(field_index);
                                    self.context_menu_position =
                                        field_row_response.hover_pos().unwrap_or(ui.cursor().min);
                                }

                                let is_selected = selection_manager.is_field_selected(field_index);
                                let text_color = if is_selected {
                                    egui::Color32::from_rgb(255, 255, 0) // Yellow for selected
                                } else {
                                    egui::Color32::from_rgb(200, 200, 200) // Gray for normal
                                };

                                ui.horizontal(|ui| {
                                    ui.label("  "); // Indentation

                                    ui.colored_label(text_color, "🔧");

                                    if let Some(field_type) = &field_element.field_type {
                                        if !field_type.is_hex_type() {
                                            if field_element.is_editing {
                                                let response = ui
                                                    .text_edit_singleline(&mut field_element.name);

                                                if response.lost_focus()
                                                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                                    || ui
                                                        .input(|i| i.key_pressed(egui::Key::Escape))
                                                {
                                                    field_element.is_editing = false;
                                                }
                                            } else {
                                                let name_button =
                                                    egui::Button::new(&field_element.name)
                                                        .fill(egui::Color32::TRANSPARENT)
                                                        .stroke(egui::Stroke::NONE);

                                                if ui.add(name_button).double_clicked() {
                                                    field_element.is_editing = true;
                                                }
                                            }
                                        } else {
                                            ui.colored_label(text_color, "");
                                        }
                                    }

                                    ui.colored_label(text_color, ":");

                                    if let Some(field_type) = &field_element.field_type {
                                        ui.colored_label(text_color, field_type.get_display_name());
                                    }
                                    ui.colored_label(text_color, ":");

                                    ui.colored_label(
                                        text_color,
                                        format!("0x{:X}", field_element.address),
                                    );
                                    ui.colored_label(
                                        text_color,
                                        format!("({} bytes)", field_element.size),
                                    );

                                    if let Some(_data) = &field_element.data {
                                        ui.colored_label(
                                            text_color,
                                            format!(
                                                "Value: {}",
                                                field_element.get_formatted_value(handle)
                                            ),
                                        );
                                    } else if let Some(error) = &field_element.error {
                                        ui.colored_label(
                                            egui::Color32::RED,
                                            format!("Error: {}", error),
                                        );
                                    } else {
                                        ui.colored_label(text_color, "Value: <no data>");
                                    }
                                });

                                field_index += 1;
                            } else {
                                break;
                            }
                        }

                        i = field_index;
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("📄");

                            if element.is_editing {
                                let response = ui.text_edit_singleline(&mut element.name);

                                if response.lost_focus()
                                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                                {
                                    element.is_editing = false;
                                }
                            } else {
                                let name_button = egui::Button::new(&element.name)
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE);

                                if ui.add(name_button).double_clicked() {
                                    element.is_editing = true;
                                }
                            }

                            ui.label(":");

                            ui.label(format!("0x{:X}", element.address));
                            ui.label(format!("({} bytes)", element.size));

                            if let Some(_data) = &element.data {
                                ui.label(format!("Value: {}", element.get_formatted_value(handle)));
                            } else if let Some(error) = &element.error {
                                ui.label(format!("Error: {}", error));
                            } else {
                                ui.label("Value: <no data>");
                            }
                        });

                        i += 1;
                    }
                }
            });
    }

    pub fn render_context_menu(
        &mut self,
        ctx: &egui::Context,
        memory_elements: &mut [MemoryElement],
        selection_manager: &SelectionManager,
    ) -> Option<ContextMenuAction> {
        if self.show_context_menu {
            let element_index = self.context_menu_element_index;
            let mut action = None;

            let has_multiple_selection = selection_manager.selected_fields.len() > 1;
            let selected_indices = if has_multiple_selection {
                selection_manager.selected_fields.clone()
            } else {
                vec![element_index.unwrap_or(0)]
            };

            egui::Window::new("Element Context Menu")
                .resizable(false)
                .collapsible(false)
                .title_bar(false)
                .fixed_pos(self.context_menu_position)
                .show(ctx, |ui| {
                    if let Some(index) = element_index {
                        let title = if has_multiple_selection {
                            format!(
                                "Element Options ({} selected)",
                                selection_manager.selected_fields.len()
                            )
                        } else {
                            "Element Options".to_string()
                        };
                        ui.heading(title);
                        ui.separator();

                        if ui.button("Add Element After").clicked() {
                            action = Some(ContextMenuAction::AddElementAfter(index));
                        }

                        if ui.button("Delete Element").clicked() {
                            if has_multiple_selection {
                                action = Some(ContextMenuAction::DeleteMultipleElements(
                                    selected_indices.clone(),
                                ));
                            } else {
                                action = Some(ContextMenuAction::DeleteElement(index));
                            }
                        }

                        ui.separator();

                        if ui.button("Change Type").clicked() {
                            if has_multiple_selection {
                                action = Some(ContextMenuAction::ChangeMultipleTypes(
                                    selected_indices.clone(),
                                ));
                            } else {
                                action = Some(ContextMenuAction::ChangeType(index));
                            }
                        }

                        if !has_multiple_selection {
                            ui.separator();

                            if ui.button("Copy Address").clicked() {
                                action = Some(ContextMenuAction::CopyAddress(index));
                            }

                            if ui.button("Copy Data").clicked() {
                                if let Some(element) = memory_elements.get(index) {
                                    if let Some(_data) = &element.data {
                                        action = Some(ContextMenuAction::CopyData(index));
                                    }
                                }
                            }

                            ui.separator();

                            if ui.button("Set as Pointer").clicked() {
                                action = Some(ContextMenuAction::SetAsPointer(index));
                            }

                            if ui.button("Follow Pointer").clicked() {
                                action = Some(ContextMenuAction::FollowPointer(index));
                            }

                            ui.separator();

                            if ui.button("New Class").clicked() {
                                if let Some(element) = memory_elements.get(index) {
                                    action = Some(ContextMenuAction::NewClass(
                                        element.address,
                                        format!("Class_{}", memory_elements.len()),
                                    ));
                                }
                            }
                        } else {
                            ui.separator();

                            ui.add_enabled(
                                false,
                                egui::Button::new("Copy Address (Single selection only)"),
                            );
                            ui.add_enabled(
                                false,
                                egui::Button::new("Copy Data (Single selection only)"),
                            );

                            ui.separator();

                            ui.add_enabled(
                                false,
                                egui::Button::new("Set as Pointer (Single selection only)"),
                            );
                            ui.add_enabled(
                                false,
                                egui::Button::new("Follow Pointer (Single selection only)"),
                            );

                            ui.separator();

                            ui.add_enabled(
                                false,
                                egui::Button::new("New Class (Single selection only)"),
                            );
                        }
                    }
                });

            if action.is_some() {
                self.show_context_menu = false;
            } else if ctx.input(|i| i.pointer.primary_clicked()) {
                let click_pos = ctx.pointer_interact_pos();
                if let Some(pos) = click_pos {
                    let distance = (pos - self.context_menu_position).length();
                    if distance > 200.0 {
                        // Arbitrary threshold
                        self.show_context_menu = false;
                    }
                }
            }

            action
        } else {
            None
        }
    }

    pub fn render_type_selection_window(
        &mut self,
        ctx: &egui::Context,
        _memory_elements: &mut [MemoryElement],
    ) -> Option<FieldType> {
        if self.show_type_selection {
            let mut selected_type = None;
            let mut window_open = true;

            egui::Window::new("Select Field Type")
                .open(&mut window_open)
                .resizable(true)
                .default_size(egui::vec2(400.0, 500.0))
                .show(ctx, |ui| {
                    let title = if self.type_selection_indices.len() > 1 {
                        format!(
                            "Select Type for {} Fields",
                            self.type_selection_indices.len()
                        )
                    } else {
                        "Select Field Type".to_string()
                    };
                    ui.heading(title);
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Hex Types
                        ui.heading("Hex Types");
                        if ui.button("Hex64").clicked() {
                            selected_type = Some(FieldType::Hex64);
                        }
                        if ui.button("Hex32").clicked() {
                            selected_type = Some(FieldType::Hex32);
                        }
                        if ui.button("Hex16").clicked() {
                            selected_type = Some(FieldType::Hex16);
                        }
                        if ui.button("Hex8").clicked() {
                            selected_type = Some(FieldType::Hex8);
                        }

                        ui.separator();

                        // Signed Integer Types
                        ui.heading("Signed Integers");
                        if ui.button("Int64").clicked() {
                            selected_type = Some(FieldType::Int64);
                        }
                        if ui.button("Int32").clicked() {
                            selected_type = Some(FieldType::Int32);
                        }
                        if ui.button("Int16").clicked() {
                            selected_type = Some(FieldType::Int16);
                        }
                        if ui.button("Int8").clicked() {
                            selected_type = Some(FieldType::Int8);
                        }

                        ui.separator();

                        // Unsigned Integer Types
                        ui.heading("Unsigned Integers");
                        if ui.button("UInt64").clicked() {
                            selected_type = Some(FieldType::UInt64);
                        }
                        if ui.button("UInt32").clicked() {
                            selected_type = Some(FieldType::UInt32);
                        }
                        if ui.button("UInt16").clicked() {
                            selected_type = Some(FieldType::UInt16);
                        }
                        if ui.button("UInt8").clicked() {
                            selected_type = Some(FieldType::UInt8);
                        }

                        ui.separator();

                        // Boolean
                        ui.heading("Boolean");
                        if ui.button("Bool").clicked() {
                            selected_type = Some(FieldType::Bool);
                        }

                        ui.separator();

                        // Floating Point Types
                        ui.heading("Floating Point");
                        if ui.button("Float").clicked() {
                            selected_type = Some(FieldType::Float);
                        }
                        if ui.button("Double").clicked() {
                            selected_type = Some(FieldType::Double);
                        }

                        ui.separator();

                        // Vector Types
                        ui.heading("Vectors");
                        if ui.button("Vector4").clicked() {
                            selected_type = Some(FieldType::Vector4);
                        }
                        if ui.button("Vector3").clicked() {
                            selected_type = Some(FieldType::Vector3);
                        }
                        if ui.button("Vector2").clicked() {
                            selected_type = Some(FieldType::Vector2);
                        }

                        ui.separator();

                        // Text Types
                        ui.heading("Text Types");
                        if ui.button("Text").clicked() {
                            selected_type = Some(FieldType::Text);
                        }
                        if ui.button("Text Pointer").clicked() {
                            selected_type = Some(FieldType::TextPointer);
                        }
                    });
                });

            if !window_open {
                self.show_type_selection = false;
            }

            if selected_type.is_some() {
                self.show_type_selection = false;
            }

            selected_type
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum ContextMenuAction {
    AddElementAfter(usize),
    DeleteElement(usize),
    DeleteMultipleElements(Vec<usize>),
    ChangeType(usize),
    ChangeMultipleTypes(Vec<usize>),
    CopyAddress(usize),
    CopyData(usize),
    SetAsPointer(usize),
    FollowPointer(usize),
    NewClass(u64, String),
}
