//! `HostnameObservation` (provider-owned) / `ResolvedHostname` (Engine-owned)
//! — see `docs/DATA_MODEL.md`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::status::ObservationStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostnameSource {
    ReverseDns,
    Sni,
    HttpHost,
}

impl HostnameSource {
    /// Starter confidence values, per `docs/DATA_MODEL.md`. `ReverseDns`'s
    /// 0.30-if-shared-suffix variant isn't implemented this phase — no
    /// CDN-suffix heuristic exists yet — so reverse DNS is always 0.50 for
    /// now.
    pub fn starter_confidence(self) -> f32 {
        match self {
            HostnameSource::HttpHost => 0.95,
            HostnameSource::Sni => 0.90,
            HostnameSource::ReverseDns => 0.50,
        }
    }
}

/// What `DNSProvider` actually saw for one hostname signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostnameObservation {
    pub queried_addr: String,
    pub source: HostnameSource,
    pub hostname: String,
    pub confidence: f32,
    pub observed_at: DateTime<Utc>,
}

/// Attaches a `HostnameObservation` to a specific connection once the
/// Engine has matched it. A provider never constructs this type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedHostname {
    pub connection_id: String,
    pub source: HostnameSource,
    pub hostname: String,
    pub confidence: f32,
    pub status: ObservationStatus,
}
