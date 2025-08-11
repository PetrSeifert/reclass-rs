#![allow(dead_code)]

use std::{
    error::Error,
    ffi::CStr,
    sync::{
        Arc,
        Weak,
    },
};

use anyhow::Context;
use obfstr::obfstr;
use raw_struct::{
    FromMemoryView,
    MemoryView,
};
use vtd_libum::{
    protocol::{
        command::{
            KeyboardState,
            MouseState,
        },
        types::{
            DirectoryTableType,
            ProcessId,
            ProcessModuleInfo,
        },
    },
    DriverInterface,
};

use crate::{
    SearchPattern,
    Signature,
    SignatureType,
};

struct AppMemoryView {
    handle: Weak<AppHandle>,
}

impl MemoryView for AppMemoryView {
    fn read_memory(
        &self,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let Some(handle) = self.handle.upgrade() else {
            return Err(anyhow::anyhow!("handle gone").into());
        };

        Ok(handle.read_slice(offset, buffer)?)
    }
}

/// Handle to the process
pub struct AppHandle {
    weak_self: Weak<Self>,
    metrics: bool,

    modules: Vec<ProcessModuleInfo>,
    process_id: ProcessId,
    ke_interface: Arc<DriverInterface>,
}

impl AppHandle {
    pub fn create(
        interface: Arc<DriverInterface>,
        process_id: ProcessId,
    ) -> anyhow::Result<Arc<Self>> {
        let modules = interface.list_modules(process_id, DirectoryTableType::Default)?;
        log::debug!(
            "{}. Process id {}",
            obfstr!("Successfully initialized handle"),
            process_id
        );

        let ke_interface = interface;
        let handle = Arc::new_cyclic(|weak| Self {
            weak_self: weak.clone(),
            metrics: false,
            modules,
            process_id,
            ke_interface,
        });

        Ok(handle)
    }

    pub fn get_all_modules(&self) -> &[ProcessModuleInfo] {
        &self.modules
    }

    pub fn get_module_by_name(&self, module_name: &str) -> Option<&ProcessModuleInfo> {
        self.modules.iter().find(|module| {
            module
                .get_base_dll_name()
                .map(|name| name.eq_ignore_ascii_case(module_name))
                .unwrap_or(false)
        })
    }

    pub fn get_module_by_address(&self, address: u64) -> Option<&ProcessModuleInfo> {
        self.modules.iter().find(|module| {
            address >= module.base_address && address < (module.base_address + module.module_size)
        })
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub fn send_keyboard_state(&self, states: &[KeyboardState]) -> anyhow::Result<()> {
        self.ke_interface.send_keyboard_state(states)?;
        Ok(())
    }

    pub fn send_mouse_state(&self, states: &[MouseState]) -> anyhow::Result<()> {
        self.ke_interface.send_mouse_state(states)?;
        Ok(())
    }

    pub fn add_metrics_record(&self, record_type: &str, record_payload: &str) {
        if !self.metrics {
            /* user opted out */
            return;
        }

        let _ = self
            .ke_interface
            .add_metrics_record(record_type, record_payload);
    }

    pub fn module_address(&self, module_name: &str, address: u64) -> Option<u64> {
        let module = self.get_module_by_name(module_name)?;
        if address < module.base_address || address >= (module.base_address + module.module_size) {
            None
        } else {
            Some(address - module.base_address)
        }
    }

    pub fn memory_address(&self, module_name: &str, offset: u64) -> anyhow::Result<u64> {
        Ok(self
            .get_module_by_name(module_name)
            .with_context(|| format!("{} {}", obfstr!("missing module"), module_name))?
            .base_address
            + offset)
    }

    pub fn module_size(&self, module_name: &str) -> anyhow::Result<u64> {
        Ok(self
            .get_module_by_name(module_name)
            .with_context(|| format!("{} {}", obfstr!("missing module"), module_name))?
            .module_size)
    }

    pub fn read_sized<T: Copy>(&self, address: u64) -> anyhow::Result<T> {
        Ok(self
            .ke_interface
            .read(self.process_id, DirectoryTableType::Default, address)?)
    }

    pub fn read_slice<T: Copy>(&self, address: u64, buffer: &mut [T]) -> anyhow::Result<()> {
        Ok(self.ke_interface.read_slice(
            self.process_id,
            DirectoryTableType::Default,
            address,
            buffer,
        )?)
    }

    pub fn read_string(
        &self,
        address: u64,
        expected_length: Option<usize>,
    ) -> anyhow::Result<String> {
        let mut expected_length = expected_length.unwrap_or(8); // Using 8 as we don't know how far we can read
        let mut buffer = vec![0u8; expected_length];

        // FIXME: Do cstring reading within the kernel driver!
        loop {
            if buffer.len() < expected_length {
                buffer.resize(expected_length, 0u8);
            }
            self.read_slice(address, buffer.as_mut_slice())
                .context("read_string")?;

            if let Ok(str) = CStr::from_bytes_until_nul(&buffer) {
                return Ok(str.to_str().context("invalid string contents")?.to_string());
            }

            expected_length += 8;
        }
    }

    pub fn create_memory_view(&self) -> Arc<dyn MemoryView + Send + Sync> {
        Arc::new(AppMemoryView {
            handle: self.weak_self.clone(),
        })
    }

    #[must_use = "The pattern search result should be handled"]
    pub fn find_pattern(
        &self,
        address: u64,
        length: usize,
        pattern: &dyn SearchPattern,
    ) -> anyhow::Result<Option<u64>> {
        if pattern.length() > length {
            return Ok(None);
        }

        let mut buffer = vec![0; length];
        self.ke_interface.read_slice(
            self.process_id,
            DirectoryTableType::Default,
            address,
            &mut buffer,
        )?;

        for (index, window) in buffer.windows(pattern.length()).enumerate() {
            if !pattern.is_matching(window) {
                continue;
            }

            return Ok(Some(address + index as u64));
        }

        Ok(None)
    }

    pub fn resolve_signature(
        &self,
        module_name: &str,
        signature: &Signature,
    ) -> anyhow::Result<u64> {
        log::trace!("Resolving '{}' in {}", signature.debug_name, module_name);
        let module_info = self
            .get_module_by_name(module_name)
            .with_context(|| format!("{} {}", obfstr!("missing module"), module_name))?;

        let inst_offset = self
            .find_pattern(
                module_info.base_address,
                module_info.module_size as usize,
                &*signature.pattern,
            )?
            .with_context(|| {
                format!(
                    "{} {}",
                    obfstr!("failed to find pattern"),
                    signature.debug_name
                )
            })?;

        let value = u32::read_object(&*self.create_memory_view(), inst_offset + signature.offset)
            .map_err(|err| anyhow::anyhow!("{}", err))? as u64;
        let value = match &signature.value_type {
            SignatureType::Offset => value,
            SignatureType::RelativeAddress { inst_length } => inst_offset + value + inst_length,
        };

        match &signature.value_type {
            SignatureType::Offset => log::trace!(
                " => {:X} (inst at {:X})",
                value,
                self.module_address(module_name, inst_offset)
                    .unwrap_or(u64::MAX)
            ),
            SignatureType::RelativeAddress { .. } => log::trace!(
                "  => {:X} ({:X})",
                value,
                self.module_address(module_name, value).unwrap_or(u64::MAX)
            ),
        }

        Ok(value)
    }
}
