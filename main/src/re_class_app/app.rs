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
use crate::memory::{MemoryStructure, MemoryStructureBuilder};

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

    pub fn get_processes_snapshot(&self) -> Vec<(ProcessId, String)> {
        self.process_state
            .processes
            .iter()
            .map(|p| {
                (
                    p.process_id,
                    p.get_image_base_name().unwrap_or("Unknown").to_string(),
                )
            })
            .collect()
    }

    /// Set the memory structure
    pub fn set_memory_structure(&mut self, memory_structure: MemoryStructure) {
        self.memory_structure = Some(memory_structure);
    }

    /// Get the current memory structure
    pub fn get_memory_structure(&self) -> Option<&MemoryStructure> {
        self.memory_structure.as_ref()
    }

    /// Get a mutable reference to the memory structure
    pub fn get_memory_structure_mut(&mut self) -> Option<&mut MemoryStructure> {
        self.memory_structure.as_mut()
    }

    /// Clear the memory structure
    pub fn clear_memory_structure(&mut self) {
        self.memory_structure = None;
    }

    /// Create a memory structure from a class definition
    pub fn create_memory_structure_from_class(&mut self, class_name: &str, address: u64) -> bool {
        if let Some(class_def) = self.memory_structure.as_ref().and_then(|ms| ms.get_class_definition(class_name)) {
            let mut builder = MemoryStructureBuilder::new();
            builder.register_class(class_def.clone());
            
            if let Some(mut new_structure) = builder.build(format!("{}_instance", class_name), address, class_name) {
                // Create nested instances for the new structure
                new_structure.create_nested_instances();
                self.memory_structure = Some(new_structure);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl Default for ReClassApp {
    fn default() -> Self {
        Self::new().expect("Failed to initialize ReClassApp")
    }
}
