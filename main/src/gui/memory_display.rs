use egui::{Color32, RichText, Ui, Widget};
use crate::memory::{
    types::FieldType,
    definitions::{ClassDefinition, ClassDefinitionRegistry},
    nodes::{MemoryStructure, ClassInstance, MemoryField},
};

/// Represents a node in the memory structure tree
#[derive(Debug, Clone)]
pub enum MemoryNode {
    Root(ClassInstance),
    ClassInstance(ClassInstance),
    Field(MemoryField),
    ClassDefinition(ClassDefinition),
}

impl MemoryNode {
    pub fn get_display_name(&self) -> String {
        match self {
            MemoryNode::Root(instance) => format!("[ROOT] {}", instance.get_display_name()),
            MemoryNode::ClassInstance(instance) => format!("[INST] {}", instance.get_display_name()),
            MemoryNode::Field(field) => field.get_display_name(),
            MemoryNode::ClassDefinition(class_def) => format!("[DEF] {}", class_def.name),
        }
    }

    pub fn get_address(&self) -> Option<u64> {
        match self {
            MemoryNode::Root(instance) => Some(instance.address),
            MemoryNode::ClassInstance(instance) => Some(instance.address),
            MemoryNode::Field(field) => Some(field.address),
            MemoryNode::ClassDefinition(_) => None,
        }
    }

    pub fn get_size(&self) -> Option<u64> {
        match self {
            MemoryNode::Root(instance) => Some(instance.get_size()),
            MemoryNode::ClassInstance(instance) => Some(instance.get_size()),
            MemoryNode::Field(field) => Some(field.get_size()),
            MemoryNode::ClassDefinition(class_def) => Some(class_def.get_size()),
        }
    }

    pub fn get_field_type(&self) -> Option<&FieldType> {
        match self {
            MemoryNode::Field(field) => Some(&field.field_type),
            _ => None,
        }
    }

    pub fn is_expandable(&self) -> bool {
        matches!(self, MemoryNode::Root(_) | MemoryNode::ClassInstance(_) | MemoryNode::ClassDefinition(_))
    }

    pub fn get_children(&self, memory_structure: &MemoryStructure) -> Vec<MemoryNode> {
        match self {
            MemoryNode::Root(instance) => {
                let mut children = Vec::new();
                for field in &instance.fields {
                    if field.field_type == FieldType::ClassInstance {
                        // If this field has a nested instance, create a ClassInstance node
                        if let Some(nested_instance) = &field.nested_instance {
                            children.push(MemoryNode::ClassInstance(nested_instance.clone()));
                        } else {
                            // Otherwise show it as a field
                            children.push(MemoryNode::Field(field.clone()));
                        }
                    } else {
                        children.push(MemoryNode::Field(field.clone()));
                    }
                }
                children
            }
            MemoryNode::ClassInstance(instance) => {
                let mut children = Vec::new();
                for field in &instance.fields {
                    if field.field_type == FieldType::ClassInstance {
                        // If this field has a nested instance, create a ClassInstance node
                        if let Some(nested_instance) = &field.nested_instance {
                            children.push(MemoryNode::ClassInstance(nested_instance.clone()));
                        } else {
                            // Otherwise show it as a field
                            children.push(MemoryNode::Field(field.clone()));
                        }
                    } else {
                        children.push(MemoryNode::Field(field.clone()));
                    }
                }
                children
            }
            MemoryNode::ClassDefinition(class_def) => {
                let mut children = Vec::new();
                for field_def in &class_def.fields {
                    // Create a mock memory field for display purposes
                    let memory_field = MemoryField::new(
                        field_def.name.clone(),
                        field_def.field_type.clone(),
                        0, // Address will be calculated when instantiated
                    );
                    children.push(MemoryNode::Field(memory_field));
                }
                children
            }
            MemoryNode::Field(field) => {
                // If this field is a ClassInstance type and has a nested instance, show it
                if field.field_type == FieldType::ClassInstance {
                    if let Some(nested_instance) = &field.nested_instance {
                        vec![MemoryNode::ClassInstance(nested_instance.clone())]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// Memory structure tree display widget
pub struct MemoryStructureDisplay {
    expanded_nodes: std::collections::HashSet<String>,
    selected_node: Option<String>,
    editing_field: Option<String>, // Node ID of field being edited
    editing_name: String, // Current editing name
    editing_type: Option<FieldType>, // Current editing type
}

impl Default for MemoryStructureDisplay {
    fn default() -> Self {
        Self {
            expanded_nodes: std::collections::HashSet::new(),
            selected_node: None,
            editing_field: None,
            editing_name: String::new(),
            editing_type: None,
        }
    }
}

impl MemoryStructureDisplay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render the memory structure as a tree
    pub fn render(&mut self, ui: &mut Ui, memory_structure: &MemoryStructure) {
        ui.heading("Memory Structure");
        
        // Root class
        let root_node = MemoryNode::Root(memory_structure.root_class.clone());
        self.render_node(ui, &root_node, memory_structure, 0);
    }

    /// Render class definition buttons
    pub fn render_class_buttons(&mut self, ui: &mut Ui, memory_structure: &MemoryStructure, selected_class: &mut Option<String>) {
        ui.heading("Available Class Definitions");
        ui.horizontal_wrapped(|ui| {
            for class_name in memory_structure.get_available_classes() {
                let is_selected = selected_class.as_ref() == Some(&class_name);
                let button_text = if is_selected { 
                    format!("✓ {}", class_name) 
                } else { 
                    class_name.clone() 
                };
                
                if ui.button(button_text).clicked() {
                    *selected_class = Some(class_name.clone());
                }
            }
        });
    }

    fn render_node(&mut self, ui: &mut Ui, node: &MemoryNode, memory_structure: &MemoryStructure, depth: usize) {
        let node_id = self.get_node_id(node);
        let is_expanded = self.expanded_nodes.contains(&node_id);
        let is_selected = self.selected_node.as_ref() == Some(&node_id);

        // Create indentation
        let indent = "  ".repeat(depth);
        
        // Node display
        ui.horizontal(|ui| {
            // Indentation
            ui.label(indent);

            // Expand/collapse button for expandable nodes
            if node.is_expandable() {
                let expand_text = if is_expanded { "▼" } else { "▶" };
                if ui.button(expand_text).clicked() {
                    if is_expanded {
                        self.expanded_nodes.remove(&node_id);
                    } else {
                        self.expanded_nodes.insert(node_id.clone());
                    }
                }
            } else {
                ui.label("  "); // Spacing for non-expandable nodes
            }

            // Node name with color coding
            let text = RichText::new(node.get_display_name());
            let colored_text = match node {
                MemoryNode::Root(_) => text.color(Color32::from_rgb(255, 215, 0)), // Gold
                MemoryNode::ClassInstance(_) => text.color(Color32::from_rgb(100, 149, 237)), // Cornflower blue
                MemoryNode::ClassDefinition(_) => text.color(Color32::from_rgb(255, 165, 0)), // Orange
                MemoryNode::Field(field) => {
                    if field.is_hex_field() {
                        text.color(Color32::from_rgb(169, 169, 169)) // Gray for hex fields
                    } else {
                        text.color(Color32::from_rgb(255, 255, 255)) // White for named fields
                    }
                }
            };

            // Handle selection and context menu
            let response = ui.selectable_label(is_selected, colored_text);
            if response.clicked() {
                self.selected_node = Some(node_id.clone());
            }

            // Context menu for fields
            if let MemoryNode::Field(field) = node {
                response.context_menu(|ui| {
                    ui.label("Field Actions");
                    ui.separator();
                    
                    // Rename field
                    if ui.button("Rename Field").clicked() {
                        self.editing_field = Some(node_id.clone());
                        self.editing_name = field.name.clone().unwrap_or_default();
                        ui.close_menu();
                    }
                    
                    // Change field type
                    if ui.button("Change Type").clicked() {
                        self.editing_field = Some(node_id.clone());
                        self.editing_type = Some(field.field_type.clone());
                        ui.close_menu();
                    }
                    
                    // Delete field (for future implementation)
                    if ui.button("Delete Field").clicked() {
                        // TODO: Implement field deletion
                        ui.close_menu();
                    }
                });
            }

            // Address and size information
            if let Some(address) = node.get_address() {
                ui.label(format!("@ 0x{:X}", address));
            }
            
            if let Some(size) = node.get_size() {
                ui.label(format!("({} bytes)", size));
            }

            // Field type information
            if let Some(field_type) = node.get_field_type() {
                ui.label(format!("[{}]", field_type));
            }
        });

        // Render children if expanded
        if is_expanded {
            for child in node.get_children(memory_structure) {
                self.render_node(ui, &child, memory_structure, depth + 1);
            }
        }
    }

    fn get_node_id(&self, node: &MemoryNode) -> String {
        match node {
            MemoryNode::Root(instance) => format!("root_{}", instance.name),
            MemoryNode::ClassInstance(instance) => format!("instance_{}_{}", instance.name, instance.address),
            MemoryNode::Field(field) => format!("field_{}_{}", field.name.as_deref().unwrap_or("unnamed"), field.address),
            MemoryNode::ClassDefinition(class_def) => format!("def_{}", class_def.name),
        }
    }

    /// Get the currently selected node
    pub fn get_selected_node(&self) -> Option<&String> {
        self.selected_node.as_ref()
    }

    /// Clear the selection
    pub fn clear_selection(&mut self) {
        self.selected_node = None;
    }

    /// Render field editing dialog
    pub fn render_field_editing(&mut self, ui: &mut Ui, memory_structure: &mut MemoryStructure) {
        if let Some(editing_node_id) = &self.editing_field {
            // Simple field editing dialog
            egui::Window::new("Edit Field")
                .collapsible(false)
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    ui.label("Field Properties");
                    ui.separator();
                    
                    // Name editing
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.editing_name);
                    
                    // Type selection
                    ui.label("Type:");
                    let mut selected_type = self.editing_type.clone().unwrap_or(FieldType::Int32);
                    
                    egui::ComboBox::from_id_source("field_type")
                        .selected_text(format!("{}", selected_type))
                        .show_ui(ui, |ui| {
                            for field_type in self.get_available_field_types() {
                                ui.selectable_value(&mut selected_type, field_type.clone(), format!("{}", field_type));
                            }
                        });
                    
                    ui.separator();
                    
                    // Buttons
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            // For now, just close the dialog
                            self.editing_field = None;
                            self.editing_name.clear();
                            self.editing_type = None;
                        }
                        
                        if ui.button("Cancel").clicked() {
                            self.editing_field = None;
                            self.editing_name.clear();
                            self.editing_type = None;
                        }
                    });
                });
        }
    }

    /// Get available field types for selection
    fn get_available_field_types(&self) -> Vec<FieldType> {
        vec![
            FieldType::Hex64, FieldType::Hex32, FieldType::Hex16, FieldType::Hex8,
            FieldType::Int64, FieldType::Int32, FieldType::Int16, FieldType::Int8,
            FieldType::UInt64, FieldType::UInt32, FieldType::UInt16, FieldType::UInt8,
            FieldType::Bool, FieldType::Float, FieldType::Double,
            FieldType::Vector4, FieldType::Vector3, FieldType::Vector2,
            FieldType::Text, FieldType::TextPointer,
            FieldType::ClassInstance,
        ]
    }
}

/// Memory structure details panel
pub struct MemoryDetailsPanel {
    selected_node: Option<MemoryNode>,
}

impl Default for MemoryDetailsPanel {
    fn default() -> Self {
        Self {
            selected_node: None,
        }
    }
}

impl MemoryDetailsPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the selected node
    pub fn set_selected_node(&mut self, node: Option<MemoryNode>) {
        self.selected_node = node;
    }

    /// Render the details panel
    pub fn render(&self, ui: &mut Ui) {
        ui.heading("Node Details");
        
        if let Some(node) = &self.selected_node {
            ui.separator();
            
            // Basic information
            ui.label(format!("Name: {}", node.get_display_name()));
            
            if let Some(address) = node.get_address() {
                ui.label(format!("Address: 0x{:X}", address));
            }
            
            if let Some(size) = node.get_size() {
                ui.label(format!("Size: {} bytes", size));
            }
            
            if let Some(field_type) = node.get_field_type() {
                ui.label(format!("Type: {}", field_type));
                ui.label(format!("Type Size: {} bytes", field_type.get_size()));
                
                if field_type.is_hex_type() {
                    ui.label("Hex Field: Yes");
                } else {
                    ui.label("Hex Field: No");
                }
                
                if field_type.is_dynamic_size() {
                    ui.label("Dynamic Size: Yes");
                } else {
                    ui.label("Dynamic Size: No");
                }
            }

            // Additional details based on node type
            match node {
                MemoryNode::Root(instance) | MemoryNode::ClassInstance(instance) => {
                    ui.separator();
                    ui.label(format!("Class: {}", instance.class_definition.name));
                    ui.label(format!("Field Count: {}", instance.fields.len()));
                }
                MemoryNode::ClassDefinition(class_def) => {
                    ui.separator();
                    ui.label(format!("Field Count: {}", class_def.fields.len()));
                    ui.label(format!("Total Size: {} bytes", class_def.get_size()));
                }
                MemoryNode::Field(field) => {
                    ui.separator();
                    if let Some(name) = &field.name {
                        ui.label(format!("Field Name: {}", name));
                    } else {
                        ui.label("Field Name: (unnamed hex field)");
                    }
                    
                    if let Some(data) = &field.data {
                        ui.label(format!("Data: {:02X?}", data));
                    } else {
                        ui.label("Data: Not loaded");
                    }
                    
                    if let Some(error) = &field.error {
                        ui.label(format!("Error: {}", error));
                    }
                }
            }
        } else {
            ui.label("No node selected");
        }
    }
} 