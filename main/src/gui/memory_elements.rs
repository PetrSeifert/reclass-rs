#[derive(Debug, Clone)]
pub enum ClassElement {
    Root,
    Pointer,
    Field,
}

#[derive(Debug, Clone, Default)]
pub enum FieldType {
    // Hex types
    #[default]
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
}

impl FieldType {
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
        }
    }

    pub fn is_hex_type(&self) -> bool {
        matches!(
            self,
            FieldType::Hex64 | FieldType::Hex32 | FieldType::Hex16 | FieldType::Hex8
        )
    }

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
            FieldType::TextPointer => "Text*",
        }
    }

    pub fn format_value(&self, data: &[u8], handle: Option<&handle::AppHandle>) -> String {
        if data.is_empty() {
            return "No data".to_string();
        }

        match self {
            FieldType::Hex64 => {
                if data.len() >= 8 {
                    format!(
                        "0x{:016X}",
                        u64::from_le_bytes([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]
                        ])
                    )
                } else {
                    format!("0x{:02X?}", data)
                }
            }
            FieldType::Hex32 => {
                if data.len() >= 4 {
                    format!(
                        "0x{:08X}",
                        u32::from_le_bytes([data[0], data[1], data[2], data[3]])
                    )
                } else {
                    format!("0x{:02X?}", data)
                }
            }
            FieldType::Hex16 => {
                if data.len() >= 2 {
                    format!("0x{:04X}", u16::from_le_bytes([data[0], data[1]]))
                } else {
                    format!("0x{:02X?}", data)
                }
            }
            FieldType::Hex8 => {
                if !data.is_empty() {
                    format!("0x{:02X}", data[0])
                } else {
                    "0x00".to_string()
                }
            }
            FieldType::Int64 => {
                if data.len() >= 8 {
                    format!(
                        "{}",
                        i64::from_le_bytes([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]
                        ])
                    )
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Int32 => {
                if data.len() >= 4 {
                    format!(
                        "{}",
                        i32::from_le_bytes([data[0], data[1], data[2], data[3]])
                    )
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Int16 => {
                if data.len() >= 2 {
                    format!("{}", i16::from_le_bytes([data[0], data[1]]))
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Int8 => {
                if !data.is_empty() {
                    format!("{}", data[0] as i8)
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::UInt64 => {
                if data.len() >= 8 {
                    format!(
                        "{}",
                        u64::from_le_bytes([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]
                        ])
                    )
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::UInt32 => {
                if data.len() >= 4 {
                    format!(
                        "{}",
                        u32::from_le_bytes([data[0], data[1], data[2], data[3]])
                    )
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::UInt16 => {
                if data.len() >= 2 {
                    format!("{}", u16::from_le_bytes([data[0], data[1]]))
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::UInt8 => {
                if !data.is_empty() {
                    format!("{}", data[0])
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Bool => {
                if !data.is_empty() {
                    format!("{}", data[0] != 0)
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Float => {
                if data.len() >= 4 {
                    let bytes = [data[0], data[1], data[2], data[3]];
                    format!("{:.6}", f32::from_le_bytes(bytes))
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Double => {
                if data.len() >= 8 {
                    let bytes = [
                        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                    ];
                    format!("{:.6}", f64::from_le_bytes(bytes))
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Vector4 => {
                if data.len() >= 16 {
                    let x = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    let y = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                    let z = f32::from_le_bytes([data[8], data[9], data[10], data[11]]);
                    let w = f32::from_le_bytes([data[12], data[13], data[14], data[15]]);
                    format!("({:.2}, {:.2}, {:.2}, {:.2})", x, y, z, w)
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Vector3 => {
                if data.len() >= 12 {
                    let x = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    let y = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                    let z = f32::from_le_bytes([data[8], data[9], data[10], data[11]]);
                    format!("({:.2}, {:.2}, {:.2})", x, y, z)
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Vector2 => {
                if data.len() >= 8 {
                    let x = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    let y = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                    format!("({:.2}, {:.2})", x, y)
                } else {
                    "Invalid".to_string()
                }
            }
            FieldType::Text => {
                // Try to convert bytes to string, stopping at null terminator
                let mut text_bytes = Vec::new();
                for &byte in data {
                    if byte == 0 {
                        break;
                    }
                    text_bytes.push(byte);
                }

                match String::from_utf8(text_bytes) {
                    Ok(text) => format!("\"{}\"", text),
                    Err(_) => format!("Invalid text: {:02X?}", data),
                }
            }
            FieldType::TextPointer => {
                if data.len() >= 8 {
                    let ptr = u64::from_le_bytes([
                        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                    ]);
                    if ptr == 0 {
                        "null".to_string()
                    } else {
                        // Try to dereference the pointer and read text
                        if let Some(handle) = handle {
                            let mut text_buffer = vec![0u8; 256]; // Read up to 256 bytes
                            match handle.read_slice(ptr, &mut text_buffer) {
                                Ok(_) => {
                                    // Find null terminator
                                    let mut text_bytes = Vec::new();
                                    for &byte in &text_buffer {
                                        if byte == 0 {
                                            break;
                                        }
                                        text_bytes.push(byte);
                                    }

                                    match String::from_utf8(text_bytes) {
                                        Ok(text) => format!("0x{:016X} -> \"{}\"", ptr, text),
                                        Err(_) => format!("0x{:016X} -> Invalid text", ptr),
                                    }
                                }
                                Err(_) => format!("0x{:016X} -> Failed to read", ptr),
                            }
                        } else {
                            format!("0x{:016X}", ptr)
                        }
                    }
                } else {
                    "Invalid".to_string()
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryElement {
    pub address: u64,
    pub size: u64,
    pub name: String,
    pub data: Option<Vec<u8>>,
    pub error: Option<String>,
    pub class_type: Option<ClassElement>,
    pub field_type: Option<FieldType>,
    pub is_editing: bool,
}

impl MemoryElement {
    pub fn new(address: u64, size: u64, name: String) -> Self {
        Self {
            address,
            size,
            name,
            data: None,
            error: None,
            class_type: None,
            field_type: None,
            is_editing: false,
        }
    }

    pub fn new_class(address: u64, size: u64, name: String, class_type: ClassElement) -> Self {
        Self {
            address,
            size,
            name,
            data: None,
            error: None,
            class_type: Some(class_type),
            field_type: None,
            is_editing: false,
        }
    }

    pub fn new_field(address: u64, name: String, field_type: FieldType) -> Self {
        let size = field_type.get_size();
        Self {
            address,
            size,
            name,
            data: None,
            error: None,
            class_type: Some(ClassElement::Field),
            field_type: Some(field_type),
            is_editing: false,
        }
    }

    #[allow(dead_code)]
    pub fn is_clickable(&self) -> bool {
        match &self.class_type {
            Some(ClassElement::Field) => true,
            Some(ClassElement::Root) | Some(ClassElement::Pointer) => false,
            None => true,
        }
    }

    #[allow(dead_code)]
    pub fn get_display_name(&self) -> String {
        match &self.class_type {
            Some(ClassElement::Root) => format!("[ROOT] {}", self.name),
            Some(ClassElement::Pointer) => format!("[PTR] {}", self.name),
            Some(ClassElement::Field) => {
                if let Some(field_type) = &self.field_type {
                    format!("[FLD] {}: {}", self.name, field_type.get_display_name())
                } else {
                    format!("[FLD] {}", self.name)
                }
            }
            None => self.name.clone(),
        }
    }

    pub fn get_formatted_value(&self, handle: Option<&handle::AppHandle>) -> String {
        if let Some(data) = &self.data {
            if let Some(field_type) = &self.field_type {
                field_type.format_value(data, handle)
            } else {
                format!("Data: {:02X?}", data)
            }
        } else {
            "No data".to_string()
        }
    }
}
