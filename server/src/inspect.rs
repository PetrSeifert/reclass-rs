//! Read-only decoding of an instance at any address, for scripts and agents.
//!
//! Unlike cards this touches no canvas state: embedded classes and arrays are
//! always expanded, and class pointers are followed up to `follow` levels deep.

use std::collections::HashSet;

use reclass_core::{
    decode::{
        element_field,
        Child,
        Decoder,
    },
    layout::hex_fill,
    memory::{
        FieldDefinition,
        FieldType,
        PointerTarget,
    },
};
use serde_json::{
    json,
    Map,
    Value,
};

use crate::canvas::hex;

/// Embedded classes and arrays nested deeper than this are not expanded.
const MAX_NESTING: u32 = 16;
/// Stops a wide pointer graph from producing an unbounded reply.
const MAX_ROWS: usize = 4000;

pub struct Inspector<'a> {
    dec: &'a Decoder<'a>,
    /// Array elements shown per array.
    elements: u32,
    visited: HashSet<(u64, u64)>,
    rows: usize,
    truncated: bool,
}

/// A field to decode plus the class that owns it, if it is editable.
type Field = (FieldDefinition, Option<u64>);

impl<'a> Inspector<'a> {
    pub fn new(dec: &'a Decoder<'a>, elements: u32) -> Self {
        Self {
            dec,
            elements,
            visited: HashSet::new(),
            rows: 0,
            truncated: false,
        }
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Rows of the class instance at `base`.
    pub fn class(&mut self, class_id: u64, base: u64, follow: u32) -> Vec<Value> {
        self.visited.insert((class_id, base));
        let fields = self.class_fields(class_id);
        self.walk(fields, base, follow, 0)
    }

    /// Rows of `size` bytes at `base` as hex fields, for memory with no class yet.
    pub fn span(&mut self, size: u64, base: u64) -> Vec<Value> {
        let fields = hex_fill(size)
            .into_iter()
            .map(|t| {
                let mut fd = element_field(&PointerTarget::FieldType(t), 0);
                fd.name = None;
                (fd, None)
            })
            .collect();
        self.walk(fields, base, 0, 0)
    }

    fn class_fields(&self, class_id: u64) -> Vec<Field> {
        self.dec
            .layout
            .classes
            .get(class_id)
            .map(|c| {
                c.fields
                    .iter()
                    .map(|f| (f.clone(), Some(class_id)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn walk(&mut self, fields: Vec<Field>, base: u64, follow: u32, nesting: u32) -> Vec<Value> {
        let mut out = Vec::new();
        let mut offset = 0;
        for (field, owner) in fields {
            if self.rows >= MAX_ROWS {
                self.truncated = true;
                break;
            }
            self.rows += 1;
            let size = self.dec.layout.field_size(&field);
            let address = base.wrapping_add(offset);
            let d = self.dec.decode(&field, address);
            let mut row = Map::new();
            row.insert("offset".into(), json!(offset));
            row.insert("size".into(), json!(size));
            row.insert("address".into(), json!(hex(address)));
            row.insert("type".into(), json!(self.dec.type_label(&field)));
            row.insert("name".into(), json!(field.name));
            row.insert("value".into(), json!(d.value));
            if !d.hints.is_empty() {
                row.insert("hints".into(), json!(d.hints));
            }
            if let Some(e) = &d.error {
                row.insert("error".into(), json!(e));
            }
            if let Some(class_id) = owner {
                row.insert("classId".into(), json!(class_id));
                row.insert("fieldId".into(), json!(field.id));
            }
            let is_pointer = matches!(
                field.field_type,
                FieldType::Pointer | FieldType::EncryptedPointer
            );
            let children = match d.child {
                Some(Child::Class {
                    class_id,
                    base,
                    via_pointer: false,
                }) if nesting < MAX_NESTING => {
                    let fields = self.class_fields(class_id);
                    Some(self.walk(fields, base, follow, nesting + 1))
                }
                Some(Child::Class {
                    class_id,
                    base,
                    via_pointer: true,
                }) if follow > 0 => {
                    // Each instance is shown once, so pointer cycles terminate.
                    if self.visited.insert((class_id, base)) {
                        let fields = self.class_fields(class_id);
                        Some(self.walk(fields, base, follow - 1, 0))
                    } else {
                        row.insert("seen".into(), json!(true));
                        None
                    }
                }
                Some(Child::Elements {
                    base,
                    element,
                    length,
                }) if nesting < MAX_NESTING && (!is_pointer || follow > 0) => {
                    let shown = length.min(self.elements);
                    if shown < length {
                        row.insert("elementsShown".into(), json!(shown));
                    }
                    let fields = (0..shown)
                        .map(|i| (element_field(&element, i), None))
                        .collect();
                    let follow = if is_pointer { follow - 1 } else { follow };
                    Some(self.walk(fields, base, follow, nesting + 1))
                }
                _ => None,
            };
            if let Some(children) = children {
                row.insert("children".into(), json!(children));
            }
            out.push(Value::Object(row));
            offset += size;
        }
        out
    }
}
