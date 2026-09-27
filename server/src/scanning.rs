//! Scan commands. They run outside the workspace lock, since a scan can read
//! gigabytes, and keep their results here between commands.
//!
//! A related scan follows a watch on its own thread: whenever the watched
//! value held still it drops what changed, and whenever it changed it drops
//! what did not, which leaves the values it is computed from.

use std::{
    sync::{
        atomic::{
            AtomicBool,
            AtomicU64,
            Ordering,
        },
        Arc,
        Mutex,
    },
    thread,
    time::{
        Duration,
        Instant,
    },
};

use reclass_core::{
    scan::{
        discover,
        First,
        Hit,
        Keep,
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

/// The watch a related scan follows.
#[derive(Clone)]
pub struct Pinned {
    pub label: String,
    pub address: u64,
    pub ty: ScanType,
}

impl Pinned {
    fn read(&self, src: &dyn MemorySource) -> Option<Vec<u8>> {
        src.read_vec(self.address, self.ty.size().unwrap_or(1))
    }

    /// Watches the value for `SETTLE` and returns it if it held `before`
    /// throughout. Games often update a displayed value some frames after
    /// the values it is computed from, so a change that is under way while
    /// memory is read shows up here rather than being missed.
    fn settled(&self, src: &dyn MemorySource, before: Option<&[u8]>) -> Option<Vec<u8>> {
        let until = Instant::now() + SETTLE;
        loop {
            let now = self.read(src);
            if now.as_deref() != before {
                return None;
            }
            if Instant::now() >= until {
                return now;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Results kept aside for while a related step has the scan.
const CACHED: usize = 1000;
/// How long the watch must hold still after memory is read for a step to count.
const SETTLE: Duration = Duration::from_millis(1000);
/// Pause between related steps, so the process gets time to do something.
const PAUSE: Duration = Duration::from_millis(250);

static GENERATION: AtomicU64 = AtomicU64::new(0);

pub struct Session {
    /// `None` while a related step works on it.
    scan: Option<Scan>,
    ty: ScanType,
    size: usize,
    pid: u32,
    /// Tells a related thread whether the session it belongs to still exists.
    generation: u64,
    /// The first results and the count as of the last related step.
    cache: Vec<Hit>,
    count: u64,
    related: Option<Related>,
}

struct Related {
    pinned: Pinned,
    /// The watched value the scan's values go with; `None` until a step sees it hold still.
    baseline: Option<Vec<u8>>,
    stop: Arc<AtomicBool>,
    running: bool,
    steps: u32,
    changes: u32,
    skipped: u32,
}

impl Related {
    fn view(&self, src: &dyn MemorySource) -> Value {
        json!({
            "label": self.pinned.label,
            "address": hex(self.pinned.address),
            "value": self.pinned.read(src).map(|b| self.pinned.ty.format(&b)),
            "running": self.running,
            "steps": self.steps,
            "changes": self.changes,
            "skipped": self.skipped,
        })
    }
}

impl Session {
    fn new(scan: Scan, pid: u32, related: Option<Related>) -> Self {
        Self {
            ty: scan.ty,
            size: scan.size,
            count: scan.count(),
            cache: scan.hits(0, CACHED),
            scan: Some(scan),
            pid,
            generation: GENERATION.fetch_add(1, Ordering::Relaxed) + 1,
            related,
        }
    }

    fn stop(&self) {
        if let Some(r) = &self.related {
            r.stop.store(true, Ordering::Relaxed);
        }
    }
}

pub type Shared = Arc<Mutex<Option<Session>>>;

fn arg<T: serde::de::DeserializeOwned>(p: &Value) -> Result<T, String> {
    serde_json::from_value(p.clone()).map_err(|e| format!("bad params: {e}"))
}

pub fn is_scan_method(method: &str) -> bool {
    matches!(
        method,
        "scan" | "scanNext" | "scanResults" | "scanClear" | "scanRelated" | "scanRelatedStop"
    )
}

/// Replaces the session, stopping a related thread working on the old one.
fn replace(shared: &Shared, session: Option<Session>) {
    let mut guard = shared.lock().unwrap();
    if let Some(old) = guard.as_ref() {
        old.stop();
    }
    *guard = session;
}

/// `pinned` is the watch a `scanRelated` follows.
pub fn handle(
    shared: &Shared,
    target: Option<Target>,
    pinned: Option<Pinned>,
    method: &str,
    p: &Value,
) -> Result<Value, String> {
    if method == "scanClear" {
        replace(shared, None);
        return Ok(Value::Null);
    }
    if method == "scanRelatedStop" {
        let generation = {
            let guard = shared.lock().unwrap();
            let s = guard
                .as_ref()
                .filter(|s| s.related.is_some())
                .ok_or("no related scan")?;
            s.stop();
            s.generation
        };
        // Wait for the step under way, so the scan can be narrowed right after.
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            let running = shared.lock().unwrap().as_ref().is_some_and(|s| {
                s.generation == generation && s.related.as_ref().is_some_and(|r| r.running)
            });
            if !running {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        return Ok(Value::Null);
    }
    let target = target.ok_or("not attached")?;
    let src = &*target.source;
    let pid = src.process().pid;
    let started = Instant::now();
    if method == "scan" || method == "scanRelated" {
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
        let cond = match (method, a.unknown, a.value, a.min, a.max) {
            ("scanRelated", _, None, None, None) => First::Unknown,
            ("scanRelated", ..) => return Err("a related scan takes no value".into()),
            (_, true, None, None, None) => First::Unknown,
            (_, false, Some(v), None, None) => First::Exact(v),
            (_, false, None, Some(lo), Some(hi)) => First::Between(lo, hi),
            _ => return Err("pass one of: value, min and max, or unknown".into()),
        };
        // Drop the old results first; they may be large.
        replace(shared, None);
        let regions = discover(src, target.pointer_size, target.within, a.read_only);
        let bytes: u64 = regions.iter().map(|r| r.size).sum();
        let related = match pinned {
            Some(pinned) if method == "scanRelated" => {
                let before = pinned.read(src).ok_or("the watched value cannot be read")?;
                let scan = Scan::first(src, &regions, ty, &cond, a.align)?;
                // If the watch changed while memory was copied, or soon after,
                // the copy may be half before and half after; the first step
                // then takes a new one.
                let baseline = pinned.settled(src, Some(&before));
                Some((scan, pinned, baseline))
            }
            None if method == "scanRelated" => return Err("pass the watch to follow".into()),
            _ => None,
        };
        let session = match related {
            Some((scan, pinned, baseline)) => {
                let stop = Arc::new(AtomicBool::new(false));
                let session = Session::new(
                    scan,
                    pid,
                    Some(Related {
                        pinned: pinned.clone(),
                        baseline,
                        stop: stop.clone(),
                        running: true,
                        steps: 0,
                        changes: 0,
                        skipped: 0,
                    }),
                );
                let (shared, source, generation) =
                    (shared.clone(), target.source.clone(), session.generation);
                thread::spawn(move || follow(shared, source, pinned, generation, stop));
                session
            }
            None => Session::new(Scan::first(src, &regions, ty, &cond, a.align)?, pid, None),
        };
        let count = session.count;
        *shared.lock().unwrap() = Some(session);
        return Ok(json!({
            "count": count,
            "type": ty.name(),
            "regions": regions.len(),
            "bytes": bytes,
            "ms": started.elapsed().as_millis() as u64,
        }));
    }
    let mut guard = shared.lock().unwrap();
    let s = guard.as_mut().ok_or("no scan yet; start one with `scan`")?;
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
            if s.related.as_ref().is_some_and(|r| r.running) {
                return Err("a related scan is running; stop it first".into());
            }
            let scan = s
                .scan
                .as_mut()
                .ok_or("a related scan is finishing a step; try again")?;
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
            scan.next(src, &cond)?;
            s.count = scan.count();
            s.cache = scan.hits(0, CACHED);
            // Narrowed by hand, the scan no longer follows the watch.
            s.related = None;
            Ok(json!({ "count": s.count, "ms": started.elapsed().as_millis() as u64 }))
        }
        "scanResults" => {
            #[derive(Deserialize)]
            struct A {
                #[serde(default)]
                offset: u64,
                limit: Option<usize>,
            }
            let a: A = arg(p)?;
            let limit = a.limit.unwrap_or(50).min(1000);
            let (hits, count) = match &s.scan {
                Some(scan) => (scan.hits(a.offset, limit), scan.count()),
                None => (
                    s.cache
                        .iter()
                        .skip(a.offset as usize)
                        .take(limit)
                        .cloned()
                        .collect(),
                    s.count,
                ),
            };
            let results: Vec<Value> = hits
                .into_iter()
                .map(|h| {
                    let current = src.read_vec(h.address, s.size);
                    json!({
                        "address": hex(h.address),
                        "value": current.as_deref().map(|b| s.ty.format(b)),
                        "previous": s.ty.format(&h.value),
                        "symbol": src.symbolize(h.address),
                    })
                })
                .collect();
            let mut reply = json!({ "count": count, "type": s.ty.name(), "results": results });
            if let Some(r) = &s.related {
                reply["related"] = r.view(src);
            }
            Ok(reply)
        }
        _ => Err(format!("unknown method {method}")),
    }
}

/// What a related step keeps, from the watched value at the baseline, before
/// the step read memory and once it settled. Changes that overlap the read
/// cannot be told apart from noise, so those steps keep nothing.
fn decide(baseline: Option<&[u8]>, before: Option<&[u8]>, after: Option<&[u8]>) -> Option<Keep> {
    let (before, after) = (before?, after?);
    if before != after {
        return None;
    }
    Some(match baseline {
        None => Keep::All,
        Some(b) if b == before => Keep::Unchanged,
        Some(_) => Keep::Changed,
    })
}

/// Runs related steps until stopped, the session is replaced, or nothing is left.
fn follow(
    shared: Shared,
    src: Arc<dyn MemorySource>,
    pinned: Pinned,
    generation: u64,
    stop: Arc<AtomicBool>,
) {
    let mine = |s: &Option<Session>| s.as_ref().is_some_and(|s| s.generation == generation);
    while !stop.load(Ordering::Relaxed) {
        let (mut scan, baseline) = {
            let mut guard = shared.lock().unwrap();
            if !mine(&guard) {
                return;
            }
            let s = guard.as_mut().unwrap();
            let baseline = s.related.as_ref().and_then(|r| r.baseline.clone());
            match s.scan.take() {
                Some(scan) => (scan, baseline),
                None => return,
            }
        };
        let before = pinned.read(&*src);
        let mut after = None;
        let kept = scan.next_deferred(&*src, || {
            after = pinned.settled(&*src, before.as_deref());
            decide(baseline.as_deref(), before.as_deref(), after.as_deref())
        });
        let mut guard = shared.lock().unwrap();
        if !mine(&guard) {
            return;
        }
        let s = guard.as_mut().unwrap();
        s.count = scan.count();
        s.cache = scan.hits(0, CACHED);
        s.scan = Some(scan);
        let Some(r) = s.related.as_mut() else {
            return;
        };
        r.steps += 1;
        match kept {
            None => r.skipped += 1,
            Some(Keep::Changed) => r.changes += 1,
            _ => {}
        }
        if kept.is_some() {
            r.baseline = after;
        }
        if s.count == 0 {
            break;
        }
        drop(guard);
        thread::sleep(PAUSE);
    }
    let mut guard = shared.lock().unwrap();
    if mine(&guard) {
        if let Some(r) = guard.as_mut().unwrap().related.as_mut() {
            r.running = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use reclass_core::demo::DemoSource;

    use super::*;

    fn run(
        shared: &Shared,
        target: Option<Target>,
        method: &str,
        p: Value,
    ) -> Result<Value, String> {
        handle(shared, target, None, method, &p)
    }

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
        let shared = Shared::default();
        let first = run(
            &shared,
            target(),
            "scan",
            json!({ "type": "i32", "value": "31337" }),
        )
        .unwrap();
        assert_eq!(first["count"], 1);
        assert_eq!(first["bytes"], 0x1000);

        src.poke(spot + 0x10, &31300i32.to_le_bytes());
        let next = run(
            &shared,
            target(),
            "scanNext",
            json!({ "cond": "decreasedBy", "value": "37" }),
        )
        .unwrap();
        assert_eq!(next["count"], 1);
        src.poke(spot + 0x10, &5i32.to_le_bytes());
        let results = run(&shared, target(), "scanResults", json!({})).unwrap();
        assert_eq!(
            results["results"],
            json!([{ "address": "1F3A8F80010", "value": "5", "previous": "31300", "symbol": null }])
        );

        assert!(run(
            &shared,
            target(),
            "scan",
            json!({ "type": "i32", "value": "1", "unknown": true })
        )
        .is_err());
        assert!(run(&shared, None, "scanNext", json!({ "cond": "changed" })).is_err());
        run(&shared, None, "scanClear", Value::Null).unwrap();
        assert!(run(&shared, target(), "scanResults", json!({})).is_err());
    }

    #[test]
    fn related_steps_keep_what_changes_with_the_watch() {
        let src = Arc::new(DemoSource::new());
        let spot = 0x1F3_A8F8_0000u64;
        let (ratio, health, noise) = (spot + 0x800, spot + 0x10, spot + 0x20);
        src.poke(spot, &[0; 0x1000]);
        src.poke(ratio, &1.0f32.to_le_bytes());
        src.poke(health, &100i32.to_le_bytes());
        let target = || {
            Some(Target {
                source: src.clone(),
                pointer_size: 8,
                within: Some((spot, spot + 0x800)),
            })
        };
        let pinned = Pinned {
            label: "ratio".into(),
            address: ratio,
            ty: ScanType::parse("f32").unwrap(),
        };
        let shared = Shared::default();
        handle(
            &shared,
            target(),
            Some(pinned),
            "scanRelated",
            &json!({ "type": "i32" }),
        )
        .unwrap();

        // Play: noise ticks all the time, and now and then health drops and
        // the ratio follows a little later, the way a health bar animates.
        let deadline = Instant::now() + Duration::from_secs(40);
        let mut hp = 100;
        let left = loop {
            for i in 0..8 {
                src.poke(noise, &(i as i32 + hp).to_le_bytes());
                thread::sleep(Duration::from_millis(120));
            }
            hp -= 7;
            src.poke(health, &hp.to_le_bytes());
            thread::sleep(Duration::from_millis(400));
            src.poke(ratio, &(hp as f32 / 100.0).to_le_bytes());
            let r = run(&shared, target(), "scanResults", json!({})).unwrap();
            let left: Vec<String> = r["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h["address"].as_str().unwrap().to_string())
                .collect();
            if left.len() <= 1 || Instant::now() > deadline {
                assert!(r["related"]["steps"].as_u64().unwrap() > 0);
                break left;
            }
        };
        assert_eq!(left, vec![hex(health)]);
        assert!(run(&shared, target(), "scanNext", json!({ "cond": "changed" })).is_err());
        run(&shared, None, "scanRelatedStop", Value::Null).unwrap();
        run(&shared, None, "scanClear", Value::Null).unwrap();
    }
}
