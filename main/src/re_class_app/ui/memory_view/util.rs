use std::sync::Arc;

use eframe::egui::{
    self,
    Color32,
    TextEdit,
    TextStyle,
    Ui,
};
use handle::AppHandle;

use crate::memory::{
    FieldType,
    MemoryField,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldKey {
    pub instance_address: u64,
    pub field_def_id: u64,
}

pub fn parse_hex_u64(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(stripped) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).ok()
    } else {
        t.parse::<u64>().ok()
    }
}

pub fn text_edit_autowidth(ui: &mut Ui, text: &mut String) -> egui::Response {
    let display = if text.is_empty() {
        " ".to_string()
    } else {
        text.clone()
    };
    let galley =
        ui.painter()
            .layout_no_wrap(display, TextStyle::Body.resolve(ui.style()), Color32::WHITE);
    let width = galley.rect.width() + 12.0;
    ui.add_sized(
        [width, ui.text_style_height(&TextStyle::Body)],
        TextEdit::singleline(text),
    )
}

pub fn field_value_string(handle: Option<Arc<AppHandle>>, field: &MemoryField) -> Option<String> {
    let handle = handle.as_ref()?;
    let addr = field.address;
    match field.field_type {
        FieldType::Hex64 => handle
            .read_sized::<u64>(addr)
            .ok()
            .map(|v| format!("0x{v:016X}")),
        FieldType::Hex32 => handle
            .read_sized::<u32>(addr)
            .ok()
            .map(|v| format!("0x{v:08X}")),
        FieldType::Hex16 => handle
            .read_sized::<u16>(addr)
            .ok()
            .map(|v| format!("0x{v:04X}")),
        FieldType::Hex8 => handle
            .read_sized::<u8>(addr)
            .ok()
            .map(|v| format!("0x{v:02X}")),

        FieldType::UInt64 => handle.read_sized::<u64>(addr).ok().map(|v| v.to_string()),
        FieldType::UInt32 => handle.read_sized::<u32>(addr).ok().map(|v| v.to_string()),
        FieldType::UInt16 => handle.read_sized::<u16>(addr).ok().map(|v| v.to_string()),
        FieldType::UInt8 => handle.read_sized::<u8>(addr).ok().map(|v| v.to_string()),

        FieldType::Int64 => handle.read_sized::<i64>(addr).ok().map(|v| v.to_string()),
        FieldType::Int32 => handle.read_sized::<i32>(addr).ok().map(|v| v.to_string()),
        FieldType::Int16 => handle.read_sized::<i16>(addr).ok().map(|v| v.to_string()),
        FieldType::Int8 => handle.read_sized::<i8>(addr).ok().map(|v| v.to_string()),

        FieldType::Bool => handle.read_sized::<u8>(addr).ok().map(|v| {
            if v != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }),
        FieldType::Float => handle.read_sized::<f32>(addr).ok().map(|v| format!("{v}")),
        FieldType::Double => handle.read_sized::<f64>(addr).ok().map(|v| format!("{v}")),

        FieldType::Vector3 | FieldType::Vector4 | FieldType::Vector2 => {
            let len = field.get_size() as usize;
            let mut buf = vec![0u8; len];
            (handle.read_slice(addr, buf.as_mut_slice()).ok()).map(|_| {
                buf.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        }

        FieldType::Text => handle.read_string(addr, Some(32)).ok(),
        FieldType::TextPointer => {
            if let Ok(ptr) = handle.read_sized::<u64>(addr) {
                if ptr != 0 {
                    handle.read_string(ptr, None).ok()
                } else {
                    Some(String::from("(null)"))
                }
            } else {
                None
            }
        }

        FieldType::Pointer => None,
        FieldType::Array => None,
        FieldType::ClassInstance => None,
        FieldType::Enum => None,
    }
}
