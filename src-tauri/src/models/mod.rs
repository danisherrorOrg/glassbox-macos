//! Provider-owned observation types and Engine-owned domain state.
//! See `docs/DATA_MODEL.md`.

mod connection;
mod envelope;
mod event;
mod flow;
mod hostname;
mod monitoring;
mod process;
mod socket;
mod status;

pub use connection::{LifecycleState, NetworkConnection};
pub use envelope::Envelope;
pub use event::{TrafficEvent, TrafficEventType};
#[allow(unused_imports)] // defined now, wired in Phase 0.5 — see flow.rs
pub use flow::Flow;
pub use hostname::{HostnameObservation, HostnameSource, ResolvedHostname};
pub use monitoring::{MonitoringState, MonitoringStatus};
pub use process::{ProcessInfo, ProcessObservation, ProcessSnapshot, ProcessState};
pub use socket::{Protocol, SocketObservation, SocketSnapshot};
pub use status::{
    polling_stale_threshold, ObservationState, ObservationStatus, Provider, ProviderState,
    ProviderStatus, PHASE_0_1_STALE_THRESHOLD_SECONDS,
};
