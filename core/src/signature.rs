use serde::{
    Deserialize,
    Serialize,
};

/// A byte-pattern signature as stored in project files.
///
/// Field names match the egui app's `AppSignature`, so both frontends read and
/// write the same `signatures` array.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureDef {
    pub name: String,
    pub module: String,
    pub pattern: String,
    pub offset: u64,
    pub is_relative: bool,
    pub rel_inst_len: u64,
}

impl SignatureDef {
    /// The pattern with runs of whitespace collapsed, as the pattern parser expects.
    pub fn sanitized_pattern(&self) -> String {
        self.pattern
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}
