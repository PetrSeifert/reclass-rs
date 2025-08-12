use std::collections::HashSet;

use serde::{
    Deserialize,
    Serialize,
};

use crate::memory::{
    definitions::{
        ClassDefinition,
        ClassDefinitionRegistry,
        EnumDefinitionRegistry,
    },
    types::{
        FieldType,
        PointerTarget,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryField {
    pub def_id: u64,
    pub name: Option<String>, // None for hex fields
    pub field_type: FieldType,
    pub address: u64,
    pub data: Option<Vec<u8>>,
    pub error: Option<String>,
    pub is_editing: bool,
    pub nested_instance: Option<ClassInstance>,
    pub pointer_target: Option<PointerTarget>,
    pub enum_size: Option<u8>,
}

impl MemoryField {
    pub fn new_named(name: String, field_type: FieldType, address: u64) -> Self {
        Self {
            def_id: 0,
            name: Some(name),
            field_type,
            address,
            data: None,
            error: None,
            is_editing: false,
            nested_instance: None,
            pointer_target: None,
            enum_size: None,
        }
    }

    pub fn new_hex(field_type: FieldType, address: u64) -> Self {
        Self {
            def_id: 0,
            name: None,
            field_type,
            address,
            data: None,
            error: None,
            is_editing: false,
            nested_instance: None,
            pointer_target: None,
            enum_size: None,
        }
    }

    pub fn get_size(&self) -> u64 {
        self.field_type.get_size()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInstance {
    pub name: String,
    pub address: u64,
    pub class_id: u64,
    pub fields: Vec<MemoryField>,
    pub total_size: u64,
}

impl ClassInstance {
    pub fn new(name: String, address: u64, class_definition: ClassDefinition) -> Self {
        let mut instance = Self {
            name,
            address,
            class_id: class_definition.id,
            fields: Vec::new(),
            total_size: 0,
        };
        instance.create_fields_from_definition(&class_definition);
        instance
    }

    fn create_fields_from_definition(&mut self, class_definition: &ClassDefinition) {
        let mut current_offset = 0;

        for field_def in &class_definition.fields {
            let field_address = self.address + field_def.offset;

            let mut memory_field = match &field_def.name {
                Some(name) => MemoryField::new_named(
                    name.clone(),
                    field_def.field_type.clone(),
                    field_address,
                ),
                None => MemoryField::new_hex(field_def.field_type.clone(), field_address),
            };
            memory_field.def_id = field_def.id;
            // Copy pointer target metadata for convenience when rendering
            if field_def.field_type == FieldType::Pointer {
                memory_field.pointer_target = field_def.pointer_target.clone();
            }

            self.fields.push(memory_field);

            if !field_def.field_type.is_dynamic_size() {
                current_offset += field_def.get_size();
            }
        }

        self.total_size = current_offset;
    }

    #[cfg(test)]
    pub fn get_field_by_name(&self, name: &str) -> Option<&MemoryField> {
        self.fields
            .iter()
            .find(|field| field.name.as_ref().map(|n| n == name).unwrap_or(false))
    }

    #[cfg(test)]
    pub fn get_field_by_index(&self, index: usize) -> Option<&MemoryField> {
        self.fields.get(index)
    }

    pub fn get_size(&self) -> u64 {
        self.total_size
    }

    #[cfg(test)]
    pub fn get_display_name_with_registry(&self, reg: &ClassDefinitionRegistry) -> String {
        let cname = reg
            .get(self.class_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("#{}", self.class_id));
        format!("{}: {}", self.name, cname)
    }
}

/// Represents the root memory structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStructure {
    pub root_class: ClassInstance,
    pub class_registry: ClassDefinitionRegistry,
    #[serde(default)]
    pub enum_registry: EnumDefinitionRegistry,
}

impl MemoryStructure {
    pub fn new(root_name: String, root_address: u64, root_class_def: ClassDefinition) -> Self {
        let root_class = ClassInstance::new(root_name, root_address, root_class_def.clone());

        let mut class_registry = ClassDefinitionRegistry::new();
        class_registry.register(root_class_def);

        Self {
            root_class,
            class_registry,
            enum_registry: EnumDefinitionRegistry::new(),
        }
    }

    pub fn rename_class(&mut self, id: u64, new_name: &str) -> bool {
        if !self.class_registry.contains(id) {
            return false;
        }

        if self.class_registry.contains_name(new_name) {
            return false;
        }

        let old_name = self.class_registry.get(id).unwrap().name.clone();
        if old_name == new_name || old_name.is_empty() || new_name.is_empty() {
            return false;
        }

        let mut moved_def_opt = self.class_registry.remove(id);
        if let Some(mut def) = moved_def_opt.take() {
            def.rename(new_name.to_string());
            self.class_registry.register(def);
        }

        // After registry is updated with the renamed definition, recalculate layout
        Self::recalc_instance_layout(
            &self.enum_registry,
            &self.class_registry,
            &mut self.root_class,
        );
        true
    }

    /// Rename enum definition and update all field references
    pub fn rename_enum(&mut self, id: u64, new_name: &str) -> bool {
        if !self.enum_registry.contains(id) {
            return false;
        }

        if self.enum_registry.contains_name(new_name) {
            return false;
        }

        let old_name = self.enum_registry.get(id).unwrap().name.clone();
        if old_name == new_name || old_name.is_empty() || new_name.is_empty() {
            return false;
        }

        // Actually rename the enum definition by remove and re-register
        if let Some(mut ed) = self.enum_registry.remove(id) {
            ed.rename(new_name.to_string());
            self.enum_registry.register(ed);
        }

        // Rebuild layout to reflect any size/name changes
        Self::recalc_instance_layout(
            &self.enum_registry,
            &self.class_registry,
            &mut self.root_class,
        );
        true
    }

    /// Check if an enum is referenced in any class definition field (by id lookup)
    pub fn is_enum_referenced(&self, enum_id: u64) -> bool {
        for cid in self.class_registry.get_class_ids() {
            if let Some(def) = self.class_registry.get(cid) {
                for f in &def.fields {
                    if f.field_type == FieldType::Enum && f.enum_id == Some(enum_id) {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[cfg(test)]
    pub fn register_class(&mut self, class_def: ClassDefinition) {
        self.class_registry.register(class_def);
    }

    #[cfg(test)]
    pub fn get_class_definition(&self, id: u64) -> Option<&ClassDefinition> {
        self.class_registry.get(id)
    }

    #[cfg(test)]
    pub fn create_class_instance(
        &mut self,
        name: String,
        address: u64,
        class_id: u64,
    ) -> Option<ClassInstance> {
        self.class_registry
            .get(class_id)
            .map(|class_def| ClassInstance::new(name, address, class_def.clone()))
    }

    pub fn create_nested_instances(&mut self) {
        let registry = self.class_registry.clone();
        Self::build_nested_for_instance(&registry, &mut self.root_class);
        Self::recalc_instance_layout(
            &self.enum_registry,
            &self.class_registry,
            &mut self.root_class,
        );
    }

    pub fn bind_nested_for_instance(&self, instance: &mut ClassInstance) {
        let registry = self.class_registry.clone();
        Self::build_nested_for_instance(&registry, instance);
        Self::recalc_instance_layout(&self.enum_registry, &self.class_registry, instance);
    }

    pub fn rebuild_root_from_registry(&mut self) {
        let root_type = self.root_class.class_id;
        if let Some(def) = self.class_registry.get(root_type).cloned() {
            let name = self.root_class.name.clone();
            let address = self.root_class.address;
            self.root_class = ClassInstance::new(name, address, def);
            let registry = self.class_registry.clone();
            Self::build_nested_for_instance(&registry, &mut self.root_class);
            Self::recalc_instance_layout(
                &self.enum_registry,
                &self.class_registry,
                &mut self.root_class,
            );
        }
    }

    fn build_nested_for_instance(registry: &ClassDefinitionRegistry, instance: &mut ClassInstance) {
        for field in &mut instance.fields {
            if field.field_type == FieldType::ClassInstance {
                let field_def_opt = registry
                    .get_by_id(instance.class_id)
                    .and_then(|def| def.fields.iter().find(|fd| fd.id == field.def_id));

                if let Some(field_def) = field_def_opt {
                    let class_def_opt = if let Some(cid) = field_def.class_id {
                        registry.get_by_id(cid)
                    } else {
                        None
                    };
                    if let Some(class_def) = class_def_opt {
                        // Always create a fresh instance and clear any stale nested linkage
                        field.nested_instance = None;
                        let mut nested_instance = ClassInstance::new(
                            field.name.clone().unwrap_or_default(),
                            field.address,
                            class_def.clone(),
                        );
                        Self::build_nested_for_instance(registry, &mut nested_instance);
                        // Use default enum registry for nested; caller will re-run with real registry on rebuild
                        Self::recalc_instance_layout(
                            &EnumDefinitionRegistry::new(),
                            registry,
                            &mut nested_instance,
                        );
                        field.nested_instance = Some(nested_instance);
                        continue;
                    }
                }
                // If no mapping found, keep None
            } else {
                // Ensure primitive fields do not retain stale nested instances
                field.nested_instance = None;
            }
        }
        Self::recalc_instance_layout(&EnumDefinitionRegistry::new(), registry, instance);
    }

    fn recalc_instance_layout(
        enum_registry: &EnumDefinitionRegistry,
        class_registry: &ClassDefinitionRegistry,
        instance: &mut ClassInstance,
    ) {
        let mut current_offset: u64 = 0;
        for field in &mut instance.fields {
            field.address = instance.address + current_offset;
            let advance = match field.field_type {
                FieldType::ClassInstance => {
                    if let Some(ref mut nested) = field.nested_instance {
                        nested.address = field.address;
                        Self::recalc_instance_layout(enum_registry, class_registry, nested);
                        nested.total_size.min(1_048_576)
                    } else {
                        0
                    }
                }
                FieldType::Array => {
                    // Look up field definition for element and length
                    let mut bytes: u64 = 0;
                    if let Some(fd) = class_registry
                        .get_by_id(instance.class_id)
                        .and_then(|def| def.fields.iter().find(|fd| fd.id == field.def_id))
                    {
                        let len = fd.array_length.unwrap_or(0) as u64;
                        let elem_size: u64 = match &fd.array_element {
                            Some(crate::memory::types::PointerTarget::FieldType(t)) => t.get_size(),
                            Some(crate::memory::types::PointerTarget::EnumId(eid)) => enum_registry
                                .get_by_id(*eid)
                                .map(|ed| ed.default_size as u64)
                                .unwrap_or(0),
                            Some(crate::memory::types::PointerTarget::ClassId(cid)) => {
                                class_registry
                                    .get_by_id(*cid)
                                    .map(|cd| cd.total_size)
                                    .unwrap_or(0)
                            }
                            Some(crate::memory::types::PointerTarget::Array { .. }) => 0,
                            None => 0,
                        };
                        bytes = elem_size.saturating_mul(len);
                    }
                    bytes
                }
                FieldType::Enum => {
                    let mut size_bytes: u64 = 4;
                    if let Some(fd) = class_registry
                        .get_by_id(instance.class_id)
                        .and_then(|def| def.fields.iter().find(|fd| fd.id == field.def_id))
                    {
                        if let Some(eid) = fd.enum_id {
                            if let Some(ed) = enum_registry.get_by_id(eid) {
                                size_bytes = ed.default_size as u64;
                            }
                        }
                    }
                    size_bytes
                }
                _ => field.get_size(),
            };
            current_offset = current_offset.saturating_add(advance);
        }
        instance.total_size = current_offset;
    }

    /// Update root class base address and recompute all field addresses/sizes
    pub fn set_root_address(&mut self, new_address: u64) {
        self.root_class.address = new_address;
        Self::recalc_instance_layout(
            &self.enum_registry,
            &self.class_registry,
            &mut self.root_class,
        );
    }

    /// Change the root class to a different class definition by name, preserving root name and address
    pub fn set_root_class_by_id(&mut self, class_id: u64) -> bool {
        if let Some(def) = self.class_registry.get(class_id).cloned() {
            let name = self.root_class.name.clone();
            let address = self.root_class.address;
            self.root_class = ClassInstance::new(name, address, def);
            let registry = self.class_registry.clone();
            Self::build_nested_for_instance(&registry, &mut self.root_class);
            Self::recalc_instance_layout(
                &self.enum_registry,
                &self.class_registry,
                &mut self.root_class,
            );
            true
        } else {
            false
        }
    }

    /// Check if assigning `target_class_id` to a field within `owner_class_id` would create a cycle
    pub fn would_create_cycle(&self, owner_class_id: u64, target_class_id: u64) -> bool {
        // If same class, direct self-cycle
        if owner_class_id == target_class_id {
            return true;
        }
        // DFS from target to see if we can reach owner
        let mut visited: HashSet<String> = HashSet::new();
        fn dfs(
            reg: &ClassDefinitionRegistry,
            current: u64,
            target: u64,
            visited: &mut HashSet<String>,
        ) -> bool {
            if !visited.insert(current.to_string()) {
                return false;
            }
            if let Some(def) = reg.get_by_id(current) {
                for f in &def.fields {
                    if f.field_type == FieldType::ClassInstance {
                        if let Some(cid) = f.class_id {
                            if cid == target {
                                return true;
                            }
                            if dfs(reg, cid, target, visited) {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }
        dfs(
            &self.class_registry,
            target_class_id,
            owner_class_id,
            &mut visited,
        )
    }

    #[allow(dead_code)]
    pub fn get_total_size(&self) -> u64 {
        self.root_class.get_size()
    }

    #[allow(dead_code)]
    pub fn get_available_classes(&self) -> Vec<u64> {
        self.class_registry.get_class_ids()
    }
}
