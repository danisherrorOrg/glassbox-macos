//! `Flow` — defined now per `docs/DATA_MODEL.md`, not wired into the Engine
//! or UI until Phase 0.5 (`docs/DECISIONS.md` ADR-004). Depends on types
//! (`ResolvedHostname`, `HTTPRequest`, `HTTPResponse`) not yet defined —
//! kept minimal (`String` placeholders) until those phases add them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Flow {
    pub flow_id: String,
    pub connection_id: String,
}
