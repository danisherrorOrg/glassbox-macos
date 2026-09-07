//! `TrafficEvent` — Engine-owned. See `docs/DATA_MODEL.md`. This phase only
//! ever constructs the three connection-lifecycle variants
//! (`ConnectionOpened`/`ConnectionClosed`/`ConnectionExpired`); `Request`/
//! `Response` are emitted starting Phase 0.3/0.4 once `TrafficProvider`
//! exists.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficEventType {
    ConnectionOpened,
    ConnectionClosed,
    ConnectionExpired,
    Request,
    Response,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficEvent {
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "type")]
    pub event_type: TrafficEventType,
    pub connection_id: Option<String>,
    pub request_id: Option<String>,
    pub response_id: Option<String>,
}
