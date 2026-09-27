//! Memory source backed by the vtd-libum kernel driver interface.

use std::sync::{
    Arc,
    Mutex,
};

use anyhow::Context;
use handle::AppHandle;
use vtd_libum::{
    protocol::types::ProcessInfo,
    DriverInterface,
};

use crate::{
    decrypt::Decryptor,
    signature::SignatureDef,
    source::{
        MemorySource,
        ModuleEntry,
        ProcessEntry,
        ProcessProvider,
    },
};

pub struct DriverProvider {
    interface: Arc<DriverInterface>,
    /// Module holding the XenuineDecrypt indirection. Defaults to the process image.
    decrypt_module: Option<String>,
}

impl DriverProvider {
    pub fn create(decrypt_module: Option<String>) -> anyhow::Result<Self> {
        Ok(Self {
            interface: Arc::new(DriverInterface::create_from_env()?),
            decrypt_module,
        })
    }
}

fn process_entry(p: &ProcessInfo) -> ProcessEntry {
    ProcessEntry {
        pid: p.process_id,
        name: p.get_image_base_name().unwrap_or("<unknown>").to_string(),
    }
}

impl ProcessProvider for DriverProvider {
    fn list_processes(&self) -> anyhow::Result<Vec<ProcessEntry>> {
        Ok(self
            .interface
            .list_processes()?
            .iter()
            .map(process_entry)
            .collect())
    }

    fn attach(&self, pid: u32) -> anyhow::Result<Arc<dyn MemorySource>> {
        let process = self
            .interface
            .list_processes()?
            .iter()
            .find(|p| p.process_id == pid)
            .map(process_entry)
            .with_context(|| format!("no process with pid {pid}"))?;
        let handle = AppHandle::create(self.interface.clone(), pid)?;
        let decrypt_module = self
            .decrypt_module
            .clone()
            .unwrap_or_else(|| process.name.clone());
        Ok(Arc::new(DriverSource {
            process,
            handle,
            decrypt_module,
            decryptor: Mutex::new(None),
        }))
    }
}

pub struct DriverSource {
    process: ProcessEntry,
    handle: Arc<AppHandle>,
    decrypt_module: String,
    decryptor: Mutex<Option<Decryptor>>,
}

impl MemorySource for DriverSource {
    fn process(&self) -> ProcessEntry {
        self.process.clone()
    }

    fn modules(&self) -> Vec<ModuleEntry> {
        self.handle
            .get_all_modules()
            .iter()
            .map(|m| ModuleEntry {
                name: m.get_base_dll_name().unwrap_or("<unknown>").to_string(),
                base: m.base_address,
                size: m.module_size,
            })
            .collect()
    }

    fn read(&self, address: u64, buffer: &mut [u8]) -> anyhow::Result<()> {
        self.handle.read_slice(address, buffer)
    }

    fn can_write(&self) -> bool {
        self.handle.can_write()
    }

    fn write(&self, address: u64, bytes: &[u8]) -> anyhow::Result<()> {
        anyhow::ensure!(self.can_write(), "the driver cannot write memory");
        self.handle.write_slice(address, bytes)
    }

    fn resolve_signature(&self, sig: &SignatureDef) -> anyhow::Result<u64> {
        let pattern = sig.sanitized_pattern();
        // Validate first: the Signature constructors panic on invalid patterns.
        handle::ByteSequencePattern::parse(&pattern).context("invalid pattern")?;
        let def = if sig.is_relative {
            handle::Signature::relative_address(&sig.name, &pattern, sig.offset, sig.rel_inst_len)
        } else {
            handle::Signature::offset(&sig.name, &pattern, sig.offset)
        };
        self.handle.resolve_signature(&sig.module, &def)
    }

    fn decrypt(&self, value: u64) -> anyhow::Result<u64> {
        let mut guard = self.decryptor.lock().unwrap();
        if guard.is_none() {
            *guard = Some(Decryptor::create(&self.handle, &self.decrypt_module)?);
        }
        // SAFETY: the decryptor mirrors the target's decrypt routine into local executable memory.
        Ok(unsafe { guard.as_ref().unwrap().decrypt(value) })
    }
}
