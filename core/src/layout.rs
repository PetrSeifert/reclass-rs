//! Byte layout of class definitions.
//!
//! `FieldDefinition::offset` ignores dynamically sized fields (embedded classes,
//! arrays), so views compute offsets here from the registries instead.

use crate::memory::{
    ClassDefinitionRegistry,
    EnumDefinitionRegistry,
    FieldDefinition,
    FieldType,
    MemoryStructure,
    PointerTarget,
};

/// Embedded classes nested deeper than this count as zero-sized (guards against cycles).
const MAX_DEPTH: u32 = 16;

#[derive(Clone, Copy)]
pub struct Layout<'a> {
    pub classes: &'a ClassDefinitionRegistry,
    pub enums: &'a EnumDefinitionRegistry,
}

impl<'a> Layout<'a> {
    pub fn of(ms: &'a MemoryStructure) -> Self {
        Self {
            classes: &ms.class_registry,
            enums: &ms.enum_registry,
        }
    }

    pub fn enum_size(&self, fd: &FieldDefinition) -> u64 {
        if let Some(size) = fd.enum_size {
            return size as u64;
        }
        fd.enum_id
            .and_then(|id| self.enums.get(id))
            .map(|e| e.default_size as u64)
            .unwrap_or(4)
    }

    pub fn field_size(&self, fd: &FieldDefinition) -> u64 {
        self.field_size_at(fd, 0)
    }

    fn field_size_at(&self, fd: &FieldDefinition, depth: u32) -> u64 {
        match fd.field_type {
            FieldType::Enum => self.enum_size(fd),
            FieldType::ClassInstance => fd
                .class_id
                .map(|cid| self.class_size_at(cid, depth + 1))
                .unwrap_or(0),
            FieldType::Array => match &fd.array_element {
                Some(elem) => self
                    .target_size_at(elem, depth + 1)
                    .saturating_mul(fd.array_length.unwrap_or(0) as u64),
                None => 0,
            },
            ref t => t.get_size(),
        }
    }

    /// Size of one value described by a pointer/array target.
    pub fn target_size(&self, target: &PointerTarget) -> u64 {
        self.target_size_at(target, 0)
    }

    fn target_size_at(&self, target: &PointerTarget, depth: u32) -> u64 {
        match target {
            PointerTarget::FieldType(t) => t.get_size(),
            PointerTarget::EnumId(id) => self
                .enums
                .get(*id)
                .map(|e| e.default_size as u64)
                .unwrap_or(4),
            PointerTarget::ClassId(cid) => self.class_size_at(*cid, depth),
            PointerTarget::Array { element, length } => self
                .target_size_at(element, depth + 1)
                .saturating_mul(*length as u64),
            PointerTarget::Pointer(_) => 8,
        }
    }

    pub fn class_size(&self, class_id: u64) -> u64 {
        self.class_size_at(class_id, 0)
    }

    fn class_size_at(&self, class_id: u64, depth: u32) -> u64 {
        if depth > MAX_DEPTH {
            return 0;
        }
        self.classes
            .get(class_id)
            .map(|c| c.fields.iter().map(|f| self.field_size_at(f, depth)).sum())
            .unwrap_or(0)
    }

    /// `(offset, size)` for each field of the class, in order.
    pub fn field_offsets(&self, class_id: u64) -> Vec<(u64, u64)> {
        let Some(def) = self.classes.get(class_id) else {
            return Vec::new();
        };
        let mut offset = 0;
        def.fields
            .iter()
            .map(|f| {
                let size = self.field_size(f);
                let entry = (offset, size);
                offset += size;
                entry
            })
            .collect()
    }
}

/// Hex fields covering exactly `bytes` bytes, largest first.
pub fn hex_fill(mut bytes: u64) -> Vec<FieldType> {
    let mut out = Vec::new();
    for (t, s) in [
        (FieldType::Hex64, 8),
        (FieldType::Hex32, 4),
        (FieldType::Hex16, 2),
        (FieldType::Hex8, 1),
    ] {
        while bytes >= s {
            out.push(t.clone());
            bytes -= s;
        }
    }
    out
}
