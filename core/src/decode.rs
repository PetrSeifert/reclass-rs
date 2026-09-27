//! Turns a field definition plus live memory into display text.

use serde::Serialize;

use crate::{
    layout::Layout,
    memory::{
        FieldDefinition,
        FieldType,
        PointerTarget,
    },
    source::{
        MemorySource,
        ModuleEntry,
    },
};

/// Colour category used by frontends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Hex,
    Int,
    Float,
    Bool,
    Vec,
    Text,
    Ptr,
    Enum,
    Class,
    Array,
}

pub fn category(t: &FieldType) -> Category {
    use FieldType::*;
    match t {
        Hex64 | Hex32 | Hex16 | Hex8 => Category::Hex,
        Int64 | Int32 | Int16 | Int8 | UInt64 | UInt32 | UInt16 | UInt8 => Category::Int,
        Float | Double => Category::Float,
        Bool => Category::Bool,
        Vector2 | Vector3 | Vector4 => Category::Vec,
        Text | TextPointer => Category::Text,
        Pointer | EncryptedPointer => Category::Ptr,
        Enum => Category::Enum,
        ClassInstance => Category::Class,
        Array => Category::Array,
    }
}

/// What a field leads to.
#[derive(Clone, Debug, PartialEq)]
pub enum Child {
    /// A class instance at `base`, reached through a pointer (`via_pointer`) or embedded inline.
    Class {
        class_id: u64,
        base: u64,
        via_pointer: bool,
    },
    /// `length` elements of `element` starting at `base`.
    Elements {
        base: u64,
        element: PointerTarget,
        length: u32,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Decoded {
    pub value: String,
    pub hints: Vec<String>,
    pub error: Option<String>,
    /// Up to 16 raw bytes at the field address.
    pub bytes: Vec<u8>,
    /// Pointer value after decryption, for pointer-like fields.
    pub pointer: Option<u64>,
    pub child: Option<Child>,
}

pub fn fmt_float(v: f64) -> String {
    if !v.is_finite() {
        return v.to_string();
    }
    let a = v.abs();
    if a != 0.0 && !(1e-4..1e7).contains(&a) {
        return format!("{v:.3e}");
    }
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    let s = if s == "-0" { "0" } else { s };
    if s.contains('.') {
        s.to_string()
    } else {
        format!("{s}.0")
    }
}

/// Whether a 32-bit pattern looks like a float someone stored on purpose:
/// zero, or a finite magnitude well clear of denormals and huge exponents.
fn plausible_f32(bits: u32) -> Option<f64> {
    let f = f32::from_bits(bits) as f64;
    (bits == 0 || (1e-4..1e7).contains(&f.abs())).then_some(f)
}

/// Both halves of a qword as floats, e.g. an (x, y) pair, when both look intended.
fn float_pair(n: u64) -> Option<(f64, f64)> {
    Some((plausible_f32(n as u32)?, plausible_f32((n >> 32) as u32)?))
}

fn ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (32..127).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

/// Decodes fields of one attached process. Caches the module list for symbolization.
pub struct Decoder<'a> {
    pub source: &'a dyn MemorySource,
    pub layout: Layout<'a>,
    modules: Vec<ModuleEntry>,
}

impl<'a> Decoder<'a> {
    pub fn new(source: &'a dyn MemorySource, layout: Layout<'a>) -> Self {
        Self {
            modules: source.modules(),
            source,
            layout,
        }
    }

    pub fn symbolize(&self, address: u64) -> Option<String> {
        self.modules
            .iter()
            .find(|m| address >= m.base && address < m.base + m.size)
            .map(|m| format!("{}+0x{:X}", m.name, address - m.base))
    }

    fn readable(&self, address: u64) -> bool {
        address != 0 && self.source.read(address, &mut [0u8; 1]).is_ok()
    }

    pub fn target_label(&self, t: &PointerTarget) -> String {
        target_label(&self.layout, t)
    }

    pub fn type_label(&self, fd: &FieldDefinition) -> String {
        type_label(&self.layout, fd)
    }

    pub fn decode(&self, fd: &FieldDefinition, address: u64) -> Decoded {
        let size = self.layout.field_size(fd);
        let mut d = Decoded::default();
        let peek = size.clamp(1, 16) as usize;
        match self.source.read_vec(address, peek) {
            Some(b) => d.bytes = b,
            None => {
                d.error = Some("unreadable".into());
                d.value = "<unreadable>".into();
                return d;
            }
        }
        let b = d.bytes.clone();
        let u64_at = |i: usize| {
            b.get(i..i + 8)
                .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
        };
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let f32_at = |i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as f64;
        use FieldType::*;
        match fd.field_type {
            Hex64 | Hex32 | Hex16 | Hex8 => self.decode_hex(&mut d, size as usize),
            Int64 => d.value = (u64_at(0).unwrap() as i64).to_string(),
            Int32 => d.value = (u32_at(0) as i32).to_string(),
            Int16 => d.value = i16::from_le_bytes([b[0], b[1]]).to_string(),
            Int8 => d.value = (b[0] as i8).to_string(),
            UInt64 => d.value = u64_at(0).unwrap().to_string(),
            UInt32 => d.value = u32_at(0).to_string(),
            UInt16 => d.value = u16::from_le_bytes([b[0], b[1]]).to_string(),
            UInt8 => d.value = b[0].to_string(),
            Bool => d.value = (b[0] != 0).to_string(),
            Float => d.value = fmt_float(f32_at(0)),
            Double => d.value = fmt_float(f64::from_bits(u64_at(0).unwrap())),
            Vector2 | Vector3 | Vector4 => {
                let parts: Vec<String> = (0..size as usize / 4)
                    .map(|i| fmt_float(f32_at(i * 4)))
                    .collect();
                d.value = format!("({})", parts.join(", "));
            }
            Text => {
                let s = self.source.read_c_string(address, 32).unwrap_or_default();
                d.value = format!("\"{s}\"");
            }
            TextPointer => {
                let p = u64_at(0).unwrap();
                d.pointer = Some(p);
                d.value = match (p, self.source.read_c_string(p, 64)) {
                    (0, _) => "nullptr".into(),
                    (_, Some(s)) => format!("\"{s}\""),
                    (_, None) => "<bad ptr>".into(),
                };
                d.hints.push(format!("@ {p:012X}"));
            }
            Pointer | EncryptedPointer => self.decode_pointer(&mut d, fd, u64_at(0).unwrap()),
            Enum => self.decode_enum(&mut d, fd, size as usize),
            ClassInstance => match fd.class_id.and_then(|id| self.layout.classes.get(id)) {
                Some(c) => {
                    d.value = format!("{{ {} }}", c.name);
                    d.child = Some(Child::Class {
                        class_id: c.id,
                        base: address,
                        via_pointer: false,
                    });
                }
                None => d.value = "{ ? }".into(),
            },
            Array => {
                let length = fd.array_length.unwrap_or(0);
                d.value = format!(
                    "[{length} × {}]",
                    fd.array_element
                        .as_ref()
                        .map(|t| self.target_label(t))
                        .unwrap_or_default()
                );
                if let Some(element) = fd.array_element.clone() {
                    d.child = Some(Child::Elements {
                        base: address,
                        element,
                        length,
                    });
                }
            }
        }
        d
    }

    fn decode_hex(&self, d: &mut Decoded, size: usize) {
        let b = &d.bytes;
        let mut n = 0u64;
        for (i, x) in b.iter().take(size).enumerate() {
            n |= (*x as u64) << (8 * i);
        }
        d.value = format!("{n:0width$X}", width = size * 2);
        if n != 0 {
            match size {
                8 => {
                    if let Some(sym) = self.symbolize(n) {
                        d.hints.push(format!("-> {sym}"));
                    } else if let Some((lo, hi)) = float_pair(n) {
                        d.hints
                            .push(format!("f32 {}, {}", fmt_float(lo), fmt_float(hi)));
                    } else if n < 0x1_0000_0000 {
                        d.hints.push(format!("int {n}"));
                    } else if self.readable(n) {
                        d.hints.push("-> ptr".into());
                    } else if (n >> 32) < 100_000 && (n & 0xFFFF_FFFF) < 100_000 {
                        d.hints
                            .push(format!("i32 {}, {}", n & 0xFFFF_FFFF, n >> 32));
                    }
                }
                4 => {
                    let f = f32::from_bits(n as u32) as f64;
                    if f.abs() > 1e-4 && f.abs() < 1e7 {
                        d.hints.push(format!("f32 {}", fmt_float(f)));
                    } else {
                        d.hints.push(format!("int {}", n as u32 as i32));
                    }
                }
                _ => d.hints.push(format!("int {n}")),
            }
        }
        let text = ascii(&b[..size]);
        if text
            .as_bytes()
            .windows(3)
            .any(|w| w.iter().all(u8::is_ascii_alphabetic))
        {
            d.hints.push(format!("'{text}'"));
        }
    }

    fn decode_pointer(&self, d: &mut Decoded, fd: &FieldDefinition, raw: u64) {
        let p = if fd.field_type == FieldType::EncryptedPointer {
            d.hints.push(format!("enc {raw:016X}"));
            match self.source.decrypt(raw) {
                Ok(v) => v,
                Err(e) => {
                    d.value = format!("{raw:016X}");
                    d.error = Some(format!("decrypt failed: {e}"));
                    return;
                }
            }
        } else {
            raw
        };
        d.pointer = Some(p);
        if p == 0 {
            d.value = "nullptr".into();
            return;
        }
        d.value = format!("{p:012X}");
        if !self.readable(p) {
            d.error = Some("invalid pointer".into());
            return;
        }
        if let Some(sym) = self.symbolize(p) {
            d.hints.push(sym);
        }
        match fd.pointer_target.as_ref() {
            Some(PointerTarget::ClassId(cid)) if self.layout.classes.get(*cid).is_some() => {
                d.child = Some(Child::Class {
                    class_id: *cid,
                    base: p,
                    via_pointer: true,
                });
            }
            Some(PointerTarget::Array { element, length }) => {
                d.child = Some(Child::Elements {
                    base: p,
                    element: (**element).clone(),
                    length: *length,
                });
            }
            Some(PointerTarget::FieldType(FieldType::Hex64)) | None => {}
            Some(t) => {
                let inner = self.decode(&element_field(t, 0), p);
                d.hints.insert(0, format!("-> {}", inner.value));
            }
        }
    }

    fn decode_enum(&self, d: &mut Decoded, fd: &FieldDefinition, size: usize) {
        let mut n = 0u64;
        for (i, x) in d.bytes.iter().take(size).enumerate() {
            n |= (*x as u64) << (8 * i);
        }
        let Some(e) = fd.enum_id.and_then(|id| self.layout.enums.get(id)) else {
            d.value = n.to_string();
            return;
        };
        d.value = if e.is_flags {
            let on: Vec<&str> = e
                .variants
                .iter()
                .filter(|v| v.value != 0 && n & v.value as u64 == v.value as u64)
                .map(|v| v.name.as_str())
                .collect();
            if on.is_empty() {
                "0".into()
            } else {
                on.join(" | ")
            }
        } else {
            e.variants
                .iter()
                .find(|v| v.value as u64 == n)
                .map(|v| v.name.clone())
                .unwrap_or_else(|| n.to_string())
        };
        d.hints.push(format!("= {n}"));
    }
}

pub fn target_label(lay: &Layout, t: &PointerTarget) -> String {
    match t {
        PointerTarget::FieldType(FieldType::Hex64) => "void".into(),
        PointerTarget::FieldType(ft) => ft.get_display_name().into(),
        PointerTarget::ClassId(id) => lay
            .classes
            .get(*id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("#{id}")),
        PointerTarget::EnumId(id) => lay
            .enums
            .get(*id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| format!("#{id}")),
        PointerTarget::Array { element, length } => {
            format!("{}[{length}]", target_label(lay, element))
        }
        PointerTarget::Pointer(inner) => format!("{}*", target_label(lay, inner)),
    }
}

pub fn type_label(lay: &Layout, fd: &FieldDefinition) -> String {
    match fd.field_type {
        FieldType::Pointer | FieldType::EncryptedPointer => {
            let inner = fd
                .pointer_target
                .as_ref()
                .map(|t| target_label(lay, t))
                .unwrap_or_else(|| "void".into());
            let enc = if fd.field_type == FieldType::EncryptedPointer {
                "enc "
            } else {
                ""
            };
            format!("{enc}{inner}*")
        }
        FieldType::ClassInstance => fd
            .class_id
            .and_then(|id| lay.classes.get(id))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "class?".into()),
        FieldType::Enum => fd
            .enum_id
            .and_then(|id| lay.enums.get(id))
            .map(|e| e.name.clone())
            .unwrap_or_else(|| "enum?".into()),
        FieldType::Array => format!(
            "{}[{}]",
            fd.array_element
                .as_ref()
                .map(|t| target_label(lay, t))
                .unwrap_or_else(|| "?".into()),
            fd.array_length.unwrap_or(0)
        ),
        ref t => t.get_display_name().into(),
    }
}

/// A synthetic field describing one element of an array (or a pointee).
pub fn element_field(t: &PointerTarget, index: u32) -> FieldDefinition {
    // A literal rather than FieldDefinition::new, which would consume a global field id.
    let mut fd = FieldDefinition {
        id: 0,
        name: Some(format!("[{index}]")),
        field_type: FieldType::Hex8,
        offset: 0,
        class_id: None,
        pointer_target: None,
        enum_id: None,
        enum_size: None,
        array_element: None,
        array_length: None,
    };
    match t {
        PointerTarget::FieldType(ft) => fd.field_type = ft.clone(),
        PointerTarget::ClassId(cid) => {
            fd.field_type = FieldType::ClassInstance;
            fd.class_id = Some(*cid);
        }
        PointerTarget::EnumId(eid) => {
            fd.field_type = FieldType::Enum;
            fd.enum_id = Some(*eid);
        }
        PointerTarget::Array { element, length } => {
            fd.field_type = FieldType::Array;
            fd.array_element = Some((**element).clone());
            fd.array_length = Some(*length);
        }
        PointerTarget::Pointer(inner) => {
            fd.field_type = FieldType::Pointer;
            fd.pointer_target = Some((**inner).clone());
        }
    }
    fd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        demo::DemoSource,
        memory::{
            ClassDefinition,
            MemoryStructure,
        },
    };

    #[test]
    fn vector2_decodes_both_components_and_preserves_following_offset() {
        let base = 0x1F3_0000_1000;
        let source = DemoSource::new();
        source.poke(
            base,
            &[
                1.0f32.to_le_bytes(),
                (-2.5f32).to_le_bytes(),
                42u32.to_le_bytes(),
            ]
            .concat(),
        );
        let mut class = ClassDefinition::new("Vector2Fields".into());
        class.add_named_field("position".into(), FieldType::Vector2);
        class.add_named_field("following".into(), FieldType::UInt32);
        let ms = MemoryStructure::new("root".into(), base, class);
        let layout = Layout::of(&ms);
        let decoder = Decoder::new(&source, layout);
        let def = ms.class_registry.get(ms.root_class.class_id).unwrap();
        let offsets = layout.field_offsets(def.id);
        let decoded = decoder.decode(&def.fields[0], base + offsets[0].0);
        assert_eq!(decoded.value, "(1.0, -2.5)");
        assert_eq!(decoded.error, None);
        assert_eq!(decoded.bytes.len(), 8);
        assert_eq!(offsets, vec![(0, 8), (8, 4)]);
        assert_eq!(def.fields[1].offset, 8);
        assert_eq!(ms.root_class.fields[1].address, base + 8);
        assert_eq!(
            decoder.decode(&def.fields[1], base + offsets[1].0).value,
            "42"
        );
    }

    #[test]
    fn vector2_array_uses_eight_byte_stride() {
        let base = 0x1F3_0000_1000;
        let source = DemoSource::new();
        source.poke(
            base,
            &[
                1.0f32.to_le_bytes(),
                2.0f32.to_le_bytes(),
                3.0f32.to_le_bytes(),
                4.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        let element = PointerTarget::FieldType(FieldType::Vector2);
        let mut class = ClassDefinition::new("Vector2Array".into());
        class.add_named_field("positions".into(), FieldType::Array);
        class.fields[0].array_element = Some(element.clone());
        class.fields[0].array_length = Some(2);
        class.add_named_field("following".into(), FieldType::UInt32);
        let ms = MemoryStructure::new("root".into(), base, class);
        let layout = Layout::of(&ms);
        let decoder = Decoder::new(&source, layout);
        let stride = layout.target_size(&element);
        assert_eq!(stride, 8);
        assert_eq!(
            layout.field_offsets(ms.root_class.class_id),
            vec![(0, 16), (16, 4)]
        );
        for (index, expected) in ["(1.0, 2.0)", "(3.0, 4.0)"].iter().enumerate() {
            let decoded = decoder.decode(
                &element_field(&element, index as u32),
                base + index as u64 * stride,
            );
            assert_eq!(&decoded.value, expected);
            assert_eq!(decoded.error, None);
        }
    }

    #[test]
    fn qwords_holding_two_floats_are_hinted() {
        assert_eq!(float_pair(0x4280_0000_42F0_0000), Some((120.0, 64.0)));
        assert_eq!(float_pair(0x0000_0000_4148_0000), Some((12.5, 0.0)));
        for not_floats in [
            0x0000_7FF6_A2B9_1A08,
            0x0000_0006_0000_000E,
            0x0000_0000_0000_0064,
        ] {
            assert_eq!(float_pair(not_floats), None, "{not_floats:X}");
        }
    }

    #[test]
    fn floats_are_short_and_stable() {
        assert_eq!(fmt_float(1.0), "1.0");
        assert_eq!(fmt_float(-9.81), "-9.81");
        assert_eq!(fmt_float(0.0), "0.0");
        assert_eq!(fmt_float(-0.0001), "0.0");
        assert_eq!(fmt_float(5000.0), "5000.0");
        assert_eq!(fmt_float(2.5e-14), "2.500e-14");
    }
}
