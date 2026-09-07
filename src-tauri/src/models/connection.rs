//! `NetworkConnection` — Engine-owned domain state. Constructed and owned
//! exclusively by the Observation Engine, by matching `SocketObservation`s
//! across successive snapshots. See `docs/DATA_MODEL.md`'s "Connection-
//! identity matching rule" and "closed vs expired rule" for the algorithm
//! this type's fields are produced by (implemented in `engine/`, not here).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::socket::Protocol;
use super::status::ObservationStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Discovered,
    Active,
    Closed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConnection {
    /// An Engine session identity, not an OS-level socket identity —
    /// assigned the first time an observation is matched, stable across
    /// polls only as long as the Engine's matching heuristic holds.
    pub connection_id: String,
    pub pid: u32,
    pub protocol: Protocol,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: Option<String>,
    pub remote_port: Option<u16>,
    pub state: String,
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
    pub lifecycle_state: LifecycleState,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub status: ObservationStatus,
}
