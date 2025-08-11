use std::{
    collections::HashSet,
    sync::atomic::{
        AtomicU64,
        Ordering,
    },
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
    pub offset: u64,                           // Offset from the start of the class
    pub class_name: Option<String>, // For ClassInstance fields, stores the target class name
    pub pointer_target: Option<PointerTarget>, // For Pointer fields, stores target info
    pub enum_name: Option<String>,  // For Enum fields, stores the enum name
    pub enum_size: Option<u8>,      // For Enum fields, underlying size in bytes (1,2,4,8)
}

impl FieldDefinition {
    #[allow(dead_code)]
    pub fn new(name: Option<String>, field_type: FieldType, offset: u64) -> Self {
        Self {
            id: next_field_id(),
            name,
            field_type,
            offset,
            class_name: None,
            pointer_target: None,
            enum_name: None,
            enum_size: None,
        }
    }

    #[allow(dead_code)]
    pub fn new_named(name: String, field_type: FieldType, offset: u64) -> Self {
        Self {
            id: next_field_id(),
            name: Some(name),
            field_type,
            offset,
            class_name: None,
            pointer_target: None,
            enum_name: None,
            enum_size: None,
        }
    }

    pub fn new_hex(field_type: FieldType, offset: u64) -> Self {
        Self {
            id: next_field_id(),
            name: None,
            field_type,
            offset,
            class_name: None,
            pointer_target: None,
            enum_name: None,
            enum_size: None,
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
}

impl ClassDefinition {
    pub fn new(name: String) -> Self {
        Self {
            id: next_class_def_id(),
            name,
            fields: Vec::new(),
            total_size: 0,
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
        field.class_name = Some(class_def.name.clone());
        self.add_field(field);
    }

    pub fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }

    #[allow(dead_code)]
    pub fn get_id(&self) -> u64 {
        self.id
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

    #[allow(dead_code)]
    pub fn get_size(&self) -> u64 {
        self.total_size
    }

    #[allow(dead_code)]
    pub fn get_field_by_name(&self, name: &str) -> Option<&FieldDefinition> {
        self.fields
            .iter()
            .find(|field| field.name.as_ref().map(|n| n == name).unwrap_or(false))
    }

    #[allow(dead_code)]
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
                f.class_name = None;
            }
            if new_type != FieldType::Pointer {
                f.pointer_target = None;
            }
            if new_type != FieldType::Enum {
                f.enum_name = None;
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
    definitions: HashMap<String, EnumDefinition>,
}

impl EnumDefinitionRegistry {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    pub fn register(&mut self, enum_def: EnumDefinition) {
        self.definitions.insert(enum_def.name.clone(), enum_def);
    }

    pub fn get(&self, name: &str) -> Option<&EnumDefinition> {
        self.definitions.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut EnumDefinition> {
        self.definitions.get_mut(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    pub fn get_enum_names(&self) -> Vec<String> {
        self.definitions.keys().cloned().collect()
    }

    pub fn remove(&mut self, name: &str) -> Option<EnumDefinition> {
        self.definitions.remove(name)
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
    definitions: HashMap<String, ClassDefinition>,
}

impl ClassDefinitionRegistry {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    pub fn register(&mut self, class_def: ClassDefinition) {
        self.definitions.insert(class_def.name.clone(), class_def);
    }

    pub fn get(&self, name: &str) -> Option<&ClassDefinition> {
        self.definitions.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut ClassDefinition> {
        self.definitions.get_mut(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    pub fn get_class_names(&self) -> Vec<String> {
        self.definitions.keys().cloned().collect()
    }

    pub fn remove(&mut self, name: &str) -> Option<ClassDefinition> {
        self.definitions.remove(name)
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

    pub fn normalize_ids(&mut self) {
        for def in self.definitions.values_mut() {
            let mut seen_field_ids: HashSet<u64> = HashSet::new();
            for f in &mut def.fields {
                if !seen_field_ids.insert(f.id) {
                    // Duplicate id; assign a fresh one not in seen
                    let mut new_id = next_field_id();
                    while seen_field_ids.contains(&new_id) {
                        new_id = next_field_id();
                    }
                    f.id = new_id;
                    seen_field_ids.insert(new_id);
                }
            }
        }
        // After normalization, reseed counters for future creations
        self.reseed_id_counters();
    }
}

impl Default for ClassDefinitionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
