//! Values the user keeps an eye on. Each is re-read every tick, keeps a short
//! history for plotting, and can be frozen: written back every tick, when the
//! driver can write memory.

use std::collections::VecDeque;

use reclass_core::{
    scan::ScanType,
    source::MemorySource,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    json,
    Value,
};

use crate::canvas::hex;

/// Samples kept per watch: a minute at the default 250 ms tick.
pub const HISTORY: usize = 240;

/// What a project file keeps of a watch.
#[derive(Clone, Serialize, Deserialize)]
pub struct Saved {
    pub label: String,
    pub expr: String,
    #[serde(rename = "type")]
    pub ty: String,
}

pub struct Watch {
    pub id: u64,
    pub label: String,
    /// Address expression, evaluated again every tick.
    pub expr: String,
    pub ty: ScanType,
    /// Bytes written back every tick while frozen.
    pub frozen: Option<Vec<u8>>,
    pub address: Result<u64, String>,
    pub value: Option<Vec<u8>>,
    /// Oldest first; `None` where the value could not be read.
    history: VecDeque<Option<f64>>,
}

/// Parses a watch type: numbers only, since watches are plotted.
pub fn parse_type(s: &str) -> Result<ScanType, String> {
    let ty = ScanType::parse(s)?;
    ty.size()
        .map(|_| ty)
        .ok_or_else(|| "watches take a number type, like i32 or f32".into())
}

impl Watch {
    pub fn new(id: u64, label: String, expr: String, ty: ScanType) -> Self {
        Self {
            id,
            label,
            expr,
            ty,
            frozen: None,
            address: Err("not read yet".into()),
            value: None,
            history: VecDeque::new(),
        }
    }

    pub fn size(&self) -> usize {
        self.ty.size().unwrap_or(1)
    }

    /// Reads the value at `address`, first writing the frozen bytes back.
    pub fn sample(&mut self, src: &dyn MemorySource, address: Result<u64, String>) {
        self.value = address.as_ref().ok().and_then(|a| {
            if let Some(bytes) = &self.frozen {
                // A failed write shows as the value not holding.
                let _ = src.write(*a, bytes);
            }
            src.read_vec(*a, self.size())
        });
        self.address = address;
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history
            .push_back(self.value.as_deref().and_then(|b| self.ty.number(b)));
    }

    /// Forgets the samples, e.g. after the watch starts reading something else.
    pub fn reset(&mut self) {
        self.history.clear();
        self.value = None;
        self.address = Err("not read yet".into());
    }

    pub fn saved(&self) -> Saved {
        Saved {
            label: self.label.clone(),
            expr: self.expr.clone(),
            ty: self.ty.name().into(),
        }
    }

    pub fn view(&self) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "expr": self.expr,
            "type": self.ty.name(),
            "address": self.address.as_ref().ok().map(|a| hex(*a)),
            "error": match (&self.address, &self.value) {
                (Err(e), _) => Some(e.clone()),
                (Ok(_), None) => Some("unreadable".to_string()),
                _ => None,
            },
            "value": self.value.as_deref().map(|b| self.ty.format(b)),
            "frozen": self.frozen.as_deref().map(|b| self.ty.format(b)),
            "history": self.history,
        })
    }
}
