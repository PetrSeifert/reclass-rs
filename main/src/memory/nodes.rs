use std::collections::HashSet;

use serde::{
    Deserialize,
    Serialize,
};

use crate::memory::{
    definitions::{
        ClassDefinition,
        ClassDefinitionRegistry,
    },
    types::FieldType,
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

    fn create_fields_from_definition(&mut self) {
        let mut current_offset = 0;

        for field_def in &self.class_definition.fields {
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
    pub fn get_display_name(&self) -> String {
        format!("{}: {}", self.name, self.class_definition.name)
    }
}

/// Represents the root memory structure
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn rename_class(&mut self, old_name: &str, new_name: &str) -> bool {
        if old_name == new_name || old_name.is_empty() || new_name.is_empty() {
            return false;
        }

        if !self.class_registry.contains(old_name) {
            return false;
        }
        if self.class_registry.contains(new_name) {
            return false;
        }
        let mut moved_def_opt = self.class_registry.remove(old_name);

        let class_names = self.class_registry.get_class_names();
        for cname in class_names {
            if let Some(def_mut) = self.class_registry.get_mut(&cname) {
                for f in &mut def_mut.fields {
                    if f.field_type == FieldType::ClassInstance {
                        if let Some(ref cn) = f.class_name {
                            if cn.eq_ignore_ascii_case(old_name) {
                                f.class_name = Some(new_name.to_string());
                            }
                        }
                    }
                }
            }
        }

        if let Some(ref mut moved_def) = moved_def_opt {
            for f in &mut moved_def.fields {
                if f.field_type == FieldType::ClassInstance {
                    if let Some(ref cn) = f.class_name {
                        if cn.eq_ignore_ascii_case(old_name) {
                            f.class_name = Some(new_name.to_string());
                        }
                    }
                }
            }
        }

        Self::rename_in_instance(&mut self.root_class, old_name, new_name);
        let registry_clone = self.class_registry.clone();
        Self::build_nested_for_instance(&registry_clone, &mut self.root_class);
        Self::recalc_instance_layout(&mut self.root_class);

        if let Some(mut def) = moved_def_opt.take() {
            def.rename(new_name.to_string());
            self.class_registry.register(def);
        }
        true
    }

    fn rename_in_instance(instance: &mut ClassInstance, old_name: &str, new_name: &str) {
        if instance
            .class_definition
            .name
            .eq_ignore_ascii_case(old_name)
        {
            instance.class_definition.name = new_name.to_string();
        }
        for f in &mut instance.class_definition.fields {
            if f.field_type == FieldType::ClassInstance {
                if let Some(ref cn) = f.class_name {
                    if cn.eq_ignore_ascii_case(old_name) {
                        f.class_name = Some(new_name.to_string());
                    }
                }
            }
        }
        for field in &mut instance.fields {
            if let Some(ref mut nested) = field.nested_instance {
                Self::rename_in_instance(nested, old_name, new_name);
            }
        }
    }

    #[cfg(test)]
    pub fn register_class(&mut self, class_def: ClassDefinition) {
        self.class_registry.register(class_def);
    }

    #[cfg(test)]
    pub fn get_class_definition(&self, name: &str) -> Option<&ClassDefinition> {
        self.class_registry.get(name)
    }

    #[cfg(test)]
    pub fn create_class_instance(
        &mut self,
        name: String,
        address: u64,
        class_name: &str,
    ) -> Option<ClassInstance> {
        self.class_registry
            .get(class_name)
            .map(|class_def| ClassInstance::new(name, address, class_def.clone()))
    }

    pub fn create_nested_instances(&mut self) {
        let registry = self.class_registry.clone();
        Self::build_nested_for_instance(&registry, &mut self.root_class);
        Self::recalc_instance_layout(&mut self.root_class);
    }

    pub fn rebuild_root_from_registry(&mut self) {
        let root_type = self.root_class.class_definition.name.clone();
        if let Some(def) = self.class_registry.get(&root_type).cloned() {
            let name = self.root_class.name.clone();
            let address = self.root_class.address;
            self.root_class = ClassInstance::new(name, address, def);
            let registry = self.class_registry.clone();
            Self::build_nested_for_instance(&registry, &mut self.root_class);
            Self::recalc_instance_layout(&mut self.root_class);
        }
    }

    fn build_nested_for_instance(registry: &ClassDefinitionRegistry, instance: &mut ClassInstance) {
        for field in &mut instance.fields {
            if field.field_type == FieldType::ClassInstance {
                // Prefer matching by definition id; fallback to name if needed
                let field_def_opt = instance
                    .class_definition
                    .fields
                    .iter()
                    .find(|fd| fd.id == field.def_id)
                    .or_else(|| {
                        field.name.as_ref().and_then(|n| {
                            instance
                                .class_definition
                                .fields
                                .iter()
                                .find(|fd| fd.name.as_ref().map(|nn| nn == n).unwrap_or(false))
                        })
                    });

                if let Some(field_def) = field_def_opt {
                    if let Some(class_name) = &field_def.class_name {
                        if let Some(class_def) = registry.get(class_name) {
                            // Always create a fresh instance and clear any stale nested linkage
                            field.nested_instance = None;
                            let mut nested_instance = ClassInstance::new(
                                field.name.clone().unwrap_or_default(),
                                field.address,
                                class_def.clone(),
                            );
                            Self::build_nested_for_instance(registry, &mut nested_instance);
                            Self::recalc_instance_layout(&mut nested_instance);
                            field.nested_instance = Some(nested_instance);
                            continue;
                        }
                    }
                }
                // If no mapping found, keep None
            } else {
                // Ensure primitive fields do not retain stale nested instances
                field.nested_instance = None;
            }
        }
        Self::recalc_instance_layout(instance);
    }

    fn recalc_instance_layout(instance: &mut ClassInstance) {
        let mut current_offset: u64 = 0;
        for field in &mut instance.fields {
            field.address = instance.address + current_offset;
            let advance = match field.field_type {
                FieldType::ClassInstance => {
                    if let Some(ref mut nested) = field.nested_instance {
                        nested.address = field.address;
                        Self::recalc_instance_layout(nested);
                        nested.total_size.min(1_048_576)
                    } else {
                        0
                    }
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
        Self::recalc_instance_layout(&mut self.root_class);
    }

    /// Change the root class to a different class definition by name, preserving root name and address
    pub fn set_root_class_by_name(&mut self, class_name: &str) -> bool {
        if let Some(def) = self.class_registry.get(class_name).cloned() {
            let name = self.root_class.name.clone();
            let address = self.root_class.address;
            self.root_class = ClassInstance::new(name, address, def);
            let registry = self.class_registry.clone();
            Self::build_nested_for_instance(&registry, &mut self.root_class);
            Self::recalc_instance_layout(&mut self.root_class);
            true
        } else {
            false
        }
    }

    /// Check if assigning `target_class_name` to a field within `owner_class_name` would create a cycle
    pub fn would_create_cycle(&self, owner_class_name: &str, target_class_name: &str) -> bool {
        // If same class, direct self-cycle
        if owner_class_name.eq_ignore_ascii_case(target_class_name) {
            return true;
        }
        // DFS from target to see if we can reach owner
        let mut visited: HashSet<String> = HashSet::new();
        fn dfs(
            reg: &ClassDefinitionRegistry,
            current: &str,
            target: &str,
            visited: &mut HashSet<String>,
        ) -> bool {
            if !visited.insert(current.to_string()) {
                return false;
            }
            if let Some(def) = reg.get(current) {
                for f in &def.fields {
                    if f.field_type == FieldType::ClassInstance {
                        if let Some(ref cn) = f.class_name {
                            if cn.eq_ignore_ascii_case(target) {
                                return true;
                            }
                            if dfs(reg, cn, target, visited) {
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
            target_class_name,
            owner_class_name,
            &mut visited,
        )
    }

    #[allow(dead_code)]
    pub fn get_total_size(&self) -> u64 {
        self.root_class.get_size()
    }

    #[allow(dead_code)]
    pub fn get_available_classes(&self) -> Vec<String> {
        self.class_registry.get_class_names()
    }
}
