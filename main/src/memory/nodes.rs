use crate::memory::types::FieldType;
use crate::memory::definitions::{ClassDefinition, ClassDefinitionRegistry};

/// Represents a field in memory with its data
#[derive(Debug, Clone)]
pub struct MemoryField {
    pub name: Option<String>, // None for hex fields
    pub field_type: FieldType,
    pub address: u64,
    pub data: Option<Vec<u8>>,
    pub error: Option<String>,
    pub is_editing: bool,
    pub nested_instance: Option<ClassInstance>, // For ClassInstance fields
}

impl MemoryField {
    pub fn new(name: Option<String>, field_type: FieldType, address: u64) -> Self {
        Self {
            name,
            field_type,
            address,
            data: None,
            error: None,
            is_editing: false,
            nested_instance: None,
        }
    }

    pub fn new_named(name: String, field_type: FieldType, address: u64) -> Self {
        Self {
            name: Some(name),
            field_type,
            address,
            data: None,
            error: None,
            is_editing: false,
            nested_instance: None,
        }
    }

    pub fn new_hex(field_type: FieldType, address: u64) -> Self {
        Self {
            name: None,
            field_type,
            address,
            data: None,
            error: None,
            is_editing: false,
            nested_instance: None,
        }
    }

    pub fn get_size(&self) -> u64 {
        self.field_type.get_size()
    }

    pub fn is_hex_field(&self) -> bool {
        self.field_type.is_hex_type()
    }

    pub fn get_display_name(&self) -> String {
        match &self.name {
            Some(name) => format!("{}: {}", name, self.field_type),
            None => self.field_type.to_string(),
        }
    }
}

/// Represents a class instance in memory
#[derive(Debug, Clone)]
pub struct ClassInstance {
    pub name: String,
    pub address: u64,
    pub class_definition: ClassDefinition,
    pub fields: Vec<MemoryField>,
    pub total_size: u64,
}

impl ClassInstance {
    pub fn new(name: String, address: u64, class_definition: ClassDefinition) -> Self {
        let mut instance = Self {
            name,
            address,
            class_definition,
            fields: Vec::new(),
            total_size: 0,
        };
        instance.create_fields_from_definition();
        instance
    }

    /// Create memory fields based on the class definition
    fn create_fields_from_definition(&mut self) {
        let mut current_offset = 0;
        
        for field_def in &self.class_definition.fields {
            let field_address = self.address + field_def.offset;
            
            let mut memory_field = match &field_def.name {
                Some(name) => MemoryField::new_named(name.clone(), field_def.field_type.clone(), field_address),
                None => MemoryField::new_hex(field_def.field_type.clone(), field_address),
            };
            
            // Handle ClassInstance fields by creating nested instances
            if field_def.field_type == FieldType::ClassInstance {
                // For now, we'll create a placeholder nested instance
                // In a real implementation, this would need the actual class definition
                if let Some(name) = &field_def.name {
                    // Create a placeholder nested instance
                    let nested_class_def = ClassDefinition::new(format!("{}_nested", name));
                    let nested_instance = ClassInstance::new(
                        name.clone(),
                        field_address,
                        nested_class_def,
                    );
                    memory_field.nested_instance = Some(nested_instance);
                }
            }
            
            self.fields.push(memory_field);
            
            if !field_def.field_type.is_dynamic_size() {
                current_offset += field_def.get_size();
            }
        }
        
        self.total_size = current_offset;
    }

    /// Get a field by name
    pub fn get_field_by_name(&self, name: &str) -> Option<&MemoryField> {
        self.fields.iter().find(|field| field.name.as_ref().map(|n| n == name).unwrap_or(false))
    }

    /// Get a field by index
    pub fn get_field_by_index(&self, index: usize) -> Option<&MemoryField> {
        self.fields.get(index)
    }

    /// Get the total size of this class instance
    pub fn get_size(&self) -> u64 {
        self.total_size
    }

    /// Get the display name for this class instance
    pub fn get_display_name(&self) -> String {
        format!("{}: {}", self.name, self.class_definition.name)
    }
}

/// Represents the root memory structure
#[derive(Debug, Clone)]
pub struct MemoryStructure {
    pub root_class: ClassInstance,
    pub class_registry: ClassDefinitionRegistry,
}

impl MemoryStructure {
    pub fn new(root_name: String, root_address: u64, root_class_def: ClassDefinition) -> Self {
        let root_class = ClassInstance::new(root_name, root_address, root_class_def.clone());
        
        let mut class_registry = ClassDefinitionRegistry::new();
        class_registry.register(root_class_def);
        
        Self {
            root_class,
            class_registry,
        }
    }

    /// Add a class definition to the registry
    pub fn register_class(&mut self, class_def: ClassDefinition) {
        self.class_registry.register(class_def);
    }

    /// Get a class definition from the registry
    pub fn get_class_definition(&self, name: &str) -> Option<&ClassDefinition> {
        self.class_registry.get(name)
    }

    /// Create a new class instance and add it to the structure
    pub fn create_class_instance(&mut self, name: String, address: u64, class_name: &str) -> Option<ClassInstance> {
        if let Some(class_def) = self.class_registry.get(class_name) {
            Some(ClassInstance::new(name, address, class_def.clone()))
        } else {
            None
        }
    }

    /// Create nested instances for all ClassInstance fields
    pub fn create_nested_instances(&mut self) {
        // Create a new root class with nested instances
        let mut new_root = self.root_class.clone();
        self.create_nested_instances_simple(&mut new_root);
        self.root_class = new_root;
    }

    /// Simple method to create nested instances (placeholder implementation)
    fn create_nested_instances_simple(&self, instance: &mut ClassInstance) {
        for field in &mut instance.fields {
            if field.field_type == FieldType::ClassInstance {
                if let Some(name) = &field.name {
                    // Find the field definition to get the class name
                    if let Some(field_def) = instance.class_definition.fields.iter()
                        .find(|fd| fd.name.as_ref().map(|n| n == name).unwrap_or(false)) {
                        
                        // Use the class name from the field definition
                        if let Some(class_name) = &field_def.class_name {
                            if let Some(class_def) = self.class_registry.get(class_name) {
                                let nested_instance = ClassInstance::new(
                                    name.clone(),
                                    field.address,
                                    class_def.clone(),
                                );
                                field.nested_instance = Some(nested_instance);
                            } else {
                                // Create a placeholder if the class definition doesn't exist
                                let placeholder_def = ClassDefinition::new(format!("{}_placeholder", name));
                                let nested_instance = ClassInstance::new(
                                    name.clone(),
                                    field.address,
                                    placeholder_def,
                                );
                                field.nested_instance = Some(nested_instance);
                            }
                        } else {
                            // Create a placeholder if no class name is specified
                            let placeholder_def = ClassDefinition::new(format!("{}_placeholder", name));
                            let nested_instance = ClassInstance::new(
                                name.clone(),
                                field.address,
                                placeholder_def,
                            );
                            field.nested_instance = Some(nested_instance);
                        }
                    } else {
                        // Create a placeholder if field definition not found
                        let placeholder_def = ClassDefinition::new(format!("{}_placeholder", name));
                        let nested_instance = ClassInstance::new(
                            name.clone(),
                            field.address,
                            placeholder_def,
                        );
                        field.nested_instance = Some(nested_instance);
                    }
                }
            }
        }
    }

    /// Get the total size of the memory structure
    pub fn get_total_size(&self) -> u64 {
        self.root_class.get_size()
    }

    /// Get all class names in the registry
    pub fn get_available_classes(&self) -> Vec<String> {
        self.class_registry.get_class_names()
    }
}

/// Builder for creating memory structures
#[derive(Clone)]
pub struct MemoryStructureBuilder {
    class_registry: ClassDefinitionRegistry,
}

impl MemoryStructureBuilder {
    pub fn new() -> Self {
        Self {
            class_registry: ClassDefinitionRegistry::new(),
        }
    }

    /// Register a class definition
    pub fn register_class(&mut self, class_def: ClassDefinition) -> &mut Self {
        self.class_registry.register(class_def);
        self
    }

    /// Build the memory structure with a root class
    pub fn build(self, root_name: String, root_address: u64, root_class_name: &str) -> Option<MemoryStructure> {
        if let Some(root_class_def) = self.class_registry.get(root_class_name) {
            let root_class = ClassInstance::new(root_name, root_address, root_class_def.clone());
            
            let mut class_registry = self.class_registry.clone();
            class_registry.register(root_class_def.clone());
            
            Some(MemoryStructure {
                root_class,
                class_registry,
            })
        } else {
            None
        }
    }
}

impl Default for MemoryStructureBuilder {
    fn default() -> Self {
        Self::new()
    }
} 