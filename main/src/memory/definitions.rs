use crate::memory::types::FieldType;
use std::collections::HashMap;

/// Represents a field in a class definition
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub name: Option<String>, // None for hex fields
    pub field_type: FieldType,
    pub offset: u64, // Offset from the start of the class
    pub class_name: Option<String>, // For ClassInstance fields, stores the target class name
}

impl FieldDefinition {
    pub fn new(name: Option<String>, field_type: FieldType, offset: u64) -> Self {
        Self {
            name,
            field_type,
            offset,
            class_name: None,
        }
    }

    pub fn new_named(name: String, field_type: FieldType, offset: u64) -> Self {
        Self {
            name: Some(name),
            field_type,
            offset,
            class_name: None,
        }
    }

    pub fn new_hex(field_type: FieldType, offset: u64) -> Self {
        Self {
            name: None,
            field_type,
            offset,
            class_name: None,
        }
    }

    pub fn get_size(&self) -> u64 {
        self.field_type.get_size()
    }

    pub fn is_hex_field(&self) -> bool {
        self.field_type.is_hex_type()
    }
}

/// Represents a class definition that can be reused for multiple instances
#[derive(Debug, Clone)]
pub struct ClassDefinition {
    pub name: String,
    pub fields: Vec<FieldDefinition>,
    pub total_size: u64,
}

impl ClassDefinition {
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            total_size: 0,
        }
    }

    /// Add a field to this class definition
    pub fn add_field(&mut self, field: FieldDefinition) {
        self.fields.push(field);
        self.recalculate_size();
    }

    /// Add a named field to this class definition
    pub fn add_named_field(&mut self, name: String, field_type: FieldType) {
        let offset = self.total_size;
        let field = FieldDefinition::new_named(name, field_type, offset);
        self.add_field(field);
    }

    /// Add a hex field to this class definition
    pub fn add_hex_field(&mut self, field_type: FieldType) {
        let offset = self.total_size;
        let field = FieldDefinition::new_hex(field_type, offset);
        self.add_field(field);
    }

    /// Add a class instance field to this class definition
    pub fn add_class_instance(&mut self, name: String, class_def: &ClassDefinition) {
        let offset = self.total_size;
        let mut field = FieldDefinition::new_named(name, FieldType::ClassInstance, offset);
        field.class_name = Some(class_def.name.clone());
        self.add_field(field);
        // Note: The actual size will be calculated dynamically when the instance is created
    }

    /// Recalculate the total size of this class
    fn recalculate_size(&mut self) {
        self.total_size = self.fields.iter()
            .filter(|field| !field.field_type.is_dynamic_size())
            .map(|field| field.get_size())
            .sum();
    }

    /// Get the total size of this class (excluding dynamic fields)
    pub fn get_size(&self) -> u64 {
        self.total_size
    }

    /// Get a field by name
    pub fn get_field_by_name(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields.iter().find(|field| field.name.as_ref().map(|n| n == name).unwrap_or(false))
    }

    /// Get a field by index
    pub fn get_field_by_index(&self, index: usize) -> Option<&FieldDefinition> {
        self.fields.get(index)
    }
}

/// Registry for storing and reusing class definitions
#[derive(Debug, Clone)]
pub struct ClassDefinitionRegistry {
    definitions: HashMap<String, ClassDefinition>,
}

impl ClassDefinitionRegistry {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    /// Register a class definition
    pub fn register(&mut self, class_def: ClassDefinition) {
        self.definitions.insert(class_def.name.clone(), class_def);
    }

    /// Get a class definition by name
    pub fn get(&self, name: &str) -> Option<&ClassDefinition> {
        self.definitions.get(name)
    }

    /// Check if a class definition exists
    pub fn contains(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    /// Get all registered class names
    pub fn get_class_names(&self) -> Vec<String> {
        self.definitions.keys().cloned().collect()
    }

    /// Remove a class definition
    pub fn remove(&mut self, name: &str) -> Option<ClassDefinition> {
        self.definitions.remove(name)
    }
}

impl Default for ClassDefinitionRegistry {
    fn default() -> Self {
        Self::new()
    }
} 