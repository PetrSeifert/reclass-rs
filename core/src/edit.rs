//! Structural edits on class definitions, addressed by stable ids.
//!
//! Unlike `ClassDefinition::set_field_type_at`, `retype_field` keeps the rest of
//! the class where it is: a smaller type is padded with hex fields and a larger
//! one consumes (and, if needed, splits) the fields after it.

use crate::{
    layout::{
        hex_fill,
        Layout,
    },
    memory::{
        ClassDefinition,
        EnumDefinition,
        EnumVariant,
        FieldType,
        MemoryStructure,
        PointerTarget,
    },
};

pub type EditResult<T = ()> = Result<T, String>;

fn field_index(ms: &MemoryStructure, class_id: u64, field_id: u64) -> EditResult<usize> {
    let def = ms
        .class_registry
        .get(class_id)
        .ok_or_else(|| format!("no class #{class_id}"))?;
    def.fields
        .iter()
        .position(|f| f.id == field_id)
        .ok_or_else(|| format!("no field #{field_id} in {}", def.name))
}

fn class_mut(ms: &mut MemoryStructure, class_id: u64) -> EditResult<&mut ClassDefinition> {
    ms.class_registry
        .get_mut(class_id)
        .ok_or_else(|| format!("no class #{class_id}"))
}

fn insert_hex(def: &mut ClassDefinition, at: usize, bytes: u64) {
    for (i, t) in hex_fill(bytes).into_iter().enumerate() {
        def.insert_hex_field_at(at + i, t);
    }
}

pub fn retype_field(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: u64,
    ty: FieldType,
) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    let sizes = field_sizes(ms, class_id);
    let offset = Layout::of(ms).field_offsets(class_id)[idx].0;
    // Prefer an enum that fits the field's current size, so the layout doesn't shift.
    let first_enum = {
        let mut ids = ms.enum_registry.get_enum_ids();
        ids.sort_unstable();
        let fits = ids.iter().copied().find(|id| {
            ms.enum_registry
                .get(*id)
                .is_some_and(|e| e.default_size as u64 == sizes[idx])
        });
        fits.or(ids.first().copied())
    };
    {
        let def = class_mut(ms, class_id)?;
        let was_unnamed = def.fields[idx].name.is_none();
        def.set_field_type_at(idx, ty.clone());
        let fd = &mut def.fields[idx];
        if was_unnamed && !ty.is_hex_type() {
            // Name by offset rather than index, so names stay put when fields are inserted.
            fd.name = Some(format!("field_{offset:02X}"));
        }
        match ty {
            FieldType::Pointer | FieldType::EncryptedPointer if fd.pointer_target.is_none() => {
                fd.pointer_target = Some(PointerTarget::FieldType(FieldType::Hex64));
            }
            FieldType::Enum => {
                fd.enum_id = fd.enum_id.or(first_enum);
                fd.enum_size = None;
            }
            _ => {}
        }
    }
    keep_layout(ms, class_id, idx, &sizes)
}

fn field_sizes(ms: &MemoryStructure, class_id: u64) -> Vec<u64> {
    let lay = Layout::of(ms);
    ms.class_registry
        .get(class_id)
        .map(|c| c.fields.iter().map(|f| lay.field_size(f)).collect())
        .unwrap_or_default()
}

/// After field `idx` changed size, pads it with hex fields or consumes (and
/// splits) the fields after it so everything else keeps its offset. `sizes`
/// are the field sizes from before the change.
fn keep_layout(ms: &mut MemoryStructure, class_id: u64, idx: usize, sizes: &[u64]) -> EditResult {
    let new_size = Layout::of(ms).field_size(&ms.class_registry.get(class_id).unwrap().fields[idx]);
    let old_size = sizes[idx];
    let def = class_mut(ms, class_id)?;
    if new_size < old_size {
        insert_hex(def, idx + 1, old_size - new_size);
    } else if new_size > old_size {
        let mut need = new_size - old_size;
        let mut next = idx + 1;
        while need > 0 && idx + 1 < def.fields.len() {
            let size = sizes[next];
            def.remove_field_at(idx + 1);
            next += 1;
            if size > need {
                insert_hex(def, idx + 1, size - need);
                need = 0;
            } else {
                need -= size;
            }
        }
    }
    Ok(())
}

/// Retypes whatever starts at `offset`, first splitting a hex field that covers it.
pub fn define_at(
    ms: &mut MemoryStructure,
    class_id: u64,
    offset: u64,
    ty: FieldType,
) -> EditResult<u64> {
    let offsets = Layout::of(ms).field_offsets(class_id);
    let idx = offsets
        .iter()
        .position(|(o, s)| offset >= *o && offset < o + s)
        .ok_or_else(|| format!("offset 0x{offset:X} is outside the class"))?;
    let (start, size) = offsets[idx];
    if start != offset {
        let def = class_mut(ms, class_id)?;
        if !def.fields[idx].field_type.is_hex_type() {
            return Err(format!(
                "0x{offset:X} is inside the {} field",
                def.fields[idx].field_type
            ));
        }
        def.remove_field_at(idx);
        insert_hex(def, idx, size - (offset - start));
        insert_hex(def, idx, offset - start);
    }
    let offsets = Layout::of(ms).field_offsets(class_id);
    let idx = offsets.iter().position(|(o, _)| *o == offset).unwrap();
    let field_id = ms.class_registry.get(class_id).unwrap().fields[idx].id;
    retype_field(ms, class_id, field_id, ty)?;
    Ok(field_id)
}

pub fn rename_field(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: u64,
    name: &str,
) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    let fd = &mut class_mut(ms, class_id)?.fields[idx];
    let name = name.trim();
    if fd.field_type.is_hex_type() {
        return Err("hex fields have no name; change the type first".into());
    }
    if name.is_empty() {
        return Err("name cannot be empty".into());
    }
    fd.name = Some(name.to_string());
    Ok(())
}

/// Inserts `count` bytes of hex fields before or after `field_id`, or at the end when `None`.
pub fn insert_bytes(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: Option<u64>,
    count: u64,
    after: bool,
) -> EditResult {
    let at = match field_id {
        Some(fid) => field_index(ms, class_id, fid)? + after as usize,
        None => ms
            .class_registry
            .get(class_id)
            .map(|d| d.fields.len())
            .unwrap_or(0),
    };
    insert_hex(class_mut(ms, class_id)?, at, count);
    Ok(())
}

pub fn remove_field(ms: &mut MemoryStructure, class_id: u64, field_id: u64) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    let def = class_mut(ms, class_id)?;
    if def.fields.len() == 1 {
        return Err("a class needs at least one field".into());
    }
    def.remove_field_at(idx);
    Ok(())
}

fn target_embeds(target: &PointerTarget) -> Option<u64> {
    match target {
        PointerTarget::ClassId(cid) => Some(*cid),
        PointerTarget::Array { element, .. } => target_embeds(element),
        _ => None,
    }
}

/// Pointee of a `Pointer` / `EncryptedPointer` field.
pub fn set_pointer_target(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: u64,
    target: PointerTarget,
) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    let fd = &mut class_mut(ms, class_id)?.fields[idx];
    if !matches!(
        fd.field_type,
        FieldType::Pointer | FieldType::EncryptedPointer
    ) {
        return Err(format!("{} is not a pointer", fd.field_type));
    }
    fd.pointer_target = Some(target);
    Ok(())
}

/// Class embedded by a `ClassInstance` field.
pub fn set_embedded_class(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: u64,
    target: u64,
) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    if ms.class_registry.get(target).is_none() {
        return Err(format!("no class #{target}"));
    }
    if ms.would_create_cycle(class_id, target) {
        return Err("embedding that class would create a cycle".into());
    }
    let fd = &mut class_mut(ms, class_id)?.fields[idx];
    if fd.field_type != FieldType::ClassInstance {
        return Err(format!("{} is not a class instance", fd.field_type));
    }
    fd.class_id = Some(target);
    Ok(())
}

pub fn set_enum(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: u64,
    enum_id: u64,
) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    if !ms.enum_registry.contains(enum_id) {
        return Err(format!("no enum #{enum_id}"));
    }
    let sizes = field_sizes(ms, class_id);
    let fd = &mut class_mut(ms, class_id)?.fields[idx];
    if fd.field_type != FieldType::Enum {
        return Err(format!("{} is not an enum", fd.field_type));
    }
    fd.enum_id = Some(enum_id);
    fd.enum_size = None;
    keep_layout(ms, class_id, idx, &sizes)
}

pub fn set_array(
    ms: &mut MemoryStructure,
    class_id: u64,
    field_id: u64,
    element: Option<PointerTarget>,
    length: Option<u32>,
) -> EditResult {
    let idx = field_index(ms, class_id, field_id)?;
    if let Some(cid) = element.as_ref().and_then(target_embeds) {
        if ms.would_create_cycle(class_id, cid) {
            return Err("an inline array of that class would create a cycle".into());
        }
    }
    let fd = &mut class_mut(ms, class_id)?.fields[idx];
    if fd.field_type != FieldType::Array {
        return Err(format!("{} is not an array", fd.field_type));
    }
    if let Some(e) = element {
        fd.array_element = Some(e);
    }
    if let Some(n) = length {
        fd.array_length = Some(n.clamp(1, 4096));
    }
    Ok(())
}

fn unique_class_name(ms: &MemoryStructure, base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 1;
    while ms.class_registry.contains_name(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

/// New class with 0x40 bytes of hex fields.
pub fn add_class(ms: &mut MemoryStructure, name: Option<&str>) -> EditResult<u64> {
    let name = unique_class_name(
        ms,
        name.map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("NewClass"),
    );
    let mut def = ClassDefinition::new(name);
    for t in hex_fill(0x40) {
        def.add_hex_field(t);
    }
    let id = def.id;
    ms.class_registry.register(def);
    Ok(id)
}

pub fn rename_class(ms: &mut MemoryStructure, class_id: u64, name: &str) -> EditResult {
    let name = name.trim();
    if ms
        .class_registry
        .get(class_id)
        .is_some_and(|c| c.name == name)
    {
        return Ok(());
    }
    if ms.class_registry.contains_name(name) {
        return Err(format!("a class named {name} already exists"));
    }
    if ms.rename_class(class_id, name) {
        Ok(())
    } else {
        Err("rename failed".into())
    }
}

/// Number of fields that use the enum (directly, or as a pointer/array target).
pub fn enum_refs(ms: &MemoryStructure, enum_id: u64) -> usize {
    fn hits(t: &PointerTarget, eid: u64) -> bool {
        match t {
            PointerTarget::EnumId(e) => *e == eid,
            PointerTarget::Array { element, .. } | PointerTarget::Pointer(element) => {
                hits(element, eid)
            }
            _ => false,
        }
    }
    let mut n = 0;
    for cid in ms.class_registry.get_class_ids() {
        for f in &ms.class_registry.get(cid).unwrap().fields {
            if (f.field_type == FieldType::Enum && f.enum_id == Some(enum_id))
                || f.pointer_target.as_ref().is_some_and(|t| hits(t, enum_id))
                || f.array_element.as_ref().is_some_and(|t| hits(t, enum_id))
            {
                n += 1;
            }
        }
    }
    n
}

/// New empty enum (4 bytes unless `size` says otherwise) with a unique name.
pub fn add_enum(ms: &mut MemoryStructure, name: Option<&str>, size: Option<u8>) -> EditResult<u64> {
    let size = size.unwrap_or(4);
    if ![1, 2, 4, 8].contains(&size) {
        return Err("size must be 1, 2, 4 or 8 bytes".into());
    }
    let base = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("NewEnum");
    let mut name = base.to_string();
    let mut n = 1;
    while ms.enum_registry.contains_name(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    let mut def = EnumDefinition::new(name);
    def.default_size = size;
    let id = def.id;
    ms.enum_registry.register(def);
    Ok(id)
}

/// Replaces an enum's name, kind, size and variants.
pub fn update_enum(
    ms: &mut MemoryStructure,
    enum_id: u64,
    name: &str,
    is_flags: bool,
    size: u8,
    variants: Vec<(String, u32)>,
) -> EditResult {
    let name = name.trim();
    if name.is_empty() {
        return Err("enum needs a name".into());
    }
    if ![1, 2, 4, 8].contains(&size) {
        return Err("size must be 1, 2, 4 or 8 bytes".into());
    }
    let taken = ms
        .enum_registry
        .get_enum_ids()
        .into_iter()
        .any(|id| id != enum_id && ms.enum_registry.get(id).is_some_and(|e| e.name == name));
    if taken {
        return Err(format!("an enum named {name} already exists"));
    }
    let mut seen = std::collections::HashSet::new();
    for (v, _) in &variants {
        if v.trim().is_empty() {
            return Err("variant names cannot be empty".into());
        }
        if !seen.insert(v.trim()) {
            return Err(format!("duplicate variant {}", v.trim()));
        }
    }
    let def = ms
        .enum_registry
        .get_mut(enum_id)
        .ok_or_else(|| format!("no enum #{enum_id}"))?;
    def.name = name.to_string();
    def.is_flags = is_flags;
    def.default_size = size;
    def.variants = variants
        .into_iter()
        .map(|(n, value)| EnumVariant {
            name: n.trim().to_string(),
            value,
        })
        .collect();
    Ok(())
}

pub fn delete_enum(ms: &mut MemoryStructure, enum_id: u64) -> EditResult {
    if !ms.enum_registry.contains(enum_id) {
        return Err(format!("no enum #{enum_id}"));
    }
    if enum_refs(ms, enum_id) > 0 {
        return Err("enum is still used by a field".into());
    }
    ms.enum_registry.remove(enum_id);
    Ok(())
}

/// Number of fields (plus the root) that refer to the class.
pub fn class_refs(ms: &MemoryStructure, class_id: u64) -> usize {
    fn hits(t: &PointerTarget, cid: u64) -> bool {
        match t {
            PointerTarget::ClassId(c) => *c == cid,
            PointerTarget::Array { element, .. } | PointerTarget::Pointer(element) => {
                hits(element, cid)
            }
            _ => false,
        }
    }
    let mut n = (ms.root_class.class_id == class_id) as usize;
    for cid in ms.class_registry.get_class_ids() {
        for f in &ms.class_registry.get(cid).unwrap().fields {
            if f.class_id == Some(class_id)
                || f.pointer_target.as_ref().is_some_and(|t| hits(t, class_id))
                || f.array_element.as_ref().is_some_and(|t| hits(t, class_id))
            {
                n += 1;
            }
        }
    }
    n
}

pub fn delete_class(ms: &mut MemoryStructure, class_id: u64) -> EditResult {
    if ms.class_registry.get(class_id).is_none() {
        return Err(format!("no class #{class_id}"));
    }
    if class_refs(ms, class_id) > 0 {
        return Err("class is still referenced".into());
    }
    ms.class_registry.remove(class_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms_with(types: &[FieldType]) -> (MemoryStructure, u64) {
        let mut def = ClassDefinition::new("T".into());
        for t in types {
            def.add_hex_field(t.clone());
        }
        let id = def.id;
        (MemoryStructure::new("root".into(), 0, def), id)
    }

    fn types(ms: &MemoryStructure, cid: u64) -> Vec<FieldType> {
        ms.class_registry
            .get(cid)
            .unwrap()
            .fields
            .iter()
            .map(|f| f.field_type.clone())
            .collect()
    }

    fn fid(ms: &MemoryStructure, cid: u64, idx: usize) -> u64 {
        ms.class_registry.get(cid).unwrap().fields[idx].id
    }

    #[test]
    fn retype_smaller_pads_with_hex() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex64, FieldType::Hex64]);
        let f0 = fid(&ms, cid, 0);

        retype_field(&mut ms, cid, f0, FieldType::Float).unwrap();
        assert_eq!(
            types(&ms, cid),
            vec![FieldType::Float, FieldType::Hex32, FieldType::Hex64]
        );
        assert_eq!(Layout::of(&ms).class_size(cid), 16);
    }

    #[test]
    fn retype_larger_consumes_and_splits_following_fields() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex32, FieldType::Hex64, FieldType::Hex64]);
        let f0 = fid(&ms, cid, 0);

        retype_field(&mut ms, cid, f0, FieldType::Vector3).unwrap();
        assert_eq!(types(&ms, cid), vec![FieldType::Vector3, FieldType::Hex64]);
        let (mut ms, cid) = ms_with(&[FieldType::Hex32, FieldType::Hex64]);
        let f0 = fid(&ms, cid, 0);

        retype_field(&mut ms, cid, f0, FieldType::Hex64).unwrap();
        assert_eq!(types(&ms, cid), vec![FieldType::Hex64, FieldType::Hex32]);
    }

    #[test]
    fn retype_pointer_gets_default_target() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex64]);
        let f0 = fid(&ms, cid, 0);

        retype_field(&mut ms, cid, f0, FieldType::Pointer).unwrap();
        let fd = &ms.class_registry.get(cid).unwrap().fields[0];
        assert_eq!(
            fd.pointer_target,
            Some(PointerTarget::FieldType(FieldType::Hex64))
        );
        assert_eq!(fd.name.as_deref(), Some("field_00"));
    }

    #[test]
    fn define_at_splits_covering_hex_field() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex64, FieldType::Hex64]);
        define_at(&mut ms, cid, 4, FieldType::Float).unwrap();
        assert_eq!(
            types(&ms, cid),
            vec![FieldType::Hex32, FieldType::Float, FieldType::Hex64]
        );
        assert!(define_at(&mut ms, cid, 6, FieldType::Int16).is_err());
    }

    #[test]
    fn insert_remove_and_class_management() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex64]);
        let f0 = fid(&ms, cid, 0);

        insert_bytes(&mut ms, cid, Some(f0), 12, false).unwrap();
        assert_eq!(
            types(&ms, cid),
            vec![FieldType::Hex64, FieldType::Hex32, FieldType::Hex64]
        );
        let f0 = fid(&ms, cid, 0);

        remove_field(&mut ms, cid, f0).unwrap();
        assert_eq!(Layout::of(&ms).class_size(cid), 12);

        let other = add_class(&mut ms, Some("T")).unwrap();
        assert_eq!(ms.class_registry.get(other).unwrap().name, "T_1");
        assert_eq!(class_refs(&ms, other), 0);
        let f0 = fid(&ms, cid, 0);

        retype_field(&mut ms, cid, f0, FieldType::Pointer).unwrap();
        let f0 = fid(&ms, cid, 0);

        set_pointer_target(&mut ms, cid, f0, PointerTarget::ClassId(other)).unwrap();
        assert_eq!(class_refs(&ms, other), 1);
        assert!(delete_class(&mut ms, other).is_err());
        assert!(rename_class(&mut ms, other, "T").is_err());
        rename_class(&mut ms, other, "Other").unwrap();
    }

    #[test]
    fn embedding_self_is_rejected() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex64]);
        let f = fid(&ms, cid, 0);
        retype_field(&mut ms, cid, f, FieldType::ClassInstance).unwrap();
        assert!(set_embedded_class(&mut ms, cid, f, cid).is_err());
    }

    #[test]
    fn enum_lifecycle() {
        let (mut ms, cid) = ms_with(&[FieldType::Hex32]);
        let e = add_enum(&mut ms, Some("ETeam"), None).unwrap();
        assert_eq!(
            add_enum(&mut ms, Some("ETeam"), None).map(|id| ms
                .enum_registry
                .get(id)
                .unwrap()
                .name
                .clone()),
            Ok("ETeam_1".into())
        );
        update_enum(
            &mut ms,
            e,
            "ETeam",
            false,
            2,
            vec![("Red".into(), 1), ("Blue".into(), 2)],
        )
        .unwrap();
        assert!(update_enum(&mut ms, e, "ETeam_1", false, 4, vec![]).is_err());
        assert!(add_enum(&mut ms, None, Some(3)).is_err());
        let small = add_enum(&mut ms, None, Some(1)).unwrap();
        assert_eq!(ms.enum_registry.get(small).unwrap().default_size, 1);
        let (mut ms2, c2) = ms_with(&[FieldType::Hex8, FieldType::Hex8]);
        add_enum(&mut ms2, Some("Wide"), None).unwrap();
        let narrow = add_enum(&mut ms2, Some("Narrow"), Some(1)).unwrap();
        let f = fid(&ms2, c2, 0);
        retype_field(&mut ms2, c2, f, FieldType::Enum).unwrap();
        assert_eq!(
            ms2.class_registry.get(c2).unwrap().fields[0].enum_id,
            Some(narrow)
        );
        assert_eq!(types(&ms2, c2), vec![FieldType::Enum, FieldType::Hex8]);
        assert!(update_enum(&mut ms, e, "ETeam", false, 3, vec![]).is_err());
        assert!(update_enum(
            &mut ms,
            e,
            "ETeam",
            false,
            4,
            vec![("A".into(), 1), ("A".into(), 2)]
        )
        .is_err());
        let f0 = fid(&ms, cid, 0);
        retype_field(&mut ms, cid, f0, FieldType::Enum).unwrap();
        set_enum(&mut ms, cid, f0, e).unwrap();
        // 2-byte enum in a 4-byte slot: the rest is padded
        assert_eq!(types(&ms, cid), vec![FieldType::Enum, FieldType::Hex16]);
        let wide = add_enum(&mut ms, Some("EWide"), None).unwrap();
        set_enum(&mut ms, cid, f0, wide).unwrap();
        assert_eq!(types(&ms, cid), vec![FieldType::Enum]);
        set_enum(&mut ms, cid, f0, e).unwrap();
        assert_eq!(types(&ms, cid), vec![FieldType::Enum, FieldType::Hex16]);
        assert!(delete_enum(&mut ms, e).is_err());
        retype_field(&mut ms, cid, f0, FieldType::Hex16).unwrap();
        delete_enum(&mut ms, e).unwrap();
    }
}
