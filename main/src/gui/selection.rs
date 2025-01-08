use crate::gui::memory_elements::{
    ClassElement,
    MemoryElement,
};

pub struct SelectionManager {
    pub selected_fields: Vec<usize>,
    pub last_selected_container: Option<usize>,
    pub pending_field_click: Option<(usize, bool, bool)>, // (field_index, ctrl_pressed, shift_pressed)
}

impl SelectionManager {
    pub fn new() -> Self {
        Self {
            selected_fields: Vec::new(),
            last_selected_container: None,
            pending_field_click: None,
        }
    }

    pub fn find_container_for_field(
        &self,
        memory_elements: &[MemoryElement],
        field_index: usize,
    ) -> Option<usize> {
        for i in (0..field_index).rev() {
            if let Some(ClassElement::Root) | Some(ClassElement::Pointer) =
                &memory_elements[i].class_type
            {
                return Some(i);
            }
        }
        None
    }

    pub fn handle_field_selection(
        &mut self,
        memory_elements: &[MemoryElement],
        field_index: usize,
        ctrl_pressed: bool,
        shift_pressed: bool,
    ) {
        let container_index = self.find_container_for_field(memory_elements, field_index);

        if container_index.is_none() {
            return;
        }

        let container_index = container_index.unwrap();

        if !ctrl_pressed && !shift_pressed {
            // Single click - clear selection and select only this field
            self.selected_fields.clear();
            self.selected_fields.push(field_index);
            self.last_selected_container = Some(container_index);
        } else if ctrl_pressed && !shift_pressed {
            // Ctrl+Click - toggle selection of this field
            if let Some(pos) = self.selected_fields.iter().position(|&x| x == field_index) {
                self.selected_fields.remove(pos);
            } else {
                // Only add if it's in the same container as existing selections
                if self.last_selected_container == Some(container_index) {
                    self.selected_fields.push(field_index);
                } else {
                    // Different container - clear and select only this field
                    self.selected_fields.clear();
                    self.selected_fields.push(field_index);
                    self.last_selected_container = Some(container_index);
                }
            }
        } else if shift_pressed && !ctrl_pressed {
            // Shift+Click - select range from last selected to current
            if let Some(last_container) = self.last_selected_container {
                if last_container == container_index {
                    // Same container - select range
                    if let Some(last_selected) = self.selected_fields.last() {
                        let start = (*last_selected).min(field_index);
                        let end = (*last_selected).max(field_index);

                        // Find all fields in this container between start and end
                        for (i, item) in
                            memory_elements.iter().enumerate().take(end + 1).skip(start)
                        {
                            if let Some(ClassElement::Field) = &item.class_type {
                                if !self.selected_fields.contains(&i) {
                                    self.selected_fields.push(i);
                                }
                            }
                        }
                    }
                } else {
                    // Different container - clear and select only this field
                    self.selected_fields.clear();
                    self.selected_fields.push(field_index);
                    self.last_selected_container = Some(container_index);
                }
            } else {
                // No previous selection - just select this field
                self.selected_fields.clear();
                self.selected_fields.push(field_index);
                self.last_selected_container = Some(container_index);
            }
        } else if ctrl_pressed && shift_pressed {
            // Ctrl+Shift+Click - add range to existing selection
            if let Some(last_container) = self.last_selected_container {
                if last_container == container_index {
                    // Same container - add range to selection
                    if let Some(last_selected) = self.selected_fields.last() {
                        let start = (*last_selected).min(field_index);
                        let end = (*last_selected).max(field_index);

                        // Find all fields in this container between start and end
                        for (i, item) in
                            memory_elements.iter().enumerate().take(end + 1).skip(start)
                        {
                            if let Some(ClassElement::Field) = &item.class_type {
                                if !self.selected_fields.contains(&i) {
                                    self.selected_fields.push(i);
                                }
                            }
                        }
                    }
                } else {
                    // Different container - ignore the click
                }
            } else {
                // No previous selection - just select this field
                self.selected_fields.clear();
                self.selected_fields.push(field_index);
                self.last_selected_container = Some(container_index);
            }
        }
    }

    pub fn is_field_selected(&self, field_index: usize) -> bool {
        self.selected_fields.contains(&field_index)
    }

    pub fn clear_selections(&mut self) {
        self.selected_fields.clear();
        self.last_selected_container = None;
    }
}
