//! Optional, explicitly elevated RC003 three-button input. Voice never depends on it.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rc003InputStatus {
    pub phase: String,
    pub last_error: Option<String>,
    pub generation: u64,
    pub report_count: u64,
    pub edge_count: u64,
}

impl Rc003InputStatus {
    pub fn stopped() -> Self {
        Self {
            phase: "stopped".into(),
            ..Self::default()
        }
    }
}

#[cfg(windows)]
mod windows_runtime;
#[cfg(windows)]
pub(crate) use windows_runtime::Rc003InputRuntime;
