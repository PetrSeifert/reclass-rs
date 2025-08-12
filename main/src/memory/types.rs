use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};

/// Represents all possible field types in the memory structure
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FieldType {
    // Hex types (no names)
    Hex64,
    Hex32,
    Hex16,
    Hex8,

    // Signed integer types
    Int64,
    Int32,
    Int16,
    Int8,

    // Unsigned integer types
    UInt64,
    UInt32,
    UInt16,
    UInt8,

    // Boolean type
    Bool,

    // Floating point types
    Float,
    Double,

    // Vector types
    Vector4,
    Vector3,
    Vector2,

    // Text types
    Text,
    TextPointer,

    // Class instance type (dynamic size)
    ClassInstance,

    // Generic pointer (64-bit) that can point to any primitive type or class instance
    Pointer,

    // Enum type (32-bit underlying by default)
    Enum,

    // Array type (dynamic size; element type and length stored in FieldDefinition)
    Array,
}

impl FieldType {
    /// Get the fixed size of the field type in bytes
    pub fn get_size(&self) -> u64 {
        match self {
            FieldType::Hex64 | FieldType::Int64 | FieldType::UInt64 | FieldType::Double => 8,
            FieldType::Hex32
            | FieldType::Int32
            | FieldType::UInt32
            | FieldType::Float
            | FieldType::Vector2 => 4,
            FieldType::Hex16 | FieldType::Int16 | FieldType::UInt16 => 2,
            FieldType::Hex8 | FieldType::Int8 | FieldType::UInt8 | FieldType::Bool => 1,
            FieldType::Vector3 => 12,
            FieldType::Vector4 => 16,
            FieldType::Text => 32,
            FieldType::TextPointer => 8,
            FieldType::Pointer => 8,
            FieldType::Enum => 4,
            FieldType::Array => 0, // Dynamic size; depends on element and length
            FieldType::ClassInstance => 0, // Dynamic size
        }
    }

    /// Check if this is a hex type (which don't have names)
    pub fn is_hex_type(&self) -> bool {
        matches!(
            self,
            FieldType::Hex64 | FieldType::Hex32 | FieldType::Hex16 | FieldType::Hex8
        )
    }

    /// Check if this field type has a dynamic size
    pub fn is_dynamic_size(&self) -> bool {
        matches!(self, FieldType::ClassInstance | FieldType::Array)
    }

    /// Get the display name for this field type
    pub fn get_display_name(&self) -> &'static str {
        match self {
            FieldType::Hex64 => "Hex64",
            FieldType::Hex32 => "Hex32",
            FieldType::Hex16 => "Hex16",
            FieldType::Hex8 => "Hex8",
            FieldType::Int64 => "Int64",
            FieldType::Int32 => "Int32",
            FieldType::Int16 => "Int16",
            FieldType::Int8 => "Int8",
            FieldType::UInt64 => "UInt64",
            FieldType::UInt32 => "UInt32",
            FieldType::UInt16 => "UInt16",
            FieldType::UInt8 => "UInt8",
            FieldType::Bool => "Bool",
            FieldType::Float => "Float",
            FieldType::Double => "Double",
            FieldType::Vector4 => "Vector4",
            FieldType::Vector3 => "Vector3",
            FieldType::Vector2 => "Vector2",
            FieldType::Text => "Text",
            FieldType::TextPointer => "TextPointer",
            FieldType::ClassInstance => "ClassInstance",
            FieldType::Pointer => "Pointer",
            FieldType::Enum => "Enum",
            FieldType::Array => "Array",
        }
    }
}

impl fmt::Display for FieldType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_display_name())
    }
}

/// Target information for a `FieldType::Pointer`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PointerTarget {
    /// Pointer to a primitive/inline field type (e.g., Int32, Float, TextPointer, etc.)
    FieldType(FieldType),
    /// Pointer to a class instance by id
    ClassId(u64),
    /// Pointer to a specific enum definition by id
    EnumId(u64),
    /// Pointer to an array at the target address (element descriptor and length)
    Array {
        element: Box<PointerTarget>,
        length: u32,
    },
}
