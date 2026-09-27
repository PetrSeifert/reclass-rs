//! Scan commands. They run outside the workspace lock, since a scan can read
//! gigabytes, and keep their results here between commands.

use std::{
    sync::Arc,
    time::Instant,
};

use reclass_core::{
    scan::{
        discover,
        First,
        Next,
        Scan,
        ScanType,
    },
    source::MemorySource,
};
use serde::Deserialize;
use serde_json::{
    json,
    Value,
};

use crate::canvas::hex;

/// The attached process and where to scan it, taken from the workspace.
pub struct Target {
    pub source: Arc<dyn MemorySource>,
    pub pointer_size: u64,
    /// `[start, end)` to scan instead of all readable memory.
    pub within: Option<(u64, u64)>,
}

pub struct Session {
    scan: Scan,
    pid: u32,
}

fn arg<T: serde::de::DeserializeOwned>(p: &Value) -> Result<T, String> {
    serde_json::from_value(p.clone()).map_err(|e| format!("bad params: {e}"))
}

pub fn is_scan_method(method: &str) -> bool {
    matches!(method, "scan" | "scanNext" | "scanResults" | "scanClear")
}

pub fn handle(
    session: &mut Option<Session>,
    target: Option<Target>,
    method: &str,
    p: &Value,
) -> Result<Value, String> {
    if method == "scanClear" {
        *session = None;
        return Ok(Value::Null);
    }
    let target = target.ok_or("not attached")?;
    let src = &*target.source;
    let pid = src.process().pid;
    let started = Instant::now();
    if method == "scan" {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct A {
            #[serde(rename = "type")]
            ty: String,
            value: Option<String>,
            min: Option<String>,
            max: Option<String>,
            #[serde(default)]
            unknown: bool,
            align: Option<u64>,
            #[serde(default)]
            read_only: bool,
        }
        let a: A = arg(p)?;
        let ty = ScanType::parse(&a.ty)?;
        let cond = match (a.unknown, a.value, a.min, a.max) {
            (true, None, None, None) => First::Unknown,
            (false, Some(v), None, None) => First::Exact(v),
            (false, None, Some(lo), Some(hi)) => First::Between(lo, hi),
            _ => return Err("pass one of: value, min and max, or unknown".into()),
        };
        // Drop the old results first; they may be large.
        *session = None;
        let regions = discover(src, target.pointer_size, target.within, a.read_only);
        let bytes: u64 = regions.iter().map(|r| r.size).sum();
        let scan = Scan::first(src, &regions, ty, &cond, a.align)?;
        let count = scan.count();
        *session = Some(Session { scan, pid });
        return Ok(json!({
            "count": count,
            "type": ty.name(),
            "regions": regions.len(),
            "bytes": bytes,
            "ms": started.elapsed().as_millis() as u64,
        }));
    }
    let s = session
        .as_mut()
        .ok_or("no scan yet; start one with `scan`")?;
    if s.pid != pid {
        return Err("the scan was of another process; start a new one".into());
    }
    match method {
        "scanNext" => {
            #[derive(Deserialize)]
            struct A {
                cond: String,
                value: Option<String>,
                min: Option<String>,
                max: Option<String>,
            }
            let a: A = arg(p)?;
            let need = |v: Option<String>| v.ok_or(format!("{} needs a value", a.cond));
            let cond = match a.cond.as_str() {
                "exact" => Next::Exact(need(a.value)?),
                "between" => Next::Between(need(a.min)?, need(a.max)?),
                "changed" => Next::Changed,
                "unchanged" => Next::Unchanged,
                "increased" => Next::Increased,
                "decreased" => Next::Decreased,
                "increasedBy" => Next::IncreasedBy(need(a.value)?),
                "decreasedBy" => Next::DecreasedBy(need(a.value)?),
                other => return Err(format!("unknown condition '{other}'")),
            };
            s.scan.next(src, &cond)?;
            Ok(json!({ "count": s.scan.count(), "ms": started.elapsed().as_millis() as u64 }))
        }
        "scanResults" => {
            #[derive(Deserialize)]
            struct A {
                #[serde(default)]
                offset: u64,
                limit: Option<usize>,
            }
            let a: A = arg(p)?;
            let size = s.scan.size;
            let results: Vec<Value> = s
                .scan
                .hits(a.offset, a.limit.unwrap_or(50).min(1000))
                .into_iter()
                .map(|h| {
                    let current = src.read_vec(h.address, size);
                    json!({
                        "address": hex(h.address),
                        "value": current.as_deref().map(|b| s.scan.format(b)),
                        "previous": s.scan.format(&h.value),
                        "symbol": src.symbolize(h.address),
                    })
                })
                .collect();
            Ok(json!({ "count": s.scan.count(), "type": s.scan.ty.name(), "results": results }))
        }
        _ => Err(format!("unknown method {method}")),
    }
}

#[cfg(test)]
mod tests {
    use reclass_core::demo::DemoSource;

    use super::*;

    #[test]
    fn scan_next_and_list_results() {
        let src = Arc::new(DemoSource::new());
        let spot = 0x1F3_A8F8_0000;
        src.poke(spot, &[0; 0x1000]);
        src.poke(spot + 0x10, &31337i32.to_le_bytes());
        let target = || {
            Some(Target {
                source: src.clone(),
                pointer_size: 8,
                within: Some((spot, spot + 0x1000)),
            })
        };
        let mut session = None;
        let first = handle(
            &mut session,
            target(),
            "scan",
            &json!({ "type": "i32", "value": "31337" }),
        )
        .unwrap();
        assert_eq!(first["count"], 1);
        assert_eq!(first["bytes"], 0x1000);

        src.poke(spot + 0x10, &31300i32.to_le_bytes());
        let next = handle(
            &mut session,
            target(),
            "scanNext",
            &json!({ "cond": "decreasedBy", "value": "37" }),
        )
        .unwrap();
        assert_eq!(next["count"], 1);
        src.poke(spot + 0x10, &5i32.to_le_bytes());
        let results = handle(&mut session, target(), "scanResults", &json!({})).unwrap();
        assert_eq!(
            results["results"],
            json!([{ "address": "1F3A8F80010", "value": "5", "previous": "31300", "symbol": null }])
        );

        assert!(handle(
            &mut session,
            target(),
            "scan",
            &json!({ "type": "i32", "value": "1", "unknown": true })
        )
        .is_err());
        assert!(handle(
            &mut session,
            None,
            "scanNext",
            &json!({ "cond": "changed" })
        )
        .is_err());
        handle(&mut session, None, "scanClear", &Value::Null).unwrap();
        assert!(handle(&mut session, target(), "scanResults", &json!({})).is_err());
    }
}
