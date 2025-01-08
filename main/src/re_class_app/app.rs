use std::sync::Arc;

use handle::AppHandle;
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
}

impl Default for ReClassApp {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ReClassApp")
    }
}
