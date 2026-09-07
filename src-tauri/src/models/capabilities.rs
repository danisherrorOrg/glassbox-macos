//! `ObservationCapabilities` — what a provider can *ever* observe,
//! independent of any one attempt, distinct from `ObservationStatus` (one
//! observation's outcome right now). See `docs/DATA_MODEL.md`.
//!
//! Ownership: Engine-owned, aggregated **per provider** from each
//! provider's own self-report (`ProviderCapabilities`) — never per
//! connection. Each provider only ever sets the fields it's actually
//! responsible for; the Engine picks exactly those fields out when
//! building the aggregate (`ObservationEngine::get_capabilities`), so a
//! provider's default/unset values on fields it doesn't own are never
//! read, not merged in some generic way that could let one provider's
//! silence override another's real answer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitedAvailability {
    Available,
    Limited,
    Unsupported,
}

/// One provider's self-report. Every provider returns this same shape but
/// only ever sets the field(s) it owns — see the module doc comment.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderCapabilities {
    pub process: Option<Availability>,
    pub sockets: Option<Availability>,
    pub dns: Option<Availability>,
    pub remote_addresses: Option<Availability>,
    pub http_metadata: Option<LimitedAvailability>,
    pub https_metadata: Option<LimitedAvailability>,
    pub request_body: Option<LimitedAvailability>,
    pub response_body: Option<LimitedAvailability>,
    pub raw_packet_data: Option<Availability>,
}

/// The Engine-aggregated result — what `get_capabilities()` returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationCapabilities {
    pub process: Availability,
    pub sockets: Availability,
    pub dns: Availability,
    pub remote_addresses: Availability,
    pub http_metadata: LimitedAvailability,
    pub https_metadata: LimitedAvailability,
    pub request_body: LimitedAvailability,
    pub response_body: LimitedAvailability,
    pub raw_packet_data: Availability,
}
