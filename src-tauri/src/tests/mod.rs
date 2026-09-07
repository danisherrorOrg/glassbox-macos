//! Shared mock provider implementations, returning scripted fixture
//! `ProcessSnapshot`/`SocketSnapshot` values, per `docs/TESTING_STRATEGY.md`.
//! Used by `engine`'s unit tests to exercise lifecycle/correlation logic
//! without any real OS calls.

use std::sync::Mutex;

use crate::models::{ProcessSnapshot, SocketSnapshot};
use crate::providers::{ProcessProvider, SocketProvider};

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
