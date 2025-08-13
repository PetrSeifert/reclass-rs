use std::sync::atomic::{
    AtomicU64,
    Ordering,
};

use serde::{
    Deserialize,
    Serialize,
};

use crate::memory::types::{
    FieldType,
    PointerTarget,
};

static FIELD_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
fn next_field_id() -> u64 {
    FIELD_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}
use std::collections::HashMap;

static CLASS_DEF_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
fn next_class_def_id() -> u64 {
    CLASS_DEF_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}
static ENUM_DEF_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
fn next_enum_def_id() -> u64 {
    ENUM_DEF_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Represents a field in a class definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub id: u64,
    pub name: Option<String>, // None for hex fields
    pub field_type: FieldType,
    pub offset: u64, // Offset from the start of the class
    pub class_id: Option<u64>,
    pub pointer_target: Option<PointerTarget>, // For Pointer fields, stores target info
    pub enum_id: Option<u64>,
    pub enum_size: Option<u8>, // For Enum fields, underlying size in bytes (1,2,4,8)
    pub array_element: Option<PointerTarget>, // For Array fields, element description
    pub array_length: Option<u32>, // For Array fields, number of elements
}

impl FieldDefinition {
    #[allow(dead_code)]
    pub fn new(name: Option<String>, field_type: FieldType, offset: u64) -> Self {
        Self {
            id: next_field_id(),
            name,
            field_type,
            offset,
            class_id: None,
            pointer_target: None,
            enum_id: None,
            enum_size: None,
            array_element: None,
            array_length: None,
        }
    }

    #[allow(dead_code)]
    pub fn new_named(name: String, field_type: FieldType, offset: u64) -> Self {
        Self {
            id: next_field_id(),
            name: Some(name),
            field_type,
            offset,
            class_id: None,
            pointer_target: None,
            enum_id: None,
            enum_size: None,
            array_element: None,
            array_length: None,
        }
    }

    pub fn new_hex(field_type: FieldType, offset: u64) -> Self {
        Self {
            id: next_field_id(),
            name: None,
            field_type,
            offset,
            class_id: None,
            pointer_target: None,
            enum_id: None,
            enum_size: None,
            array_element: None,
            array_length: None,
        }
    }

    #[allow(dead_code)]
    pub fn get_size(&self) -> u64 {
        self.field_type.get_size()
    }
}

/// Represents a class definition that can be reused for multiple instances
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDefinition {
    pub id: u64,
    pub name: String,
    pub fields: Vec<FieldDefinition>,
    pub total_size: u64,
    #[serde(default)]
    pub entry_offset: Option<u64>,
}

impl ClassDefinition {
    pub fn new(name: String) -> Self {
        Self {
            id: next_class_def_id(),
            name,
            fields: Vec::new(),
            total_size: 0,
            entry_offset: None,
        }
    }

    pub fn add_field(&mut self, field: FieldDefinition) {
        self.fields.push(field);
        self.recalculate_size();
    }

    #[cfg(test)]
    pub fn add_named_field(&mut self, name: String, field_type: FieldType) {
        let offset = self.total_size;
        let field = FieldDefinition::new_named(name, field_type, offset);
        self.add_field(field);
    }

    pub fn add_hex_field(&mut self, field_type: FieldType) {
        let offset = self.total_size;
        let field = FieldDefinition::new_hex(field_type, offset);
        self.add_field(field);
    }

    #[cfg(test)]
    pub fn add_class_instance(&mut self, name: String, class_def: &ClassDefinition) {
        let offset = self.total_size;
        let mut field = FieldDefinition::new_named(name, FieldType::ClassInstance, offset);
        field.class_id = Some(class_def.id);
        self.add_field(field);
    }

    pub fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }

    fn recalculate_size(&mut self) {
        let mut running_offset: u64 = 0;
        for field in &mut self.fields {
            field.offset = running_offset;
            if !field.field_type.is_dynamic_size() {
                running_offset = running_offset.saturating_add(field.get_size());
            }
        }
        self.total_size = running_offset;
    }

    #[cfg(test)]
    pub fn get_field_by_name(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields
            .iter()
            .find(|field| field.name.as_ref().map(|n| n == name).unwrap_or(false))
    }

    #[cfg(test)]
    pub fn get_field_by_index(&self, index: usize) -> Option<&FieldDefinition> {
        self.fields.get(index)
    }

    pub fn insert_hex_field_at(&mut self, index: usize, field_type: FieldType) {
        let field = FieldDefinition::new_hex(field_type, 0);
        let idx = index.min(self.fields.len());
        self.fields.insert(idx, field);
        self.recalculate_size();
    }

    pub fn remove_field_at(&mut self, index: usize) {
        if index < self.fields.len() {
            self.fields.remove(index);
            self.recalculate_size();
        }
    }

    pub fn set_field_type_at(&mut self, index: usize, new_type: FieldType) {
        if let Some(f) = self.fields.get_mut(index) {
            f.field_type = new_type.clone();
            if new_type != FieldType::ClassInstance {
                f.class_id = None;
            }
            if new_type != FieldType::Pointer {
                f.pointer_target = None;
            }
            if new_type != FieldType::Enum {
                f.enum_id = None;
            }
            if new_type != FieldType::Array {
                f.array_element = None;
                f.array_length = None;
            } else {
                if f.array_element.is_none() {
                    f.array_element = Some(PointerTarget::FieldType(FieldType::Hex8));
                }
                if f.array_length.is_none() {
                    f.array_length = Some(1);
                }
            }
            if !new_type.is_hex_type() && f.name.is_none() {
                f.name = Some(format!("var_{index}"));
            } else if new_type.is_hex_type() {
                f.name = None;
            }
            self.recalculate_size();
        }
    }
}

/// Represents an enum definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDefinition {
    pub id: u64,
    pub name: String,
    pub is_flags: bool,
    pub default_size: u8, // 1,2,4,8 bytes
    pub variants: Vec<EnumVariant>,
}

impl EnumDefinition {
    pub fn new(name: String) -> Self {
        Self {
            id: next_enum_def_id(),
            name,
            is_flags: false,
            default_size: 4,
            variants: Vec::new(),
        }
    }

    pub fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub value: u32,
}

/// Registry for enum definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumDefinitionRegistry {
    definitions: HashMap<u64, EnumDefinition>,
}

impl EnumDefinitionRegistry {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    pub fn register(&mut self, enum_def: EnumDefinition) {
        self.definitions.insert(enum_def.id, enum_def);
    }

    pub fn get(&self, id: u64) -> Option<&EnumDefinition> {
        self.definitions.get(&id)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut EnumDefinition> {
        self.definitions.get_mut(&id)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.definitions.contains_key(&id)
    }
    pub fn contains_name(&self, name: &str) -> bool {
        self.definitions.values().any(|d| d.name == name)
    }

    pub fn get_enum_ids(&self) -> Vec<u64> {
        self.definitions.keys().cloned().collect()
    }
    pub fn remove(&mut self, id: u64) -> Option<EnumDefinition> {
        self.definitions.remove(&id)
    }

    pub fn get_by_id(&self, id: u64) -> Option<&EnumDefinition> {
        self.definitions.get(&id)
    }

    pub fn reseed_id_counters(&self) {
        let mut max_enum_id: u64 = 1;
        for def in self.definitions.values() {
            if def.id > max_enum_id {
                max_enum_id = def.id;
            }
        }
        // Bump to the next id to avoid collisions on new creations
        ENUM_DEF_ID_COUNTER.store(max_enum_id.saturating_add(1), Ordering::Relaxed);
    }
}

impl Default for EnumDefinitionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry for storing and reusing class definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDefinitionRegistry {
    definitions: HashMap<u64, ClassDefinition>,
}

impl ClassDefinitionRegistry {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    pub fn register(&mut self, class_def: ClassDefinition) {
        self.definitions.insert(class_def.id, class_def);
    }

    pub fn get(&self, id: u64) -> Option<&ClassDefinition> {
        self.definitions.get(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut ClassDefinition> {
        self.definitions.get_mut(&id)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.definitions.contains_key(&id)
    }

    pub fn contains_name(&self, name: &str) -> bool {
        self.definitions.values().any(|d| d.name == name)
    }

    pub fn get_class_ids(&self) -> Vec<u64> {
        self.definitions.values().map(|d| d.id).collect()
    }

    pub fn remove(&mut self, id: u64) -> Option<ClassDefinition> {
        self.definitions.remove(&id)
    }

    pub fn reseed_id_counters(&self) {
        let mut max_field_id: u64 = 1;
        let mut max_class_id: u64 = 1;
        for def in self.definitions.values() {
            if def.id > max_class_id {
                max_class_id = def.id;
            }
            for f in &def.fields {
                if f.id > max_field_id {
                    max_field_id = f.id;
                }
            }
        }
        // Bump to next id to avoid collisions on new creations
        FIELD_ID_COUNTER.store(max_field_id.saturating_add(1), Ordering::Relaxed);
        CLASS_DEF_ID_COUNTER.store(max_class_id.saturating_add(1), Ordering::Relaxed);
    }

    pub fn get_by_id(&self, id: u64) -> Option<&ClassDefinition> {
        self.definitions.values().find(|d| d.id == id)
    }
}

impl Default for ClassDefinitionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
