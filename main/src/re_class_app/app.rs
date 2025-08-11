use std::sync::Arc;

use handle::AppHandle;
use serde::{
    Deserialize,
    Serialize,
};
use vtd_libum::{
    protocol::types::{
        DirectoryTableType,
        ProcessId,
        ProcessInfo,
        ProcessModuleInfo,
    },
    DriverInterface,
};

use crate::memory::MemoryStructure;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppSignature {
    pub name: String,
    pub module: String,
    pub pattern: String,
    pub offset: u64,
    pub is_relative: bool,
    pub rel_inst_len: u64,
    #[serde(skip)]
    pub offset_buf: String,
    #[serde(skip)]
    pub rel_inst_len_buf: String,
    #[serde(skip)]
    pub last_value: Option<u64>,
    #[serde(skip)]
    pub last_error: Option<String>,
}

pub struct ProcessState {
    pub processes: Vec<ProcessInfo>,
    pub modules: Vec<ProcessModuleInfo>,
    pub selected_process: Option<ProcessInfo>,
}

impl ProcessState {
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            modules: Vec::new(),
            selected_process: None,
        }
    }
}

pub struct ReClassApp {
    pub ke_interface: Arc<DriverInterface>,
    pub handle: Option<Arc<AppHandle>>,
    pub process_state: ProcessState,
    pub memory_structure: Option<MemoryStructure>,
    pub signatures: Vec<AppSignature>,
}

impl ReClassApp {
    pub fn new() -> anyhow::Result<Self> {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();

        let ke_interface = Arc::new(DriverInterface::create_from_env()?);

        Ok(Self {
            ke_interface,
            handle: None,
            process_state: ProcessState::new(),
            memory_structure: None,
            signatures: Vec::new(),
        })
    }

    pub fn fetch_processes(&mut self) -> anyhow::Result<()> {
        self.process_state.processes = self.ke_interface.list_processes()?;
        Ok(())
    }

    pub fn create_handle(&mut self, process_id: ProcessId) -> anyhow::Result<()> {
        self.handle = Some(AppHandle::create(self.ke_interface.clone(), process_id)?);
        Ok(())
    }

    pub fn fetch_modules(&mut self, process_id: ProcessId) -> anyhow::Result<()> {
        self.process_state.modules = self
            .ke_interface
            .list_modules(process_id, DirectoryTableType::Default)?;
        Ok(())
    }

    pub fn get_processes(&self) -> &Vec<ProcessInfo> {
        &self.process_state.processes
    }

    pub fn get_modules(&self) -> &Vec<ProcessModuleInfo> {
        &self.process_state.modules
    }

    pub fn select_process(&mut self, process: ProcessInfo) {
        self.process_state.selected_process = Some(process);
    }

    pub fn get_process_by_id(&self, process_id: ProcessId) -> Option<&ProcessInfo> {
        self.process_state
            .processes
            .iter()
            .find(|p| p.process_id == process_id)
    }

    pub fn set_memory_structure(&mut self, memory_structure: MemoryStructure) {
        self.memory_structure = Some(memory_structure);
    }

    pub fn get_memory_structure(&self) -> Option<&MemoryStructure> {
        self.memory_structure.as_ref()
    }

    pub fn get_memory_structure_mut(&mut self) -> Option<&mut MemoryStructure> {
        self.memory_structure.as_mut()
    }

    pub fn get_signatures_mut(&mut self) -> &mut Vec<AppSignature> {
        &mut self.signatures
    }

    pub fn resolve_signature_by_name(&self, name: &str) -> Option<u64> {
        let sig = self
            .signatures
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))?;
        let handle = self.handle.as_ref()?;
        // Validate pattern first to avoid panic inside constructors
        let sanitized = sig.pattern.split_whitespace().collect::<Vec<_>>().join(" ");
        handle::ByteSequencePattern::parse(&sanitized)?;
        let sig_def = if sig.is_relative {
            handle::Signature::relative_address(&sig.name, &sanitized, sig.offset, sig.rel_inst_len)
        } else {
            handle::Signature::offset(&sig.name, &sanitized, sig.offset)
        };
        handle.resolve_signature(&sig.module, &sig_def).ok()
    }
}

impl Default for ReClassApp {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ReClassApp")
    }
}
