//! The project file (`memory_structure.json`), shared with the egui app.

use std::path::Path;

use anyhow::Context;
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    memory::MemoryStructure,
    signature::SignatureDef,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectFile {
    pub memory: MemoryStructure,
    #[serde(default)]
    pub signatures: Vec<SignatureDef>,
    /// Frontend-specific state (e.g. the web canvas). The egui app ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<serde_json::Value>,
}

impl ProjectFile {
    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let mut project: Self = serde_json::from_str(text).context("invalid project file")?;
        // New definitions must not reuse ids from the file.
        project.memory.class_registry.reseed_id_counters();
        project.memory.enum_registry.reseed_id_counters();
        project.memory.create_nested_instances();
        Ok(project)
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_json(&text)
    }

    pub fn to_json(&mut self) -> anyhow::Result<String> {
        // Keep the egui app's instance tree in sync with the definitions.
        self.memory.rebuild_root_from_registry();
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        let text = self.to_json()?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_loads_the_repo_sample() {
        let (memory, signatures) = crate::demo::demo_project();
        let mut p = ProjectFile {
            memory,
            signatures,
            web: Some(serde_json::json!({ "rootExpr": "[$GWorld]" })),
        };
        let back = ProjectFile::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back.signatures, p.signatures);
        assert_eq!(back.web, p.web);
        assert_eq!(back.memory.class_registry.get_class_ids().len(), 6);

        let sample = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../memory_structure.json"
        ))
        .unwrap();
        ProjectFile::from_json(&sample).unwrap();
    }
}
