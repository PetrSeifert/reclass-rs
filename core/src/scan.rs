//! Value scanning, Cheat Engine style: a first scan finds the addresses that
//! hold a value (or snapshots all memory when the value is unknown), and each
//! next scan keeps those whose value still matches, or changed as described.
//!
//! Drivers cannot list memory regions, so readable memory is found by reading:
//! a 32-bit address space is small enough to probe whole, and a 64-bit one is
//! crawled from the modules by following pointer-like values.

use std::collections::{
    BTreeSet,
    HashSet,
};

use crate::{
    decode::fmt_float,
    source::{
        MemorySource,
        ModuleEntry,
    },
};

const PAGE: u64 = 0x1000;
/// Probing granularity: Windows allocations are 64 KiB aligned.
const CHUNK: u64 = 0x1_0000;
/// Bytes read at once while scanning.
const BLOCK: u64 = 0x10_0000;
/// Most addresses a scan keeps as a list.
pub const MAX_LISTED: u64 = 10_000_000;
/// Most bytes an unknown-value scan keeps a copy of.
pub const MAX_SNAPSHOT: u64 = 2 << 30;
/// Most bytes a 64-bit crawl collects, and how many pointer hops it follows.
const MAX_CRAWL: u64 = 8 << 30;
const CRAWL_ROUNDS: usize = 4;
/// Highest user-mode address of a 64-bit process.
const USER_END: u64 = 0x8000_0000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub base: u64,
    pub size: u64,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.base + self.size
    }
}

/// Appends `[lo, hi)`, merging it into the last region if they touch.
fn push(out: &mut Vec<Region>, lo: u64, hi: u64) {
    match out.last_mut() {
        Some(last) if last.end() == lo => last.size += hi - lo,
        _ => out.push(Region {
            base: lo,
            size: hi - lo,
        }),
    }
}

/// Sorts and merges overlapping or touching regions.
fn normalize(mut regions: Vec<Region>) -> Vec<Region> {
    regions.sort_by_key(|r| r.base);
    let mut out: Vec<Region> = Vec::new();
    for r in regions {
        match out.last_mut() {
            Some(last) if r.base <= last.end() => {
                let end = last.end().max(r.end());
                last.size = end - last.base;
            }
            _ => out.push(r),
        }
    }
    out
}

/// `regions` without the parts covered by `holes` (both sorted and merged).
fn subtract(regions: &[Region], holes: &[Region]) -> Vec<Region> {
    let mut out = Vec::new();
    for r in regions {
        let mut lo = r.base;
        for h in holes
            .iter()
            .filter(|h| h.base < r.end() && h.end() > r.base)
        {
            if h.base > lo {
                push(&mut out, lo, h.base);
            }
            lo = lo.max(h.end());
        }
        if lo < r.end() {
            push(&mut out, lo, r.end());
        }
    }
    out
}

/// Readable spans of `[start, end)`: whole 64 KiB chunks where they read, and
/// the readable pages of chunks that do not.
pub fn probe(src: &dyn MemorySource, start: u64, end: u64) -> Vec<Region> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; CHUNK as usize];
    let mut chunk = start & !(CHUNK - 1);
    while chunk < end {
        let (lo, hi) = (chunk.max(start), (chunk + CHUNK).min(end));
        if src.read(lo, &mut buf[..(hi - lo) as usize]).is_ok() {
            push(&mut out, lo, hi);
        } else if src.read(lo, &mut buf[..1]).is_ok() || src.read(hi - 1, &mut buf[..1]).is_ok() {
            // Allocations commit from their start (heaps) or their end (stacks),
            // so a chunk readable nowhere at either end is taken as empty.
            let mut page = lo & !(PAGE - 1);
            while page < hi {
                let (plo, phi) = (page.max(lo), (page + PAGE).min(hi));
                // Protection is per page, so one byte tells for the whole page.
                if src.read(plo, &mut buf[..1]).is_ok() {
                    push(&mut out, plo, phi);
                }
                page += PAGE;
            }
        }
        chunk += CHUNK;
    }
    out
}

/// Calls `f(address, bytes, len)` for the memory of `regions` in blocks, where
/// `len` bytes start in the block and `overlap` more follow so values that
/// cross into the next block can be read. Unreadable pages are skipped.
fn for_each_block(
    src: &dyn MemorySource,
    regions: &[Region],
    overlap: u64,
    mut f: impl FnMut(u64, &[u8], usize),
) {
    let mut buf = Vec::new();
    for r in regions {
        let mut at = r.base;
        while at < r.end() {
            let len = BLOCK.min(r.end() - at);
            let with = (len + overlap).min(r.end() - at);
            buf.resize(with as usize, 0);
            if src.read(at, &mut buf).is_ok() {
                f(at, &buf, len as usize);
            } else {
                // Freed since it was found: take what is still there, page by page.
                let mut page = at;
                while page < at + len {
                    let n = (PAGE - page % PAGE).min(at + len - page);
                    buf.resize(n as usize, 0);
                    if src.read(page, &mut buf).is_ok() {
                        f(page, &buf, n as usize);
                    }
                    page += n;
                }
            }
            at += len;
        }
    }
}

/// Parts of a module image that are not writable (headers, code, constants),
/// from its section table. `None` if the headers cannot be read.
fn image_read_only(src: &dyn MemorySource, m: &ModuleEntry) -> Option<Vec<Region>> {
    let u16_at = |a: u64| {
        let mut b = [0u8; 2];
        src.read(a, &mut b).ok().map(|_| u16::from_le_bytes(b))
    };
    let mut mz = [0u8; 2];
    src.read(m.base, &mut mz).ok()?;
    if &mz != b"MZ" {
        return None;
    }
    let nt = m.base + src.read_u32(m.base + 0x3C)? as u64;
    if src.read_u32(nt)? != u32::from_le_bytes(*b"PE\0\0") {
        return None;
    }
    let sections = u16_at(nt + 6)? as u64;
    let table = nt + 0x18 + u16_at(nt + 0x14)? as u64;
    let mut out = Vec::new();
    let mut first = m.size;
    for i in 0..sections {
        let s = table + i * 0x28;
        let size = src.read_u32(s + 8)? as u64;
        let rva = src.read_u32(s + 0xC)? as u64;
        let flags = src.read_u32(s + 0x24)?;
        first = first.min(rva);
        const WRITABLE: u32 = 0x8000_0000;
        if flags & WRITABLE == 0 {
            let end = (rva + size).div_ceil(PAGE) * PAGE;
            out.push(Region {
                base: m.base + rva,
                size: end - rva,
            });
        }
    }
    out.push(Region {
        base: m.base,
        size: first,
    });
    Some(normalize(out))
}

/// Memory of a 64-bit process reachable from `roots`: pointer-like values in
/// them name 64 KiB chunks, which are probed and grown into the contiguous
/// readable memory around them, which is searched for pointers in turn.
fn crawl(src: &dyn MemorySource, modules: &[ModuleEntry], roots: &[Region]) -> Vec<Region> {
    let in_module = |v: u64| modules.iter().any(|m| v >= m.base && v < m.base + m.size);
    let mut tried: HashSet<u64> = HashSet::new();
    let mut found = Vec::new();
    let mut total = 0;
    let mut frontier = roots.to_vec();
    for _ in 0..CRAWL_ROUNDS {
        let mut seeds = BTreeSet::new();
        for_each_block(src, &frontier, 0, |_, bytes, len| {
            for q in bytes[..len].as_chunks::<8>().0 {
                let v = u64::from_le_bytes(*q);
                if (CHUNK..USER_END).contains(&v) && !in_module(v) && !tried.contains(&(v / CHUNK))
                {
                    seeds.insert(v / CHUNK);
                }
            }
        });
        let mut next = Vec::new();
        // Probes chunk `c` once; true if any of it is readable.
        let mut take = |c: u64, next: &mut Vec<Region>| {
            if !tried.insert(c) {
                return 0;
            }
            let spans = probe(src, c * CHUNK, (c + 1) * CHUNK);
            let bytes = spans.iter().map(|r| r.size).sum::<u64>();
            next.extend(spans);
            bytes
        };
        for c in seeds {
            if total > MAX_CRAWL {
                break;
            }
            let bytes = take(c, &mut next);
            if bytes == 0 {
                continue;
            }
            total += bytes;
            // Heaps are contiguous: grow the span both ways while it reads.
            let mut down = c;
            while down > 1 && total <= MAX_CRAWL {
                let bytes = take(down - 1, &mut next);
                if bytes == 0 {
                    break;
                }
                total += bytes;
                down -= 1;
            }
            let mut up = c;
            while up + 1 < USER_END / CHUNK && total <= MAX_CRAWL {
                let bytes = take(up + 1, &mut next);
                if bytes == 0 {
                    break;
                }
                total += bytes;
                up += 1;
            }
        }
        if next.is_empty() {
            break;
        }
        found.extend(next.iter().copied());
        frontier = next;
    }
    found
}

/// Readable memory to scan: `within` if given, else everything readable of a
/// 32-bit process, or the modules plus what a crawl reaches for a 64-bit one.
/// Unless `read_only` is set, read-only parts of module images are left out.
pub fn discover(
    src: &dyn MemorySource,
    pointer_size: u64,
    within: Option<(u64, u64)>,
    read_only: bool,
) -> Vec<Region> {
    let modules = src.modules();
    let holes = || {
        let mut holes = Vec::new();
        for m in &modules {
            holes.extend(image_read_only(src, m).unwrap_or_default());
        }
        normalize(holes)
    };
    let regions = match within {
        Some((start, end)) => probe(src, start, end),
        None if pointer_size == 4 => probe(src, CHUNK, 1 << 32),
        None => {
            let mut images = Vec::new();
            for m in &modules {
                images.extend(probe(src, m.base, m.base + m.size));
            }
            let images = normalize(images);
            let data = subtract(&images, &holes());
            let mut all = crawl(src, &modules, &data);
            all.extend(images);
            normalize(all)
        }
    };
    if read_only {
        regions
    } else {
        subtract(&regions, &holes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numeric {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl Numeric {
    fn size(self) -> usize {
        use Numeric::*;
        match self {
            I8 | U8 => 1,
            I16 | U16 => 2,
            I32 | U32 | F32 => 4,
            I64 | U64 | F64 => 8,
        }
    }

    fn is_float(self) -> bool {
        matches!(self, Numeric::F32 | Numeric::F64)
    }

    fn signed(self) -> bool {
        matches!(
            self,
            Numeric::I8 | Numeric::I16 | Numeric::I32 | Numeric::I64
        )
    }

    fn read(self, b: &[u8]) -> Num {
        use Numeric::*;
        let mut raw = [0u8; 8];
        raw[..self.size()].copy_from_slice(&b[..self.size()]);
        let u = u64::from_le_bytes(raw);
        match self {
            F32 => Num::F(f32::from_bits(u as u32) as f64),
            F64 => Num::F(f64::from_bits(u)),
            I8 => Num::I(u as u8 as i8 as i128),
            I16 => Num::I(u as u16 as i16 as i128),
            I32 => Num::I(u as u32 as i32 as i128),
            I64 => Num::I(u as i64 as i128),
            _ => Num::I(u as i128),
        }
    }
}

/// What a scan looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanType {
    Num(Numeric),
    /// An exact byte string: UTF-8 text.
    Text,
    /// An exact byte string with `??` wildcards.
    Bytes,
}

impl ScanType {
    pub fn parse(s: &str) -> Result<Self, String> {
        use Numeric::*;
        Ok(ScanType::Num(match s.to_ascii_lowercase().as_str() {
            "i8" | "int8" => I8,
            "i16" | "int16" => I16,
            "i32" | "int32" | "int" => I32,
            "i64" | "int64" => I64,
            "u8" | "uint8" | "byte" => U8,
            "u16" | "uint16" => U16,
            "u32" | "uint32" => U32,
            "u64" | "uint64" => U64,
            "f32" | "float" => F32,
            "f64" | "double" => F64,
            "text" | "string" => return Ok(ScanType::Text),
            "bytes" | "aob" => return Ok(ScanType::Bytes),
            other => return Err(format!("unknown scan type '{other}'")),
        }))
    }

    pub fn name(self) -> &'static str {
        use Numeric::*;
        match self {
            ScanType::Num(n) => match n {
                I8 => "i8",
                I16 => "i16",
                I32 => "i32",
                I64 => "i64",
                U8 => "u8",
                U16 => "u16",
                U32 => "u32",
                U64 => "u64",
                F32 => "f32",
                F64 => "f64",
            },
            ScanType::Text => "text",
            ScanType::Bytes => "bytes",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Num {
    I(i128),
    F(f64),
}

impl Num {
    fn f(self) -> f64 {
        match self {
            Num::I(i) => i as f64,
            Num::F(f) => f,
        }
    }
}

/// A value to compare with, parsed for the scan's type.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Int(i128),
    /// A float matches within `tol`: half a unit of the last digit typed,
    /// so `100` matches 99.5..100.5 and `1.25` matches 1.245..1.255.
    Float {
        v: f64,
        tol: f64,
    },
    Pattern {
        bytes: Vec<u8>,
        mask: Vec<u8>,
    },
}

pub(crate) fn parse_int(s: &str) -> Option<i128> {
    let s = s.trim();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    let v = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(h) => i128::from_str_radix(h, 16).ok()?,
        None => digits.parse::<i128>().ok()?,
    };
    Some(if neg { -v } else { v })
}

fn parse_value(ty: ScanType, s: &str) -> Result<Value, String> {
    match ty {
        ScanType::Num(n) if n.is_float() => {
            let v: f64 = s
                .trim()
                .parse()
                .map_err(|_| format!("'{s}' is not a number"))?;
            let t = s.trim().to_ascii_lowercase();
            let tol = if t.contains('e') {
                v.abs() * 1e-6
            } else {
                let decimals = t.split_once('.').map(|(_, d)| d.len()).unwrap_or(0);
                0.5 * 10f64.powi(-(decimals as i32))
            };
            Ok(Value::Float { v, tol })
        }
        ScanType::Num(n) => {
            let v =
                parse_int(s).ok_or_else(|| format!("'{s}' is not an integer (use 0x for hex)"))?;
            let bits = n.size() as u32 * 8;
            let (min, max) = if n.signed() {
                (-(1i128 << (bits - 1)), (1i128 << bits) - 1)
            } else {
                (0, (1i128 << bits) - 1)
            };
            // Signed types also take their unsigned spelling, e.g. 0xFFFFFFFF for -1.
            if v < min || v > max {
                return Err(format!("{v} does not fit in {}", ty.name()));
            }
            Ok(Value::Int(v))
        }
        ScanType::Text => {
            if s.is_empty() {
                return Err("text to find cannot be empty".into());
            }
            Ok(Value::Pattern {
                bytes: s.as_bytes().to_vec(),
                mask: vec![0xFF; s.len()],
            })
        }
        ScanType::Bytes => {
            let mut bytes = Vec::new();
            let mut mask = Vec::new();
            for t in s.split_whitespace() {
                if t.chars().all(|c| c == '?') && t.len() <= 2 {
                    bytes.push(0);
                    mask.push(0);
                } else {
                    let b = u8::from_str_radix(t, 16)
                        .map_err(|_| format!("'{t}' is not a hex byte or ??"))?;
                    bytes.push(b);
                    mask.push(0xFF);
                }
            }
            if mask.iter().all(|m| *m == 0) {
                return Err("byte pattern needs at least one fixed byte".into());
            }
            Ok(Value::Pattern { bytes, mask })
        }
    }
}

/// How the first scan picks addresses.
#[derive(Clone, Debug, PartialEq)]
pub enum First {
    Exact(String),
    Between(String, String),
    /// Every address, to be narrowed by comparing later scans.
    Unknown,
}

/// How a next scan narrows the addresses.
#[derive(Clone, Debug, PartialEq)]
pub enum Next {
    Exact(String),
    Between(String, String),
    Changed,
    Unchanged,
    Increased,
    Decreased,
    IncreasedBy(String),
    DecreasedBy(String),
}

enum Test {
    /// Exactly these bytes. Integers are compared this way too, so -1 and
    /// 0xFF are the same i8.
    Bytes(Vec<u8>),
    /// These bytes where `mask` is set: patterns with `??` wildcards.
    Masked {
        bytes: Vec<u8>,
        mask: Vec<u8>,
    },
    /// A float within `tol` of `v`.
    Near {
        v: f64,
        tol: f64,
    },
    Between(Num, Num),
    Changed,
    Unchanged,
    Increased,
    Decreased,
    /// `current - previous == by`, within `tol` for floats.
    Delta {
        by: Num,
        tol: f64,
    },
}

struct Matcher {
    ty: ScanType,
    test: Test,
}

impl Matcher {
    fn number(&self) -> Result<Numeric, String> {
        match self.ty {
            ScanType::Num(n) => Ok(n),
            t => Err(format!(
                "{} scans only compare equal, changed or unchanged",
                t.name()
            )),
        }
    }

    fn bound(&self, s: &str) -> Result<Num, String> {
        Ok(match parse_value(self.ty, s)? {
            Value::Int(i) => Num::I(i),
            Value::Float { v, .. } => Num::F(v),
            Value::Pattern { .. } => unreachable!(),
        })
    }

    fn equal(ty: ScanType, s: &str) -> Result<Self, String> {
        let test = match (parse_value(ty, s)?, ty) {
            (Value::Int(v), ScanType::Num(n)) => Test::Bytes(v.to_le_bytes()[..n.size()].to_vec()),
            (Value::Float { v, tol }, _) => Test::Near { v, tol },
            (Value::Pattern { bytes, mask }, _) if mask.iter().all(|m| *m == 0xFF) => {
                Test::Bytes(bytes)
            }
            (Value::Pattern { bytes, mask }, _) => Test::Masked { bytes, mask },
            (Value::Int(_), _) => unreachable!("integers are only parsed for numbers"),
        };
        Ok(Self { ty, test })
    }

    /// Bytes an exact byte or pattern test compares.
    fn len(&self) -> Option<usize> {
        match &self.test {
            Test::Bytes(bytes) | Test::Masked { bytes, .. } => Some(bytes.len()),
            _ => None,
        }
    }

    fn between(ty: ScanType, lo: &str, hi: &str) -> Result<Self, String> {
        let mut m = Self {
            ty,
            test: Test::Changed,
        };
        m.number()?;
        let (lo, hi) = (m.bound(lo)?, m.bound(hi)?);
        if lo.f() > hi.f() {
            return Err("the lower bound is above the upper one".into());
        }
        m.test = Test::Between(lo, hi);
        Ok(m)
    }

    fn next(ty: ScanType, cond: &Next) -> Result<Self, String> {
        let simple = |test| Self { ty, test };
        let delta = |s: &str, sign: i8| -> Result<Self, String> {
            let m = simple(Test::Changed);
            m.number()?;
            let test = match parse_value(ty, s)? {
                Value::Int(i) => Test::Delta {
                    by: Num::I(sign as i128 * i),
                    tol: 0.0,
                },
                Value::Float { v, tol } => Test::Delta {
                    by: Num::F(sign as f64 * v),
                    tol,
                },
                Value::Pattern { .. } => unreachable!(),
            };
            Ok(simple(test))
        };
        let m = match cond {
            Next::Exact(v) => Self::equal(ty, v)?,
            Next::Between(lo, hi) => Self::between(ty, lo, hi)?,
            Next::Changed => simple(Test::Changed),
            Next::Unchanged => simple(Test::Unchanged),
            Next::Increased => simple(Test::Increased),
            Next::Decreased => simple(Test::Decreased),
            Next::IncreasedBy(v) => delta(v, 1)?,
            Next::DecreasedBy(v) => delta(v, -1)?,
        };
        if matches!(m.test, Test::Increased | Test::Decreased) {
            m.number()?;
        }
        Ok(m)
    }

    fn matches(&self, cur: &[u8], prev: &[u8]) -> bool {
        let num = |b: &[u8]| match self.ty {
            ScanType::Num(n) => n.read(b),
            _ => Num::I(0),
        };
        match &self.test {
            Test::Bytes(bytes) => cur == bytes.as_slice(),
            Test::Masked { bytes, mask } => cur
                .iter()
                .zip(bytes)
                .zip(mask)
                .all(|((c, b), m)| c & m == *b),
            Test::Near { v, tol } => (num(cur).f() - v).abs() <= *tol,
            Test::Between(lo, hi) => {
                let c = num(cur);
                match (c, lo, hi) {
                    (Num::I(c), Num::I(lo), Num::I(hi)) => c >= *lo && c <= *hi,
                    _ => c.f() >= lo.f() && c.f() <= hi.f(),
                }
            }
            Test::Changed => cur != prev,
            Test::Unchanged => cur == prev,
            // Integers compare as integers: i128 to f64 is slow at this scale.
            Test::Increased => match (num(cur), num(prev)) {
                (Num::I(c), Num::I(p)) => c > p,
                (c, p) => c.f() > p.f(),
            },
            Test::Decreased => match (num(cur), num(prev)) {
                (Num::I(c), Num::I(p)) => c < p,
                (c, p) => c.f() < p.f(),
            },
            Test::Delta { by, tol } => match (num(cur), num(prev), by) {
                (Num::I(c), Num::I(p), Num::I(by)) => c - p == *by,
                (c, p, by) => (c.f() - p.f() - by.f()).abs() <= *tol,
            },
        }
    }
}

/// A snapshot of one region, with a bit per aligned slot that still matches.
struct Block {
    base: u64,
    /// Offset of the first aligned slot, and the distance between slots.
    first: usize,
    align: usize,
    bytes: Vec<u8>,
    alive: Vec<u64>,
}

impl Block {
    fn offset(&self, slot: usize) -> usize {
        self.first + slot * self.align
    }

    /// Slots still alive, in order. Skips 64 dead slots at a time.
    fn alive(&self) -> impl Iterator<Item = usize> + '_ {
        self.alive.iter().enumerate().flat_map(|(w, &bits)| {
            let mut bits = bits;
            std::iter::from_fn(move || {
                (bits != 0).then(|| {
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    w * 64 + bit
                })
            })
        })
    }

    fn count(&self) -> u64 {
        self.alive.iter().map(|w| w.count_ones() as u64).sum()
    }
}

enum Candidates {
    /// Snapshots of whole regions, after an unknown-value scan and until
    /// few enough addresses match to list them.
    Dense(Vec<Block>),
    /// Matching addresses (sorted) and their values at the last scan.
    Listed {
        addresses: Vec<u64>,
        values: Vec<u8>,
    },
}

/// One result: an address and its value at the last scan.
pub struct Hit {
    pub address: u64,
    pub value: Vec<u8>,
}

/// A scan in progress.
pub struct Scan {
    pub ty: ScanType,
    /// Bytes compared at each address.
    pub size: usize,
    pub align: u64,
    candidates: Candidates,
}

/// A copy of `r`, with a flag per page telling whether it could be read.
fn snapshot(src: &dyn MemorySource, r: Region) -> (Vec<u8>, Vec<bool>) {
    let mut bytes = vec![0u8; r.size as usize];
    let mut readable = vec![false; bytes.len().div_ceil(PAGE as usize)];
    for_each_block(src, &[r], 0, |at, data, len| {
        let off = (at - r.base) as usize;
        bytes[off..off + len].copy_from_slice(&data[..len]);
        readable[off / PAGE as usize..(off + len).div_ceil(PAGE as usize)].fill(true);
    });
    (bytes, readable)
}

/// Where the aligned slots of `len` bytes at `base` that hold a whole `size`
/// byte value start, and how many there are.
fn slot_span(base: u64, len: usize, size: usize, align: u64) -> (usize, usize) {
    let first = (base.div_ceil(align) * align - base) as usize;
    let n = match len.checked_sub(first + size) {
        Some(room) => room / align as usize + 1,
        None => 0,
    };
    (first, n)
}

impl Scan {
    /// First scan over `regions`. `align` defaults to the value's size (at
    /// most 4) for numbers and 1 for text and bytes.
    pub fn first(
        src: &dyn MemorySource,
        regions: &[Region],
        ty: ScanType,
        cond: &First,
        align: Option<u64>,
    ) -> Result<Self, String> {
        let matcher = match cond {
            First::Exact(v) => Some(Matcher::equal(ty, v)?),
            First::Between(lo, hi) => Some(Matcher::between(ty, lo, hi)?),
            First::Unknown => None,
        };
        let size = match (ty, matcher.as_ref().and_then(Matcher::len)) {
            (ScanType::Num(n), _) => n.size(),
            (_, Some(len)) => len,
            _ => return Err(format!("{} scans need a value", ty.name())),
        };
        let align = align.unwrap_or(match ty {
            ScanType::Num(n) => n.size().min(4) as u64,
            _ => 1,
        });
        if align == 0 {
            return Err("alignment must be at least 1".into());
        }
        let candidates = match matcher {
            None => {
                let total: u64 = regions.iter().map(|r| r.size).sum();
                if total > MAX_SNAPSHOT {
                    return Err(format!(
                        "an unknown-value scan would copy {} MiB (limit {} MiB); limit it with a module or range",
                        total >> 20,
                        MAX_SNAPSHOT >> 20
                    ));
                }
                let mut blocks = Vec::new();
                for r in regions {
                    let (bytes, readable) = snapshot(src, *r);
                    let (first, n) = slot_span(r.base, bytes.len(), size, align);
                    let mut block = Block {
                        base: r.base,
                        first,
                        align: align as usize,
                        bytes,
                        alive: vec![0; n.div_ceil(64)],
                    };
                    for i in 0..n {
                        let s = block.offset(i);
                        let pages = s / PAGE as usize..=(s + size - 1) / PAGE as usize;
                        if readable[pages].iter().all(|r| *r) {
                            block.alive[i / 64] |= 1 << (i % 64);
                        }
                    }
                    blocks.push(block);
                }
                Candidates::Dense(blocks)
            }
            Some(m) => {
                let mut addresses = Vec::new();
                let mut values = Vec::new();
                let mut overflow = false;
                for_each_block(src, regions, size as u64 - 1, |at, data, len| {
                    if overflow {
                        return;
                    }
                    let (first, n) = slot_span(at, len, 1, align);
                    for s in (0..n).map(|i| first + i * align as usize) {
                        let Some(cur) = data.get(s..s + size) else {
                            break;
                        };
                        if m.matches(cur, cur) {
                            addresses.push(at + s as u64);
                            values.extend_from_slice(cur);
                        }
                    }
                    overflow = addresses.len() as u64 > MAX_LISTED;
                });
                if overflow {
                    return Err(format!(
                        "more than {MAX_LISTED} addresses match; use a rarer value, a module or a range"
                    ));
                }
                Candidates::Listed { addresses, values }
            }
        };
        Ok(Self {
            ty,
            size,
            align,
            candidates,
        })
    }

    /// Re-reads the candidates and keeps those that match `cond`.
    pub fn next(&mut self, src: &dyn MemorySource, cond: &Next) -> Result<(), String> {
        let m = Matcher::next(self.ty, cond)?;
        if let Some(len) = m.len() {
            if len != self.size {
                return Err(format!(
                    "the value must be {} bytes like the first scan",
                    self.size
                ));
            }
        }
        let size = self.size;
        match &mut self.candidates {
            Candidates::Dense(blocks) => {
                for b in blocks.iter_mut() {
                    let region = Region {
                        base: b.base,
                        size: b.bytes.len() as u64,
                    };
                    let (cur, readable) = snapshot(src, region);
                    let mut alive = vec![0u64; b.alive.len()];
                    for i in b.alive() {
                        let s = b.offset(i);
                        let pages = s / PAGE as usize..=(s + size - 1) / PAGE as usize;
                        if readable[pages].iter().all(|r| *r)
                            && m.matches(&cur[s..s + size], &b.bytes[s..s + size])
                        {
                            alive[i / 64] |= 1 << (i % 64);
                        }
                    }
                    b.alive = alive;
                    b.bytes = cur;
                }
                blocks.retain(|b| b.count() > 0);
                if self.count() <= MAX_LISTED {
                    self.list();
                }
            }
            Candidates::Listed { addresses, values } => {
                let mut kept = Vec::new();
                let mut kept_values = Vec::new();
                let mut buf = Vec::new();
                let mut i = 0;
                while i < addresses.len() {
                    // Read the candidates of one 64 KiB span together.
                    let start = addresses[i];
                    let mut j = i + 1;
                    while j < addresses.len() && addresses[j] + size as u64 - start <= CHUNK {
                        j += 1;
                    }
                    buf.resize((addresses[j - 1] + size as u64 - start) as usize, 0);
                    let whole = src.read(start, &mut buf).is_ok();
                    for k in i..j {
                        let a = addresses[k];
                        let off = (a - start) as usize;
                        // Part of the span is gone: read what is left one by one.
                        let one;
                        let cur = if whole {
                            &buf[off..off + size]
                        } else {
                            match src.read_vec(a, size) {
                                Some(v) => {
                                    one = v;
                                    &one[..]
                                }
                                None => continue,
                            }
                        };
                        if m.matches(cur, &values[k * size..(k + 1) * size]) {
                            kept.push(a);
                            kept_values.extend_from_slice(cur);
                        }
                    }
                    i = j;
                }
                *addresses = kept;
                *values = kept_values;
            }
        }
        Ok(())
    }

    /// Turns snapshots into a list of the addresses still alive.
    fn list(&mut self) {
        let Candidates::Dense(blocks) = &self.candidates else {
            return;
        };
        let mut addresses = Vec::new();
        let mut values = Vec::new();
        for b in blocks {
            for s in b.alive().map(|i| b.offset(i)) {
                addresses.push(b.base + s as u64);
                values.extend_from_slice(&b.bytes[s..s + self.size]);
            }
        }
        self.candidates = Candidates::Listed { addresses, values };
    }

    pub fn count(&self) -> u64 {
        match &self.candidates {
            Candidates::Dense(blocks) => blocks.iter().map(Block::count).sum(),
            Candidates::Listed { addresses, .. } => addresses.len() as u64,
        }
    }

    /// Up to `limit` results from the `offset`th, in address order.
    pub fn hits(&self, offset: u64, limit: usize) -> Vec<Hit> {
        match &self.candidates {
            Candidates::Listed { addresses, values } => addresses
                .iter()
                .enumerate()
                .skip(offset as usize)
                .take(limit)
                .map(|(k, a)| Hit {
                    address: *a,
                    value: values[k * self.size..(k + 1) * self.size].to_vec(),
                })
                .collect(),
            Candidates::Dense(blocks) => {
                let mut skip = offset;
                let mut out = Vec::new();
                for b in blocks {
                    let n = b.count();
                    if skip >= n {
                        skip -= n;
                        continue;
                    }
                    for s in b.alive().skip(skip as usize).map(|i| b.offset(i)) {
                        if out.len() == limit {
                            return out;
                        }
                        out.push(Hit {
                            address: b.base + s as u64,
                            value: b.bytes[s..s + self.size].to_vec(),
                        });
                    }
                    skip = 0;
                }
                out
            }
        }
    }

    /// A value of this scan's type as text.
    pub fn format(&self, bytes: &[u8]) -> String {
        match self.ty {
            ScanType::Num(n) => match n.read(bytes) {
                Num::I(i) => i.to_string(),
                Num::F(f) => fmt_float(f),
            },
            ScanType::Text => format!("\"{}\"", String::from_utf8_lossy(bytes).escape_debug()),
            ScanType::Bytes => bytes
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoSource;

    const SPOT: u64 = 0x1F3_A8F8_0000;

    fn heap() -> Vec<Region> {
        vec![Region {
            base: 0x1F3_A8C0_0000,
            size: 0x40_0000,
        }]
    }

    /// A page of the demo heap that nothing else uses.
    fn spot() -> Vec<Region> {
        vec![Region {
            base: SPOT,
            size: 0x1000,
        }]
    }

    /// A demo source whose `spot()` page holds nothing but zeros.
    fn source() -> DemoSource {
        let src = DemoSource::new();
        src.poke(SPOT, &[0; 0x1000]);
        src
    }

    fn addresses(scan: &Scan) -> Vec<u64> {
        scan.hits(0, 100).iter().map(|h| h.address).collect()
    }

    #[test]
    fn probing_finds_readable_spans() {
        let src = DemoSource::new();
        let found = probe(&src, 0x1F3_A8B0_0000, 0x1F3_A910_0000);
        assert_eq!(found, heap());
        assert_eq!(probe(&src, 0x1000, 0x2_0000), Vec::<Region>::new());
    }

    #[test]
    fn crawling_reaches_the_heap_from_the_modules() {
        let src = DemoSource::new();
        let found = discover(&src, 8, None, true);
        let heap = heap()[0];
        assert!(
            found
                .iter()
                .any(|r| r.base <= heap.base && r.end() >= heap.end()),
            "{found:X?}"
        );
    }

    #[test]
    fn exact_then_next_scans_narrow_down() {
        let src = DemoSource::new();
        src.poke(SPOT, &0x5EED_1234u32.to_le_bytes());
        src.poke(SPOT + 0x40, &0x5EED_1234u32.to_le_bytes());
        let ty = ScanType::parse("u32").unwrap();
        let mut scan =
            Scan::first(&src, &heap(), ty, &First::Exact("0x5EED1234".into()), None).unwrap();
        assert_eq!(addresses(&scan), vec![SPOT, SPOT + 0x40]);

        src.poke(SPOT, &0x5EED_1239u32.to_le_bytes());
        scan.next(&src, &Next::IncreasedBy("5".into())).unwrap();
        assert_eq!(addresses(&scan), vec![SPOT]);
        assert_eq!(
            scan.format(&scan.hits(0, 1)[0].value),
            0x5EED_1239.to_string()
        );
        scan.next(&src, &Next::Unchanged).unwrap();
        assert_eq!(scan.count(), 1);
        scan.next(&src, &Next::Exact("7".into())).unwrap();
        assert_eq!(scan.count(), 0);
    }

    #[test]
    fn unknown_value_scans_compare_snapshots() {
        let src = DemoSource::new();
        let region = [Region {
            base: SPOT,
            size: 0x2000,
        }];
        let ty = ScanType::parse("f32").unwrap();
        let mut scan = Scan::first(&src, &region, ty, &First::Unknown, None).unwrap();
        assert_eq!(scan.count(), 0x2000 / 4);
        let page: Vec<u64> = scan.hits(3, 2).iter().map(|h| h.address).collect();
        assert_eq!(page, vec![SPOT + 12, SPOT + 16]);
        src.poke(SPOT + 0x1FFC, &75.5f32.to_le_bytes());
        scan.next(&src, &Next::Changed).unwrap();
        assert_eq!(addresses(&scan), vec![SPOT + 0x1FFC]);
        src.poke(SPOT + 0x1FFC, &70.0f32.to_le_bytes());
        scan.next(&src, &Next::Decreased).unwrap();
        scan.next(&src, &Next::Exact("70".into())).unwrap();
        assert_eq!(addresses(&scan), vec![SPOT + 0x1FFC]);
    }

    #[test]
    fn floats_match_to_the_digits_typed() {
        let src = source();
        src.poke(
            SPOT,
            &[99.7f32.to_le_bytes(), 1.25f32.to_le_bytes()].concat(),
        );
        let ty = ScanType::parse("float").unwrap();
        let find = |v: &str| {
            addresses(&Scan::first(&src, &spot(), ty, &First::Exact(v.into()), None).unwrap())
        };
        assert_eq!(find("100"), vec![SPOT]);
        assert_eq!(find("99.7"), vec![SPOT]);
        assert_eq!(find("99.8"), Vec::<u64>::new());
        assert_eq!(find("1.25"), vec![SPOT + 4]);
        let between = Scan::first(
            &src,
            &spot(),
            ty,
            &First::Between("1".into(), "2".into()),
            None,
        );
        assert_eq!(addresses(&between.unwrap()), vec![SPOT + 4]);
    }

    #[test]
    fn text_and_byte_patterns() {
        let src = DemoSource::new();
        src.poke(SPOT + 3, b"xyzzy-marker");
        let text = Scan::first(
            &src,
            &heap(),
            ScanType::Text,
            &First::Exact("zzy-mark".into()),
            None,
        );
        assert_eq!(addresses(&text.unwrap()), vec![SPOT + 5]);
        let bytes = Scan::first(
            &src,
            &heap(),
            ScanType::Bytes,
            &First::Exact("79 ?? 7A 79 2D".into()),
            None,
        );
        assert_eq!(addresses(&bytes.unwrap()), vec![SPOT + 4]);
        let mut scan = Scan::first(
            &src,
            &heap(),
            ScanType::Text,
            &First::Exact("marker".into()),
            None,
        )
        .unwrap();
        assert!(scan.next(&src, &Next::Increased).is_err());
        assert!(
            scan.next(&src, &Next::Exact("other".into())).is_err(),
            "length differs"
        );
        scan.next(&src, &Next::Unchanged).unwrap();
        assert_eq!(scan.count(), 1);
    }

    #[test]
    fn values_are_checked_against_the_type() {
        let src = source();
        let first = |ty: &str, cond: First| {
            Scan::first(&src, &spot(), ScanType::parse(ty).unwrap(), &cond, None)
        };
        assert!(first("u8", First::Exact("256".into())).is_err());
        assert!(first("u32", First::Exact("-1".into())).is_err());
        assert!(first("i32", First::Exact("1.5".into())).is_err());
        assert!(first("i32", First::Between("5".into(), "1".into())).is_err());
        assert!(first("text", First::Unknown).is_err());
        assert!(first("bytes", First::Exact("?? ??".into())).is_err());
        assert!(ScanType::parse("i128").is_err());
        // i8 -1 is also found as its unsigned spelling.
        src.poke(SPOT, &[0xFF]);
        let scan = first("i8", First::Exact("0xFF".into())).unwrap();
        assert_eq!(addresses(&scan), vec![SPOT]);
    }

    #[test]
    fn read_only_image_parts_come_from_the_section_table() {
        let src = DemoSource::new();
        let m = src.modules()[0].clone();
        src.poke(m.base, b"MZ");
        src.poke(m.base + 0x3C, &0x80u32.to_le_bytes());
        src.poke(m.base + 0x80, b"PE\0\0");
        src.poke(m.base + 0x86, &2u16.to_le_bytes());
        src.poke(m.base + 0x94, &0xF0u16.to_le_bytes());
        let section = |rva: u32, size: u32, flags: u32| {
            let mut s = vec![0u8; 0x28];
            s[8..12].copy_from_slice(&size.to_le_bytes());
            s[0xC..0x10].copy_from_slice(&rva.to_le_bytes());
            s[0x24..0x28].copy_from_slice(&flags.to_le_bytes());
            s
        };
        src.poke(m.base + 0x188, &section(0x1000, 0x1800, 0x6000_0020));
        src.poke(m.base + 0x1B0, &section(0x3000, 0x100, 0xC000_0040));
        assert_eq!(
            image_read_only(&src, &m),
            Some(vec![Region {
                base: m.base,
                size: 0x3000
            }])
        );
        let regions = discover(&src, 8, Some((m.base, m.base + 0x4000)), false);
        assert_eq!(
            regions,
            vec![Region {
                base: m.base + 0x3000,
                size: 0x1000
            }]
        );
    }
}
