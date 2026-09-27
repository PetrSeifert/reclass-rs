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

/// The reply of `scanResults`: address, value, the value at the last scan when
/// it differs, and the module the address is in.
pub fn scan_results(r: &Value, offset: u64) -> String {
    let rows: Vec<Vec<String>> = r["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            let value = h["value"].as_str().unwrap_or("<unreadable>");
            let previous = str_of(&h["previous"]);
            vec![
                format!("0x{}", str_of(&h["address"])),
                value.to_string(),
                if previous == value {
                    String::new()
                } else {
                    format!("was {previous}")
                },
                str_of(&h["symbol"]).to_string(),
            ]
        })
        .collect();
    let mut out = String::new();
    let related = &r["related"];
    if related.is_object() {
        out.push_str(&format!(
            "{} {} (0x{} = {}): {} steps, {} with a change, {} skipped\n",
            if related["running"] == true {
                "following"
            } else {
                "followed"
            },
            str_of(&related["label"]),
            str_of(&related["address"]),
            related["value"].as_str().unwrap_or("<unreadable>"),
            related["steps"],
            related["changes"],
            related["skipped"],
        ));
    }
    out.push_str(&table(&rows));
    // Only the watch left, or nothing: its inputs likely have another type.
    let list = r["results"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let only_watch = list.len() == 1 && list[0]["address"] == related["address"];
    if related.is_object() && u64_of(&related["changes"]) > 0 && (list.is_empty() || only_watch) {
        out.push_str(&format!(
            "{} The values it comes from may be stored as another type: start again with a different one.
",
            if only_watch {
                "Only the watch itself is left.".to_string()
            } else {
                format!("No {} value changed along with it.", str_of(&r["type"]))
            }
        ));
    }
    let count = u64_of(&r["count"]);
    let shown = offset + rows.len() as u64;
    if shown < count {
        out.push_str(&format!(
            "… {} more; pass --offset {shown}\n",
            count - shown
        ));
    }
    out
}

/// A number with at most three decimals and no trailing zeros.
fn short(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// The last `width` samples as block characters, scaled between their extremes.
fn sparkline(history: &[Value], width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let recent = &history[history.len().saturating_sub(width)..];
    let nums: Vec<f64> = recent.iter().filter_map(Value::as_f64).collect();
    let (lo, hi) = nums
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    recent
        .iter()
        .map(|v| match v.as_f64() {
            None => ' ',
            Some(_) if hi <= lo => BARS[3],
            Some(v) => BARS[((v - lo) / (hi - lo) * 7.0).round() as usize],
        })
        .collect()
}

/// Watches with their values, the last samples and their range.
pub fn watches(r: &Value) -> String {
    let list = r["watches"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if list.is_empty() {
        return "no watches; add one with `reclass watch add EXPR TYPE`\n".into();
    }
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|w| {
            let history = w["history"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            let nums: Vec<f64> = history.iter().filter_map(Value::as_f64).collect();
            let range = match (
                nums.iter().copied().reduce(f64::min),
                nums.iter().copied().reduce(f64::max),
            ) {
                (Some(lo), Some(hi)) if lo != hi => format!("{}..{}", short(lo), short(hi)),
                _ => String::new(),
            };
            let value = match (&w["value"], &w["error"]) {
                (Value::String(v), _) => v.clone(),
                (_, e) => format!("<{}>", str_of(e)),
            };
            vec![
                format!("#{}", w["id"]),
                str_of(&w["label"]).to_string(),
                w["address"]
                    .as_str()
                    .map(|a| format!("0x{a}"))
                    .unwrap_or_default(),
                str_of(&w["type"]).to_string(),
                value,
                if w["frozen"].is_string() {
                    "frozen".into()
                } else {
                    String::new()
                },
                sparkline(history, 40),
                range,
            ]
        })
        .collect();
    table(&rows)
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
