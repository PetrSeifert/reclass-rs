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
        project
            .memory
            .validate_embedded_definitions()
            .context("invalid project definitions")?;
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
        self.memory
            .validate_embedded_definitions()
            .context("invalid project definitions")?;
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
    use crate::memory::{
        definitions::{
            ClassDefinition,
            FieldDefinition,
        },
        types::{
            FieldType,
            PointerTarget,
        },
    };

    fn embedded_project(edges: &[&[usize]]) -> ProjectFile {
        let mut classes: Vec<_> = (0..edges.len())
            .map(|i| ClassDefinition::new(format!("Class{i}")))
            .collect();
        for (owner, targets) in edges.iter().enumerate() {
            for &target in targets.iter() {
                let mut field = FieldDefinition::new(None, FieldType::ClassInstance, 0);
                field.class_id = Some(classes[target].id);
                classes[owner].add_field(field);
            }
        }
        let mut memory = MemoryStructure::new("root".into(), 0, classes[0].clone());
        for class in classes {
            memory.class_registry.register(class);
        }
        ProjectFile {
            memory,
            signatures: vec![],
            web: None,
        }
    }

    #[test]
    fn rejects_self_embedding_on_load() {
        let project = embedded_project(&[&[0]]);
        // Serialize directly: saving rebuilds instances too.
        let json = serde_json::to_string(&project).unwrap();
        let error = ProjectFile::from_json(&json).unwrap_err();
        assert!(format!("{error:#}").contains("embedding cycle"));
    }

    #[test]
    fn rejects_indirect_and_disconnected_embedding_cycles() {
        for edges in [vec![&[1][..], &[0][..]], vec![&[][..], &[2][..], &[1][..]]] {
            let project = embedded_project(&edges);
            let json = serde_json::to_string(&project).unwrap();
            let error = ProjectFile::from_json(&json).unwrap_err();
            assert!(format!("{error:#}").contains("embedding cycle"));
        }
    }

    #[test]
    fn rejects_embedding_cycles_on_save() {
        let mut project = embedded_project(&[&[0]]);
        let error = project.to_json().unwrap_err();
        assert!(format!("{error:#}").contains("embedding cycle"));
        assert!(project.memory.root_class.fields[0]
            .nested_instance
            .is_none());
    }

    #[test]
    fn loads_shared_embedded_classes_and_pointer_cycles() {
        let mut project = embedded_project(&[&[1, 2], &[3], &[3], &[]]);
        let root_id = project.memory.root_class.class_id;
        let mut leaf = project
            .memory
            .class_registry
            .get_class_ids()
            .into_iter()
            .find_map(|id| {
                project
                    .memory
                    .class_registry
                    .get(id)
                    .filter(|class| class.name == "Class3")
                    .cloned()
            })
            .unwrap();
        for field_type in [FieldType::Pointer, FieldType::EncryptedPointer] {
            let mut field = FieldDefinition::new(None, field_type, 0);
            field.pointer_target = Some(PointerTarget::ClassId(root_id));
            leaf.add_field(field);
        }
        project.memory.class_registry.register(leaf);
        let json = serde_json::to_string(&project).unwrap();
        let loaded = ProjectFile::from_json(&json).unwrap();
        for field in &loaded.memory.root_class.fields {
            let child = field.nested_instance.as_ref().unwrap();
            let leaf = child.fields[0].nested_instance.as_ref().unwrap();
            assert_eq!(leaf.total_size, 16);
            assert!(leaf
                .fields
                .iter()
                .all(|field| field.nested_instance.is_none()));
        }
    }

    #[test]
    fn bounds_embedded_definition_depth() {
        for count in [64, 65, 10_000] {
            let edges: Vec<Vec<usize>> = (0..count)
                .map(|i| if i + 1 < count { vec![i + 1] } else { vec![] })
                .collect();
            let project = embedded_project(&edges.iter().map(Vec::as_slice).collect::<Vec<_>>());
            let json = serde_json::to_string(&project).unwrap();
            let result = ProjectFile::from_json(&json);
            if count == 64 {
                result.unwrap();
            } else {
                assert!(format!("{:#}", result.unwrap_err())
                    .contains("embedded class depth exceeds 64"));
            }
        }
    }

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
