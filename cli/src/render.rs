//! Plain-text views of server replies, compact enough to read in a terminal
//! or an agent's context. `--json` bypasses all of this.

use serde_json::Value;

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn u64_of(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

/// Left-aligns every column but the last.
pub fn table(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            rows.iter()
                .filter(|r| c + 1 < r.len())
                .map(|r| r[c].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (c, cell) in row.iter().enumerate() {
            if c + 1 < row.len() {
                line.push_str(&format!("{cell:<w$}  ", w = widths[c]));
            } else {
                line.push_str(cell);
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// What an uninteresting hex row shows, so runs of the same can collapse.
fn filler(row: &Value) -> Option<&'static str> {
    if !str_of(&row["type"]).starts_with("Hex") || !row["name"].is_null() {
        return None;
    }
    let value = str_of(&row["value"]);
    if row.get("error").is_some() {
        Some("unreadable")
    } else if row.get("hints").is_none() && value.bytes().all(|b| b == b'0') {
        Some("0")
    } else {
        None
    }
}

/// Pointer values get a `0x` prefix so they can be pasted back into an
/// expression, where bare digits are decimal.
fn value_of(row: &Value) -> String {
    let value = str_of(&row["value"]);
    let ty = str_of(&row["type"]);
    let points = ty.ends_with('*')
        || (ty == "Hex64"
            && row["hints"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|h| str_of(h).starts_with("->")));
    if points && !value.is_empty() && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        format!("0x{value}")
    } else {
        value.to_string()
    }
}

/// Appends rows as `[offset, type, name, value+hints]` cells, nesting children.
/// Runs of zeroed or unreadable hex fields collapse into one line.
fn instance_rows(rows: &[Value], indent: usize, out: &mut Vec<Vec<String>>) {
    let pad = " ".repeat(indent * 2);
    let mut i = 0;
    while i < rows.len() {
        let row = &rows[i];
        let kind = filler(row);
        let run = rows[i..]
            .iter()
            .take_while(|r| kind.is_some() && filler(r) == kind)
            .count();
        if let (true, Some(kind)) = (run > 1, kind) {
            let bytes: u64 = rows[i..i + run].iter().map(|r| u64_of(&r["size"])).sum();
            out.push(vec![
                format!("{pad}{:04X}", u64_of(&row["offset"])),
                "Hex".into(),
                String::new(),
                format!("{kind} × 0x{bytes:X} bytes"),
            ]);
            i += run;
            continue;
        }
        let mut value = value_of(row);
        for hint in row["hints"].as_array().into_iter().flatten() {
            value.push_str("  ");
            value.push_str(str_of(hint));
        }
        if let Some(e) = row["error"].as_str() {
            value.push_str(&format!("  !! {e}"));
        }
        if row["seen"] == true {
            value.push_str("  (shown above)");
        }
        out.push(vec![
            format!("{pad}{:04X}", u64_of(&row["offset"])),
            str_of(&row["type"]).into(),
            str_of(&row["name"]).into(),
            value,
        ]);
        if let Some(children) = row["children"].as_array() {
            instance_rows(children, indent + 1, out);
            if let Some(shown) = row["elementsShown"].as_u64() {
                out.push(vec![
                    format!("{pad}  …"),
                    String::new(),
                    String::new(),
                    format!("first {shown} elements shown; pass --elements for more"),
                ]);
            }
        }
        i += 1;
    }
}

/// The reply of `inspect`.
pub fn instance(r: &Value) -> String {
    let what = r["className"].as_str().unwrap_or("memory");
    let mut out = format!(
        "{what} @ 0x{} (0x{:X} bytes)\n",
        str_of(&r["address"]),
        u64_of(&r["size"])
    );
    let mut lines = Vec::new();
    instance_rows(
        r["rows"].as_array().map(Vec::as_slice).unwrap_or(&[]),
        0,
        &mut lines,
    );
    out.push_str(&table(&lines));
    if r["truncated"] == true {
        out.push_str("… output truncated; follow fewer pointers\n");
    }
    out
}

/// A class definition from `defs`, without memory.
pub fn layout(class: &Value, root: bool) -> String {
    let mut out = format!(
        "{} #{} (0x{:X} bytes, {} refs{})\n",
        str_of(&class["name"]),
        u64_of(&class["id"]),
        u64_of(&class["size"]),
        u64_of(&class["refs"]),
        if root { ", root" } else { "" }
    );
    let fields = class["fields"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut lines = Vec::new();
    let mut i = 0;
    while i < fields.len() {
        let f = &fields[i];
        let hex = |f: &Value| f["name"].is_null() && str_of(&f["ty"]).starts_with("Hex");
        let run = fields[i..].iter().take_while(|f| hex(f)).count();
        if run > 0 {
            let bytes: u64 = fields[i..i + run].iter().map(|f| u64_of(&f["size"])).sum();
            lines.push(vec![
                format!("{:04X}", u64_of(&f["offset"])),
                format!("{bytes:X}"),
                "Hex".into(),
                "(undefined)".into(),
            ]);
            i += run;
            continue;
        }
        lines.push(vec![
            format!("{:04X}", u64_of(&f["offset"])),
            format!("{:X}", u64_of(&f["size"])),
            str_of(&f["typeLabel"]).into(),
            str_of(&f["name"]).into(),
        ]);
        i += 1;
    }
    out.push_str(&table(&lines));
    out
}

/// 16 bytes per line with an ASCII column.
pub fn hexdump(address: u64, hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect();
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let cells: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        let text: String = chunk
            .iter()
            .map(|&b| {
                if (32..127).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!(
            "{:012X}  {:<47}  {text}\n",
            address + i as u64 * 16,
            cells.join(" ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn instance_nests_and_collapses_filler() {
        let r = json!({
            "className": "Player", "address": "1000", "size": 0x28,
            "rows": [
                { "offset": 0, "size": 8, "type": "Hex64", "name": null, "value": "0000000000000000" },
                { "offset": 8, "size": 8, "type": "Hex64", "name": null, "value": "0000000000000000" },
                { "offset": 0x10, "size": 4, "type": "Int32", "name": "health", "value": "100" },
                { "offset": 0x14, "size": 4, "type": "Hex32", "name": null, "value": "42C80000", "hints": ["f32 100.0"] },
                { "offset": 0x18, "size": 8, "type": "Weapon*", "name": "weapon", "value": "2000",
                  "children": [{ "offset": 0, "size": 4, "type": "Int32", "name": "ammo", "value": "30" }] },
                { "offset": 0x20, "size": 8, "type": "Hex64", "name": null, "value": "0000000000000000" },
                { "offset": 0x28, "size": 8, "type": "Hex64", "name": null, "value": "<unreadable>", "error": "unreadable" },
                { "offset": 0x30, "size": 4, "type": "Hex32", "name": null, "value": "<unreadable>", "error": "unreadable" },
            ],
        });
        assert_eq!(
            instance(&r),
            "Player @ 0x1000 (0x28 bytes)\n\
             0000    Hex              0 × 0x10 bytes\n\
             0010    Int32    health  100\n\
             0014    Hex32            42C80000  f32 100.0\n\
             0018    Weapon*  weapon  0x2000\n\
             \x20 0000  Int32    ammo    30\n\
             0020    Hex64            0000000000000000\n\
             0028    Hex              unreadable × 0xC bytes\n"
        );
    }

    #[test]
    fn hexdump_lines() {
        assert_eq!(
            hexdump(0x10, "41424300FF"),
            "000000000010  41 42 43 00 FF                                   ABC..\n"
        );
    }
}
