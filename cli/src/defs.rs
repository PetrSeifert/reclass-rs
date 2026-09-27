//! Resolves names the user types (classes, fields, enums, types) against the
//! server's definitions, which it reports as the `defs` message.

use anyhow::{
    anyhow,
    bail,
    Result,
};
use serde_json::{
    json,
    Value,
};

pub struct Defs {
    pub classes: Vec<Value>,
    pub enums: Vec<Value>,
}

/// A field of a class, as addressed by `Class.name` or `Class+0x10`.
pub struct FieldRef<'a> {
    pub class: &'a Value,
    pub field: &'a Value,
}

impl FieldRef<'_> {
    pub fn params(&self) -> Value {
        json!({ "classId": self.class["id"], "fieldId": self.field["id"] })
    }
}

/// Primitive types by the names users may type. `void` is only valid as a pointee.
const PRIMITIVES: &[(&str, &str)] = &[
    ("hex64", "Hex64"),
    ("hex32", "Hex32"),
    ("hex16", "Hex16"),
    ("hex8", "Hex8"),
    ("int64", "Int64"),
    ("i64", "Int64"),
    ("int32", "Int32"),
    ("i32", "Int32"),
    ("int", "Int32"),
    ("int16", "Int16"),
    ("i16", "Int16"),
    ("int8", "Int8"),
    ("i8", "Int8"),
    ("uint64", "UInt64"),
    ("u64", "UInt64"),
    ("uint32", "UInt32"),
    ("u32", "UInt32"),
    ("uint16", "UInt16"),
    ("u16", "UInt16"),
    ("uint8", "UInt8"),
    ("u8", "UInt8"),
    ("byte", "UInt8"),
    ("bool", "Bool"),
    ("float", "Float"),
    ("f32", "Float"),
    ("double", "Double"),
    ("f64", "Double"),
    ("vector2", "Vector2"),
    ("vec2", "Vector2"),
    ("vector3", "Vector3"),
    ("vec3", "Vector3"),
    ("vector4", "Vector4"),
    ("vec4", "Vector4"),
    ("text", "Text"),
    ("textpointer", "TextPointer"),
    ("textptr", "TextPointer"),
    ("char*", "TextPointer"),
    ("pointer", "Pointer"),
    ("ptr", "Pointer"),
    ("void*", "Pointer"),
];

fn primitive(s: &str) -> Option<&'static str> {
    let s = s.to_ascii_lowercase();
    PRIMITIVES.iter().find(|(k, _)| *k == s).map(|(_, v)| *v)
}

fn fields(class: &Value) -> &[Value] {
    class["fields"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// Parses `0x`-prefixed hex or decimal.
pub fn number(s: &str) -> Result<u64> {
    let s = s.trim();
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    parsed.map_err(|_| anyhow!("'{s}' is not a number (use 0x for hex)"))
}

/// Splits `T[n]` into `T` and `n`.
fn array_suffix(s: &str) -> Result<Option<(&str, u32)>> {
    let Some(inner) = s.strip_suffix(']') else {
        return Ok(None);
    };
    let open = inner
        .rfind('[')
        .ok_or_else(|| anyhow!("unbalanced ']' in '{s}'"))?;
    let n = number(&inner[open + 1..])?;
    let n = u32::try_from(n).map_err(|_| anyhow!("array length {n} is too large"))?;
    Ok(Some((inner[..open].trim(), n)))
}

impl Defs {
    pub fn from_state(state: &Value) -> Self {
        let list = |k: &str| state["defs"][k].as_array().cloned().unwrap_or_default();
        Self {
            classes: list("classes"),
            enums: list("enums"),
        }
    }

    fn find<'a>(list: &'a [Value], s: &str) -> Option<&'a Value> {
        if let Some(id) = s.strip_prefix('#') {
            let id = number(id).ok()?;
            return list.iter().find(|v| v["id"] == id);
        }
        list.iter().find(|v| v["name"] == s).or_else(|| {
            let mut ci = list.iter().filter(|v| {
                v["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(s))
            });
            match (ci.next(), ci.next()) {
                (Some(v), None) => Some(v),
                _ => None,
            }
        })
    }

    pub fn class(&self, s: &str) -> Result<&Value> {
        Self::find(&self.classes, s.trim())
            .ok_or_else(|| anyhow!("no class '{s}' (list them with `reclass classes`)"))
    }

    pub fn enumeration(&self, s: &str) -> Result<&Value> {
        Self::find(&self.enums, s.trim())
            .ok_or_else(|| anyhow!("no enum '{s}' (list them with `reclass enums`)"))
    }

    /// `Class.name` or `Class+offset`, where the offset must start a field.
    pub fn field(&self, s: &str) -> Result<FieldRef<'_>> {
        let span = |f: &Value| {
            (
                f["offset"].as_u64().unwrap_or(0),
                f["size"].as_u64().unwrap_or(0),
            )
        };
        if let Some((class, offset)) = s.split_once('+') {
            let class = self.class(class)?;
            let offset = number(offset)?;
            if let Some(field) = fields(class).iter().find(|f| span(f).0 == offset) {
                return Ok(FieldRef { class, field });
            }
            match fields(class).iter().map(span).find(|(o, n)| offset > *o && offset < o + n) {
                Some((start, _)) => bail!(
                    "0x{offset:X} is inside the field at 0x{start:X}; `reclass define` splits hex fields"
                ),
                None => bail!("0x{offset:X} is outside {}", class["name"].as_str().unwrap_or("?")),
            }
        }
        let (class, name) = s
            .split_once('.')
            .ok_or_else(|| anyhow!("'{s}' is not a field; write Class.field or Class+0x10"))?;
        let class = self.class(class)?;
        let field = fields(class)
            .iter()
            .find(|f| f["name"] == name)
            .ok_or_else(|| {
                anyhow!(
                    "{} has no field '{name}' (hex fields have no name; use Class+0x10)",
                    class["name"].as_str().unwrap_or("?")
                )
            })?;
        Ok(FieldRef { class, field })
    }

    /// A pointee, array element or embedded class, as the server's `PointerTarget`.
    pub fn target(&self, s: &str) -> Result<Value> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("void") {
            return Ok(json!({ "FieldType": "Hex64" }));
        }
        if let Some(p) = primitive(s) {
            return Ok(json!({ "FieldType": p }));
        }
        if let Some(inner) = s.strip_suffix('*') {
            return Ok(json!({ "Pointer": self.target(inner)? }));
        }
        if let Some((inner, length)) = array_suffix(s)? {
            return Ok(json!({ "Array": { "element": self.target(inner)?, "length": length } }));
        }
        self.named_target(s)
    }

    fn named_target(&self, s: &str) -> Result<Value> {
        let class = Self::find(&self.classes, s);
        let enumeration = Self::find(&self.enums, s);
        match (class, enumeration) {
            (Some(c), _) => Ok(json!({ "ClassId": c["id"] })),
            (None, Some(e)) => Ok(json!({ "EnumId": e["id"] })),
            (None, None) => {
                bail!("unknown type '{s}': not a primitive, class or enum (see `reclass --help`)")
            }
        }
    }

    /// A field type, as the `{ ty, target, length }` params of `retype` / `defineAt`.
    pub fn type_spec(&self, s: &str) -> Result<Value> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        if let Some(rest) = lower
            .strip_prefix("enc:")
            .or_else(|| lower.strip_prefix("enc "))
        {
            let rest = &s[s.len() - rest.len()..];
            let mut spec = self.type_spec(rest)?;
            if spec["ty"] != "Pointer" {
                bail!("only pointers can be encrypted: '{s}'");
            }
            spec["ty"] = json!("EncryptedPointer");
            return Ok(spec);
        }
        if let Some(p) = primitive(s) {
            return Ok(json!({ "ty": p }));
        }
        if let Some(inner) = s.strip_suffix('*') {
            return Ok(json!({ "ty": "Pointer", "target": self.target(inner)? }));
        }
        if let Some((inner, length)) = array_suffix(s)? {
            return Ok(json!({ "ty": "Array", "target": self.target(inner)?, "length": length }));
        }
        let target = self.named_target(s)?;
        let ty = if target.get("ClassId").is_some() {
            "ClassInstance"
        } else {
            "Enum"
        };
        Ok(json!({ "ty": ty, "target": target }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> Defs {
        Defs::from_state(&json!({ "defs": {
            "classes": [
                { "id": 3, "name": "Player", "fields": [
                    { "id": 30, "name": null, "offset": 0, "size": 8 },
                    { "id": 31, "name": "health", "offset": 8, "size": 4 },
                ] },
                { "id": 4, "name": "Weapon", "fields": [] },
            ],
            "enums": [{ "id": 7, "name": "ETeam" }],
        } }))
    }

    #[test]
    fn type_specs() {
        let d = defs();
        let cases = [
            ("f32", json!({ "ty": "Float" })),
            ("Vec3", json!({ "ty": "Vector3" })),
            ("ptr", json!({ "ty": "Pointer" })),
            ("char*", json!({ "ty": "TextPointer" })),
            (
                "Player",
                json!({ "ty": "ClassInstance", "target": { "ClassId": 3 } }),
            ),
            (
                "player",
                json!({ "ty": "ClassInstance", "target": { "ClassId": 3 } }),
            ),
            ("ETeam", json!({ "ty": "Enum", "target": { "EnumId": 7 } })),
            (
                "Player*",
                json!({ "ty": "Pointer", "target": { "ClassId": 3 } }),
            ),
            ("void*", json!({ "ty": "Pointer" })),
            (
                "u8*",
                json!({ "ty": "Pointer", "target": { "FieldType": "UInt8" } }),
            ),
            (
                "enc Weapon*",
                json!({ "ty": "EncryptedPointer", "target": { "ClassId": 4 } }),
            ),
            ("enc:void*", json!({ "ty": "EncryptedPointer" })),
            (
                "float[0x10]",
                json!({ "ty": "Array", "target": { "FieldType": "Float" }, "length": 16 }),
            ),
            (
                "Player*[8]",
                json!({ "ty": "Array", "target": { "Pointer": { "ClassId": 3 } }, "length": 8 }),
            ),
            (
                "f32[4]*",
                json!({ "ty": "Pointer", "target": { "Array": { "element": { "FieldType": "Float" }, "length": 4 } } }),
            ),
            (
                "#4",
                json!({ "ty": "ClassInstance", "target": { "ClassId": 4 } }),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(d.type_spec(input).unwrap(), expected, "{input}");
        }
        for bad in ["Nope", "enc f32", "f32[", "f32[x]"] {
            assert!(d.type_spec(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn field_refs() {
        let d = defs();
        assert_eq!(d.field("Player.health").unwrap().field["id"], 31);
        assert_eq!(d.field("Player+0x8").unwrap().field["id"], 31);
        assert_eq!(d.field("Player+0").unwrap().field["id"], 30);
        assert!(d
            .field("Player+4")
            .err()
            .unwrap()
            .to_string()
            .contains("inside"));
        assert!(d.field("Player+0x40").is_err());
        assert!(d.field("Player.armor").is_err());
        assert!(d.field("Player").is_err());
    }
}
