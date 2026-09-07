//! Live-monitoring state machine — `docs/[9] TODO.md` Phase 0.2. Distinct
//! from `ObservationStatus`: this describes whether the *polling loop* for
//! a selected process is running, not any single observation's freshness.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitoringState {
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringStatus {
    pub state: MonitoringState,
    pub pid: Option<u32>,
    pub reason: Option<String>,
}
