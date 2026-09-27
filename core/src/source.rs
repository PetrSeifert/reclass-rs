use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};

use crate::signature::SignatureDef;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEntry {
    pub name: String,
    pub base: u64,
    pub size: u64,
}

/// An attached process whose memory can be read.
pub trait MemorySource: Send + Sync {
    fn process(&self) -> ProcessEntry;
    fn modules(&self) -> Vec<ModuleEntry>;
    fn read(&self, address: u64, buffer: &mut [u8]) -> anyhow::Result<()>;
    fn resolve_signature(&self, signature: &SignatureDef) -> anyhow::Result<u64>;
    /// Decrypts the raw value of an `EncryptedPointer` field.
    fn decrypt(&self, value: u64) -> anyhow::Result<u64>;
    /// Advances simulated process state. Only the demo source does anything here.
    fn tick(&self) {}
}

/// Lists processes and attaches to one of them.
pub trait ProcessProvider: Send + Sync {
    fn list_processes(&self) -> anyhow::Result<Vec<ProcessEntry>>;
    fn attach(&self, pid: u32) -> anyhow::Result<Arc<dyn MemorySource>>;
}

impl dyn MemorySource + '_ {
    pub fn read_vec(&self, address: u64, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read(address, &mut buf).ok().map(|_| buf)
    }

    pub fn read_u64(&self, address: u64) -> Option<u64> {
        let mut buf = [0u8; 8];
        self.read(address, &mut buf).ok()?;
        Some(u64::from_le_bytes(buf))
    }

    pub fn read_u32(&self, address: u64) -> Option<u32> {
        let mut buf = [0u8; 4];
        self.read(address, &mut buf).ok()?;
        Some(u32::from_le_bytes(buf))
    }

    /// Reads a pointer of `size` bytes (4 or 8).
    pub fn read_pointer(&self, address: u64, size: u64) -> Option<u64> {
        match size {
            4 => self.read_u32(address).map(u64::from),
            _ => self.read_u64(address),
        }
    }

    /// Pointer size of the process, from the PE header of its main image:
    /// 4 for a PE32 (32-bit) image, 8 for PE32+. `None` if it cannot be read.
    pub fn detect_pointer_size(&self) -> Option<u64> {
        let image = self.module_by_name(&self.process().name)?;
        let mut mz = [0u8; 2];
        self.read(image.base, &mut mz).ok()?;
        if &mz != b"MZ" {
            return None;
        }
        let nt = image.base + self.read_u32(image.base + 0x3C)? as u64;
        if self.read_u32(nt)? != u32::from_le_bytes(*b"PE\0\0") {
            return None;
        }
        let mut magic = [0u8; 2];
        self.read(nt + 0x18, &mut magic).ok()?;
        match u16::from_le_bytes(magic) {
            0x10B => Some(4),
            0x20B => Some(8),
            _ => None,
        }
    }

    /// Reads a NUL-terminated string of at most `max` bytes. Non-printable bytes become '.'.
    pub fn read_c_string(&self, address: u64, max: usize) -> Option<String> {
        // Read in chunks so a string near the end of a mapping still resolves.
        let mut out = String::new();
        let mut offset = 0;
        while offset < max {
            let chunk = (max - offset).min(32);
            let bytes = self.read_vec(address + offset as u64, chunk)?;
            for b in bytes {
                if b == 0 {
                    return Some(out);
                }
                out.push(if (32..127).contains(&b) {
                    b as char
                } else {
                    '.'
                });
            }
            offset += chunk;
        }
        Some(out)
    }

    pub fn module_by_name(&self, name: &str) -> Option<ModuleEntry> {
        self.modules()
            .into_iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    /// `module+0x1234` for addresses inside a module.
    pub fn symbolize(&self, address: u64) -> Option<String> {
        self.modules()
            .into_iter()
            .find(|m| address >= m.base && address < m.base + m.size)
            .map(|m| format!("{}+0x{:X}", m.name, address - m.base))
    }
}
