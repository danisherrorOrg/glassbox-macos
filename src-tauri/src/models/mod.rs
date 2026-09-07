//! Provider-owned observation types and Engine-owned domain state.
//! See `docs/DATA_MODEL.md`.

mod connection;
mod envelope;
mod flow;
mod process;
mod socket;
mod status;

pub use connection::{LifecycleState, NetworkConnection};
pub use envelope::Envelope;
#[allow(unused_imports)] // defined now, wired in Phase 0.5 — see flow.rs
pub use flow::Flow;
pub use process::{ProcessInfo, ProcessObservation, ProcessSnapshot, ProcessState};
pub use socket::{Protocol, SocketObservation, SocketSnapshot};
pub use status::{ObservationState, ObservationStatus, Provider, ProviderState, ProviderStatus};
