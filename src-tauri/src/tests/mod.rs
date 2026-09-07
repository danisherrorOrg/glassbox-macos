//! Shared mock provider implementations, returning scripted fixture
//! `ProcessSnapshot`/`SocketSnapshot` values, per `docs/TESTING_STRATEGY.md`.
//! Used by `engine`'s unit tests to exercise lifecycle/correlation logic
//! without any real OS calls.

use std::sync::Mutex;

use crate::models::{HostnameObservation, ProcessSnapshot, ProviderStatus, SocketSnapshot};
use crate::providers::{CapturedFlow, DNSProvider, ProcessProvider, SocketProvider, TrafficProvider};

pub struct MockProcessProvider {
    snapshots: Mutex<Vec<ProcessSnapshot>>,
}

impl MockProcessProvider {
    /// `snapshots` are returned in order, oldest first; the last one is
    /// returned repeatedly once exhausted.
    pub fn new(snapshots: Vec<ProcessSnapshot>) -> Self {
        Self {
            snapshots: Mutex::new(snapshots),
        }
    }
}

impl ProcessProvider for MockProcessProvider {
    fn snapshot(&self) -> ProcessSnapshot {
        let mut snapshots = self.snapshots.lock().unwrap();
        if snapshots.len() > 1 {
            snapshots.remove(0)
        } else {
            snapshots
                .first()
                .cloned()
                .expect("MockProcessProvider needs at least one snapshot")
        }
    }
}

pub struct MockSocketProvider {
    snapshots: Mutex<Vec<SocketSnapshot>>,
    denied_pids: Vec<u32>,
}

impl MockSocketProvider {
    pub fn new(snapshots: Vec<SocketSnapshot>) -> Self {
        Self {
            snapshots: Mutex::new(snapshots),
            denied_pids: Vec::new(),
        }
    }

    pub fn with_denied_pids(mut self, pids: Vec<u32>) -> Self {
        self.denied_pids = pids;
        self
    }
}

impl SocketProvider for MockSocketProvider {
    fn snapshot(&self) -> SocketSnapshot {
        let mut snapshots = self.snapshots.lock().unwrap();
        if snapshots.len() > 1 {
            snapshots.remove(0)
        } else {
            snapshots
                .first()
                .cloned()
                .expect("MockSocketProvider needs at least one snapshot")
        }
    }

    fn is_permitted(&self, pid: u32) -> bool {
        !self.denied_pids.contains(&pid)
    }
}

/// A DNS provider that never resolves anything — the "no PTR record" case,
/// good enough for tests that don't exercise hostname resolution directly.
pub struct MockDnsProvider;

impl DNSProvider for MockDnsProvider {
    fn resolve(&self, _addr: &str) -> (Option<HostnameObservation>, ProviderStatus) {
        (None, ProviderStatus::observed(chrono::Utc::now()))
    }
}

/// Preloaded with the flows `take_flows` should return — good enough for
/// correlation-matching tests, which don't need a real subprocess/IPC.
#[derive(Default)]
pub struct MockTrafficProvider {
    flows: Mutex<Vec<CapturedFlow>>,
    // Tracks `start`/`stop` calls so `status()` reflects the mock's actual
    // lifecycle instead of always claiming `Observed` — a test exercising
    // `ObservationEngine::traffic_status()` against this mock (rather than
    // just `poll_traffic_flows`) needs it to distinguish a real start/stop
    // from a scripted-but-never-started session.
    running: Mutex<bool>,
}

impl MockTrafficProvider {
    pub fn with_flows(flows: Vec<CapturedFlow>) -> Self {
        Self {
            flows: Mutex::new(flows),
            running: Mutex::new(false),
        }
    }
}

impl TrafficProvider for MockTrafficProvider {
    fn start(&self, _pid: u32) -> ProviderStatus {
        *self.running.lock().unwrap() = true;
        ProviderStatus::observed(chrono::Utc::now())
    }

    fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }

    fn take_flows(&self) -> Vec<CapturedFlow> {
        std::mem::take(&mut self.flows.lock().unwrap())
    }

    fn status(&self) -> ProviderStatus {
        if *self.running.lock().unwrap() {
            ProviderStatus::observed(chrono::Utc::now())
        } else {
            ProviderStatus::unavailable(chrono::Utc::now(), "capture not started")
        }
    }
}
