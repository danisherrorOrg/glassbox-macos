//! The two-level status envelope every Tauri command response carries.
//! See `docs/OBSERVATION_CONTRACT.md`, "How this surfaces in the Tauri
//! command/event contract."

use serde::{Deserialize, Serialize};

use super::status::ObservationStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub status: ObservationStatus,
    pub data: Option<T>,
}

impl<T> Envelope<T> {
    pub fn ok(status: ObservationStatus, data: T) -> Self {
        Self {
            status,
            data: Some(data),
        }
    }

    pub fn denied(status: ObservationStatus) -> Self {
        Self { status, data: None }
    }
}
