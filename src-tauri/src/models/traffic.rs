//! Traffic-layer provider-owned types: `CorrelationEvidence` (the formal
//! input to Engine correlation) and `RawHTTPRequest`/`RawHTTPResponse`
//! (transient, in-memory only — never persisted). See `docs/DATA_MODEL.md`.
//!
//! Defined now (Phase 0.3), ahead of `DATA_MODEL.md`'s table placement
//! under Phase 0.4 — `TrafficProvider`'s trait signature (Phase 0.3) can't
//! exist without these types, since its trait method returns exactly
//! `(RawHTTPRequest, Option<RawHTTPResponse>, CorrelationEvidence)` per
//! `DATA_MODEL.md`. `HTTPRequest`/`HTTPResponse` (Engine-owned, redacted)
//! stay Phase 0.4 work, along with the Redactor and Engine wiring — see
//! `docs/[9] TODO.md`'s Phase 0.3 section for the exact split.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What `TrafficProvider` actually has available when it captures a flow —
/// not a `connection_id`, because the traffic provider has no knowledge of
/// the Engine's internal identity scheme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationEvidence {
    pub pid: Option<u32>,
    pub protocol: Option<String>,
    pub local_addr: Option<String>,
    pub local_port: Option<u16>,
    pub remote_addr: Option<String>,
    pub remote_port: Option<u16>,
    pub hostname: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub source: String,
}

/// What `TrafficProvider` actually captured, after the mitmproxy addon's
/// tier-1 (highly-sensitive) redaction has already run. No Engine-assigned
/// identity of any kind — that doesn't exist yet at capture time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawHTTPRequest {
    pub method: String,
    pub host: String,
    pub path: String,
    /// Tier-1 fields already redacted by the mitmproxy addon; tier-2
    /// fields present raw.
    pub headers: HashMap<String, String>,
    /// The full body, not a truncated preview — truncation to
    /// `body_preview` happens only when Phase 0.4's `Redactor` produces
    /// `HTTPRequest`.
    pub body: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawHTTPResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    /// `None` when the addon couldn't compute a duration (missing
    /// request/response timestamps) — never coerced to `0.0`, which would
    /// be indistinguishable from a real, observed zero-millisecond
    /// response. Same field-level-absence pattern as `cpu_percent`/
    /// `bytes_sent` (`OBSERVATION_CONTRACT.md`).
    pub duration_ms: Option<f64>,
}
