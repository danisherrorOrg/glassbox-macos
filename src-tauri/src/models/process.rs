//! Process-layer types: `ProcessObservation`/`ProcessSnapshot` (provider-owned)
//! and `ProcessInfo` (Engine-owned). See `docs/DATA_MODEL.md`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::status::{ObservationStatus, ProviderStatus};

/// Exactly what `ProcessProvider` saw for one process in one call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessObservation {
    pub pid: u32,
    pub name: String,
    /// `None` when neither `sysinfo::Process::exe()` nor the `proc_pidpath`
    /// FFI fallback could resolve a path — genuinely happens for `pid 0`
    /// (`kernel_task`), which `sysinfo` does enumerate on macOS. Never
    /// rendered as `""`; that would silently look like a real (empty) path
    /// rather than "unknown" (`docs/OBSERVATION_CONTRACT.md`'s no-inference
    /// rule).
    pub executable_path: Option<String>,
    pub cpu_percent: Option<f32>,
    pub memory_bytes: Option<u64>,
}

/// Immutable, point-in-time output of `ProcessProvider`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub timestamp: DateTime<Utc>,
    pub observations: Vec<ProcessObservation>,
    pub status: ProviderStatus,
}

/// Engine-derived: whether the process is still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Running,
    Exited,
}

/// Built from a `ProcessObservation` plus Engine-derived judgment. A
/// provider never constructs this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// See `ProcessObservation.executable_path` — `None` means genuinely
    /// unknown, not a real empty path.
    pub executable_path: Option<String>,
    pub cpu_percent: Option<f32>,
    pub memory_bytes: Option<u64>,
    pub process_state: ProcessState,
    pub status: ObservationStatus,
    pub active_connection_count: Option<u32>,
}
