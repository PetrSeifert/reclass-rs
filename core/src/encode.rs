//! Turns a value typed for a field back into the bytes to write: the inverse
//! of `decode`, accepting what it displays.

use crate::{
    layout::Layout,
    memory::{
        FieldDefinition,
        FieldType,
    },
    scan::parse_int,
};

/// The bytes that store `text` in a field of `fd`'s type. Hex fields take hex
/// digits, pointers a hex address or `nullptr`, enums a variant name, a number
/// or `A | B` for flags, and text a string that leaves room for its NUL.
pub fn encode(layout: &Layout, fd: &FieldDefinition, text: &str) -> Result<Vec<u8>, String> {
    use FieldType::*;
    let size = layout.field_size(fd) as usize;
    let t = text.trim();
    match fd.field_type {
        Hex64 | Hex32 | Hex16 | Hex8 => {
            let digits: String = t
                .trim_start_matches("0x")
                .trim_start_matches("0X")
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '_')
                .collect();
            let v = u64::from_str_radix(&digits, 16).map_err(|_| format!("'{t}' is not hex"))?;
            unsigned(v, size)
        }
        Int64 | Int32 | Int16 | Int8 => int(t, size, true),
        UInt64 | UInt32 | UInt16 | UInt8 => int(t, size, false),
        Bool => match t.to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(vec![1]),
            "false" | "0" => Ok(vec![0]),
            _ => Err(format!("'{t}' is not true or false")),
        },
        Float => Ok((float(t)? as f32).to_le_bytes().to_vec()),
        Double => Ok(float(t)?.to_le_bytes().to_vec()),
        Vector2 | Vector3 | Vector4 => {
            let parts: Vec<&str> = t
                .trim_start_matches('(')
                .trim_end_matches(')')
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() != size / 4 {
                return Err(format!("{} takes {} numbers", fd.field_type, size / 4));
            }
            let mut out = Vec::with_capacity(size);
            for p in parts {
                out.extend_from_slice(&(float(p)? as f32).to_le_bytes());
            }
            Ok(out)
        }
        Text => {
            // The display wraps text in quotes; spaces inside them are kept.
            let s = text
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(text);
            if s.len() >= size {
                return Err(format!("text is longer than {} bytes", size - 1));
            }
            let mut out = s.as_bytes().to_vec();
            out.push(0);
            Ok(out)
        }
        Pointer => {
            if t == "nullptr" {
                return Ok(vec![0; size]);
            }
            let digits = t.trim_start_matches("0x").trim_start_matches("0X");
            let v = u64::from_str_radix(digits, 16)
                .map_err(|_| format!("'{t}' is not a hex address"))?;
            unsigned(v, size)
        }
        Enum => {
            let variants = fd
                .enum_id
                .and_then(|id| layout.enums.get(id))
                .map(|e| e.variants.as_slice())
                .unwrap_or_default();
            let mut v = 0u64;
            for part in t.split('|').map(str::trim) {
                v |= match variants.iter().find(|x| x.name.eq_ignore_ascii_case(part)) {
                    Some(x) => x.value as u64,
                    None => parse_int(part)
                        .filter(|n| *n >= 0 && *n <= u64::MAX as i128)
                        .ok_or_else(|| format!("'{part}' is not a variant or a number"))?
                        as u64,
                };
            }
            unsigned(v, size)
        }
        EncryptedPointer => Err("encrypted pointers cannot be written".into()),
        TextPointer => Err("write the text it points to instead".into()),
        ClassInstance | Array => Err("write the fields inside instead".into()),
    }
}

fn bytes(n: usize) -> String {
    if n == 1 {
        "1 byte".into()
    } else {
        format!("{n} bytes")
    }
}

fn unsigned(v: u64, size: usize) -> Result<Vec<u8>, String> {
    if size < 8 && v >> (size * 8) != 0 {
        return Err(format!("0x{v:X} does not fit in {}", bytes(size)));
    }
    Ok(v.to_le_bytes()[..size].to_vec())
}

/// Signed types also take their unsigned spelling, e.g. 0xFFFFFFFF for -1.
fn int(t: &str, size: usize, signed: bool) -> Result<Vec<u8>, String> {
    let v = parse_int(t).ok_or_else(|| format!("'{t}' is not an integer (use 0x for hex)"))?;
    let bits = size as u32 * 8;
    let min = if signed { -(1i128 << (bits - 1)) } else { 0 };
    if v < min || v > (1i128 << bits) - 1 {
        return Err(format!("{v} does not fit in {}", bytes(size)));
    }
    Ok((v as u128 as u64).to_le_bytes()[..size].to_vec())
}

fn float(t: &str) -> Result<f64, String> {
    t.trim()
        .parse()
        .map_err(|_| format!("'{}' is not a number", t.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        edit,
        memory::{
            ClassDefinition,
            MemoryStructure,
        },
    };

    fn ms() -> MemoryStructure {
        MemoryStructure::new("root".into(), 0, ClassDefinition::new("Root".into()))
    }

    fn enc(ms: &MemoryStructure, fd: &FieldDefinition, text: &str) -> Result<Vec<u8>, String> {
        encode(&Layout::of(ms), fd, text)
    }

    fn field(t: FieldType) -> FieldDefinition {
        FieldDefinition::new_named("f".into(), t, 0)
    }

    #[test]
    fn numbers_take_their_type_width() {
        let ms = ms();
        assert_eq!(enc(&ms, &field(FieldType::Int32), "-1"), Ok(vec![0xFF; 4]));
        assert_eq!(
            enc(&ms, &field(FieldType::Int16), "0xFFFF"),
            Ok(vec![0xFF; 2])
        );
        assert!(enc(&ms, &field(FieldType::UInt8), "256").is_err());
        assert!(enc(&ms, &field(FieldType::UInt8), "-1").is_err());
        assert_eq!(
            enc(&ms, &field(FieldType::Hex32), "0000002A"),
            Ok(vec![42, 0, 0, 0])
        );
        assert!(enc(&ms, &field(FieldType::Hex16), "12345").is_err());
        assert_eq!(
            enc(&ms, &field(FieldType::Float), "100"),
            Ok(100f32.to_le_bytes().to_vec())
        );
        assert_eq!(enc(&ms, &field(FieldType::Bool), "true"), Ok(vec![1]));
    }

    #[test]
    fn displayed_values_round_trip() {
        let ms = ms();
        let v = enc(&ms, &field(FieldType::Vector3), "(1.0, -2.5, 3.0)").unwrap();
        assert_eq!(&v[4..8], &(-2.5f32).to_le_bytes());
        assert!(enc(&ms, &field(FieldType::Vector3), "1 2").is_err());
        assert_eq!(
            enc(&ms, &field(FieldType::Text), "\"hi there\"").unwrap(),
            b"hi there\0"
        );
        assert!(enc(&ms, &field(FieldType::Text), &"x".repeat(32)).is_err());
        assert_eq!(
            enc(&ms, &field(FieldType::Pointer), "nullptr"),
            Ok(vec![0; 8])
        );
        assert_eq!(
            enc(&ms, &field(FieldType::Pointer), "1F3A8C40000"),
            Ok(0x1F3A8C40000u64.to_le_bytes().to_vec())
        );
        assert!(enc(&ms, &field(FieldType::EncryptedPointer), "0").is_err());
    }

    #[test]
    fn pointers_follow_the_target_width() {
        let mut ms = ms();
        edit::set_pointer_size(&mut ms, 4).unwrap();
        assert_eq!(
            enc(&ms, &field(FieldType::Pointer), "0x10"),
            Ok(vec![0x10, 0, 0, 0])
        );
        assert!(enc(&ms, &field(FieldType::Pointer), "1F3A8C40000").is_err());
    }

    #[test]
    fn enums_take_names_numbers_and_flags() {
        let mut ms = ms();
        let id = edit::add_enum(&mut ms, Some("Flags"), Some(2)).unwrap();
        edit::update_enum(
            &mut ms,
            id,
            "Flags",
            true,
            2,
            vec![("Alive".into(), 1), ("Bot".into(), 4)],
        )
        .unwrap();
        let mut fd = field(FieldType::Enum);
        fd.enum_id = Some(id);
        assert_eq!(enc(&ms, &fd, "alive | Bot"), Ok(vec![5, 0]));
        assert_eq!(enc(&ms, &fd, "0x100"), Ok(vec![0, 1]));
        assert!(enc(&ms, &fd, "Dead").is_err());
    }
}
