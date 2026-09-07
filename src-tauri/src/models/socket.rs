//! Socket-layer provider-owned type: `SocketObservation`/`SocketSnapshot`.
//! See `docs/DATA_MODEL.md` — NOT `NetworkConnection` (that's Engine-owned,
//! see `connection.rs`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::status::ProviderStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
}

/// Exactly what `SocketProvider` saw for one socket in one snapshot. No
/// `connection_id`, no `lifecycle_state`, no `first_seen`/`last_seen` —
/// those are Engine-derived by diffing observations across snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketObservation {
    pub pid: u32,
    pub protocol: Protocol,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: Option<String>,
    pub remote_port: Option<u16>,
    /// LISTEN / ESTABLISHED / etc., as reported this instant. UDP sockets
    /// have no TCP-style state; represented as the literal string "UDP".
    pub state: String,
    /// Always `None` on macOS via this provider stack (`sysinfo`/`netstat2`/
    /// `libproc`) — verified unobtainable by the Phase 0 permissions spike
    /// (`docs/PERMISSIONS_AND_PLATFORM.md`, question 4; `PIF-045`). Kept
    /// `Option` per the field-level-absence rule, not because it's ever
    /// populated today.
    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
}

/// Immutable, point-in-time output of `SocketProvider`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketSnapshot {
    pub timestamp: DateTime<Utc>,
    pub observations: Vec<SocketObservation>,
    pub status: ProviderStatus,
}
