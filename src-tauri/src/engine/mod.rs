//! The Observation Engine: consumes provider snapshots, diffs them into
//! `NetworkConnection` lifecycle state, merges in process data, and is the
//! single owner of in-memory domain state. See `docs/ARCHITECTURE.md`.
//!
//! Connection-identity matching, the `closed`/`expired` rule, and the
//! `discovered`/`active` transition all follow `docs/DATA_MODEL.md`
//! exactly — see that document for the reasoning, this module for the
//! mechanism.

use std::collections::{HashMap, HashSet};

use chrono::Utc;

use crate::models::{
    Envelope, LifecycleState, NetworkConnection, ObservationState, ObservationStatus,
    ProcessInfo, ProcessState, Protocol, Provider, ProviderState,
};
use crate::providers::{ProcessProvider, SocketProvider};

/// Engine-tracked record for one process — the subset of `ProcessInfo`
/// that's persisted across refreshes; `status`/`active_connection_count`
/// are computed at query time from the engine's current layer statuses.
struct TrackedProcess {
    name: String,
    executable_path: String,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
    process_state: ProcessState,
}

/// The exact tuple the connection-identity matching rule matches on
/// (`docs/DATA_MODEL.md`, "Connection-identity matching rule").
type MatchTuple = (u32, Protocol, String, u16, Option<String>, Option<u16>);

fn observation_tuple(
    pid: u32,
    protocol: Protocol,
    local_addr: &str,
    local_port: u16,
    remote_addr: &Option<String>,
    remote_port: Option<u16>,
) -> MatchTuple {
    (
        pid,
        protocol,
        local_addr.to_string(),
        local_port,
        remote_addr.clone(),
        remote_port,
    )
}

pub struct ObservationEngine {
    process_provider: Box<dyn ProcessProvider>,
    socket_provider: Box<dyn SocketProvider>,
    processes: HashMap<u32, TrackedProcess>,
    connections: HashMap<String, NetworkConnection>,
    next_connection_seq: u64,
    process_layer_status: ObservationStatus,
    socket_layer_status: ObservationStatus,
}

fn not_yet_queried(provider: Provider) -> ObservationStatus {
    ObservationStatus {
        state: ObservationState::Unavailable,
        observed_at: Utc::now(),
        last_successful_at: None,
        reason: Some("not queried yet this session".to_string()),
        provider: Some(provider),
    }
}

impl ObservationEngine {
    pub fn new(
        process_provider: Box<dyn ProcessProvider>,
        socket_provider: Box<dyn SocketProvider>,
    ) -> Self {
        Self {
            process_provider,
            socket_provider,
            processes: HashMap::new(),
            connections: HashMap::new(),
            next_connection_seq: 0,
            process_layer_status: not_yet_queried(Provider::Process),
            socket_layer_status: not_yet_queried(Provider::Socket),
        }
    }

    /// Whether a layer's current status is one that carries data alongside
    /// it (`observed`/`stale`/`transient_failure`) vs. one that doesn't
    /// (`unavailable`/`permission_denied`/`unsupported`) — see
    /// `docs/OBSERVATION_CONTRACT.md`.
    fn carries_data(state: ObservationState) -> bool {
        !matches!(
            state,
            ObservationState::Unavailable
                | ObservationState::PermissionDenied
                | ObservationState::Unsupported
        )
    }

    fn refresh_processes(&mut self) -> ObservationStatus {
        let snapshot = self.process_provider.snapshot();
        let new_status = ObservationStatus::from_provider(
            &snapshot.status,
            Some(&self.process_layer_status),
            Provider::Process,
        );

        if snapshot.status.state == ProviderState::Observed {
            let seen_pids: HashSet<u32> = snapshot.observations.iter().map(|o| o.pid).collect();

            for obs in snapshot.observations {
                self.processes.insert(
                    obs.pid,
                    TrackedProcess {
                        name: obs.name,
                        executable_path: obs.executable_path,
                        cpu_percent: obs.cpu_percent,
                        memory_bytes: obs.memory_bytes,
                        process_state: ProcessState::Running,
                    },
                );
            }

            // Confirmed exit: previously Running, absent from this
            // successful snapshot. This is the one positive closure signal
            // available this phase (`docs/DATA_MODEL.md`'s closed/expired
            // rule) — close that PID's tracked connections too.
            let exited_pids: Vec<u32> = self
                .processes
                .iter()
                .filter(|(pid, p)| {
                    p.process_state == ProcessState::Running && !seen_pids.contains(pid)
                })
                .map(|(pid, _)| *pid)
                .collect();

            for pid in exited_pids {
                if let Some(p) = self.processes.get_mut(&pid) {
                    p.process_state = ProcessState::Exited;
                }
                let now = snapshot.timestamp;
                for conn in self.connections.values_mut() {
                    if conn.pid == pid && conn.lifecycle_state != LifecycleState::Closed {
                        conn.lifecycle_state = LifecycleState::Closed;
                        conn.status = ObservationStatus {
                            state: ObservationState::Observed,
                            observed_at: now,
                            last_successful_at: Some(now),
                            reason: None,
                            provider: Some(Provider::Process),
                        };
                    }
                }
            }
        }
        // A non-`observed` snapshot leaves `self.processes` and every
        // tracked connection's lifecycle untouched — a failed poll must
        // never manufacture state (`TESTING_STRATEGY.md`'s 4th mandatory
        // test).

        self.process_layer_status = new_status.clone();
        new_status
    }

    fn refresh_connections(&mut self) -> ObservationStatus {
        let snapshot = self.socket_provider.snapshot();
        let new_status = ObservationStatus::from_provider(
            &snapshot.status,
            Some(&self.socket_layer_status),
            Provider::Socket,
        );

        if snapshot.status.state == ProviderState::Observed {
            let now = snapshot.timestamp;

            // Only currently-discovered/active connections are eligible to
            // match — a connection absent from one successful snapshot and
            // reappearing later is a *new* connection, not a resumption
            // (`docs/DATA_MODEL.md`).
            let mut tuple_to_id: HashMap<MatchTuple, String> = HashMap::new();
            for (id, conn) in self.connections.iter() {
                if matches!(
                    conn.lifecycle_state,
                    LifecycleState::Discovered | LifecycleState::Active
                ) {
                    let key = observation_tuple(
                        conn.pid,
                        conn.protocol,
                        &conn.local_addr,
                        conn.local_port,
                        &conn.remote_addr,
                        conn.remote_port,
                    );
                    tuple_to_id.insert(key, id.clone());
                }
            }

            let mut seen_ids: HashSet<String> = HashSet::new();

            for obs in snapshot.observations {
                let key = observation_tuple(
                    obs.pid,
                    obs.protocol,
                    &obs.local_addr,
                    obs.local_port,
                    &obs.remote_addr,
                    obs.remote_port,
                );
                // An exact-tuple candidate already claimed this round by an
                // earlier observation is ambiguous — treated the same as no
                // match, per the split-over-merge policy.
                let candidate = tuple_to_id
                    .get(&key)
                    .filter(|id| !seen_ids.contains(*id))
                    .cloned();

                match candidate {
                    Some(id) => {
                        seen_ids.insert(id.clone());
                        if let Some(conn) = self.connections.get_mut(&id) {
                            conn.state = obs.state;
                            conn.bytes_sent = obs.bytes_sent;
                            conn.bytes_received = obs.bytes_received;
                            conn.last_seen = now;
                            // Matched a second consecutive successful
                            // snapshot — satisfies the discovered->active
                            // transition on its own, regardless of state.
                            if conn.lifecycle_state == LifecycleState::Discovered {
                                conn.lifecycle_state = LifecycleState::Active;
                            }
                        }
                    }
                    None => {
                        self.next_connection_seq += 1;
                        let id = format!("c-{}", self.next_connection_seq);
                        let lifecycle_state = if obs.state == "ESTABLISHED" {
                            LifecycleState::Active
                        } else {
                            LifecycleState::Discovered
                        };
                        seen_ids.insert(id.clone());
                        self.connections.insert(
                            id.clone(),
                            NetworkConnection {
                                connection_id: id,
                                pid: obs.pid,
                                protocol: obs.protocol,
                                local_addr: obs.local_addr,
                                local_port: obs.local_port,
                                remote_addr: obs.remote_addr,
                                remote_port: obs.remote_port,
                                state: obs.state,
                                bytes_sent: obs.bytes_sent,
                                bytes_received: obs.bytes_received,
                                lifecycle_state,
                                first_seen: now,
                                last_seen: now,
                                status: new_status.clone(),
                            },
                        );
                    }
                }
            }

            // Previously-tracked, unmatched this round: no positive
            // evidence of closure, so `expired` — never `closed` — per the
            // closed/expired rule. (A `closed` transition only ever
            // happens via confirmed process exit, in `refresh_processes`.)
            for (id, conn) in self.connections.iter_mut() {
                if matches!(
                    conn.lifecycle_state,
                    LifecycleState::Discovered | LifecycleState::Active
                ) && !seen_ids.contains(id)
                {
                    conn.lifecycle_state = LifecycleState::Expired;
                }
            }
        }
        // A non-`observed` snapshot leaves every tracked connection's
        // `lifecycle_state`/`last_seen` unchanged — only `status` (below)
        // reflects the failed/degraded poll.

        for conn in self.connections.values_mut() {
            conn.status = new_status.clone();
        }

        self.socket_layer_status = new_status.clone();
        new_status
    }

    fn to_process_info(&self, pid: u32, tracked: &TrackedProcess) -> ProcessInfo {
        let active_connection_count = if self.socket_layer_status.state == ObservationState::Observed
        {
            Some(
                self.connections
                    .values()
                    .filter(|c| {
                        c.pid == pid
                            && matches!(
                                c.lifecycle_state,
                                LifecycleState::Discovered | LifecycleState::Active
                            )
                    })
                    .count() as u32,
            )
        } else {
            None
        };

        ProcessInfo {
            pid,
            name: tracked.name.clone(),
            executable_path: tracked.executable_path.clone(),
            cpu_percent: tracked.cpu_percent,
            memory_bytes: tracked.memory_bytes,
            process_state: tracked.process_state,
            status: self.process_layer_status.clone(),
            active_connection_count,
        }
    }

    /// `get_processes` — see `docs/OBSERVATION_CONTRACT.md`'s command
    /// contract for the two-level envelope this returns.
    pub fn get_processes(&mut self) -> Envelope<Vec<ProcessInfo>> {
        let status = self.refresh_processes();
        if !Self::carries_data(status.state) {
            return Envelope::denied(status);
        }
        let data = self
            .processes
            .iter()
            .map(|(pid, tracked)| self.to_process_info(*pid, tracked))
            .collect();
        Envelope::ok(status, data)
    }

    /// `get_connections(pid)` — always refreshes the whole socket layer
    /// (one `netstat2` call covers every pid, not just the requested one)
    /// before filtering. Whether `pid` itself is permitted is a separate
    /// check from whether the refresh as a whole succeeded — `netstat2`
    /// can't tell us that itself (`PIF-046`), so the Engine asks
    /// `SocketProvider::is_permitted` directly.
    pub fn get_connections(&mut self, pid: u32) -> Envelope<Vec<NetworkConnection>> {
        let status = self.refresh_connections();

        if !self.socket_provider.is_permitted(pid) {
            let denied = ObservationStatus::denied(
                status.observed_at,
                "other user's process",
                Provider::Socket,
            );
            return Envelope::denied(denied);
        }

        if !Self::carries_data(status.state) {
            return Envelope::denied(status);
        }

        let data = self
            .connections
            .values()
            .filter(|c| c.pid == pid)
            .cloned()
            .collect();
        Envelope::ok(status, data)
    }
}

/// Engine tests for the mandatory scenarios in `docs/TESTING_STRATEGY.md`
/// (process termination, polling-gap-vs-closed, provider-failure-must-not-
/// manufacture-state), run against mock providers — no real OS calls.
#[cfg(test)]
mod engine_tests {
    use super::*;
    use crate::models::{ProcessObservation, ProviderStatus, SocketObservation};
    use crate::tests::{MockProcessProvider, MockSocketProvider};
    use chrono::Duration as ChronoDuration;

    fn process_snapshot(now: chrono::DateTime<Utc>, pids: &[u32]) -> crate::models::ProcessSnapshot {
        crate::models::ProcessSnapshot {
            timestamp: now,
            observations: pids
                .iter()
                .map(|&pid| ProcessObservation {
                    pid,
                    name: format!("proc-{pid}"),
                    executable_path: format!("/usr/bin/proc-{pid}"),
                    cpu_percent: Some(0.0),
                    memory_bytes: Some(1024),
                })
                .collect(),
            status: ProviderStatus::observed(now),
        }
    }

    fn socket_snapshot_established(
        now: chrono::DateTime<Utc>,
        pid: u32,
    ) -> crate::models::SocketSnapshot {
        crate::models::SocketSnapshot {
            timestamp: now,
            observations: vec![SocketObservation {
                pid,
                protocol: Protocol::Tcp,
                local_addr: "127.0.0.1".to_string(),
                local_port: 5000,
                remote_addr: Some("93.184.216.34".to_string()),
                remote_port: Some(443),
                state: "ESTABLISHED".to_string(),
                bytes_sent: None,
                bytes_received: None,
            }],
            status: ProviderStatus::observed(now),
        }
    }

    fn socket_snapshot_empty(now: chrono::DateTime<Utc>) -> crate::models::SocketSnapshot {
        crate::models::SocketSnapshot {
            timestamp: now,
            observations: vec![],
            status: ProviderStatus::observed(now),
        }
    }

    #[test]
    fn process_termination_closes_its_connections() {
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);

        let process = MockProcessProvider::new(vec![
            process_snapshot(t0, &[100]),
            process_snapshot(t1, &[]), // pid 100 confirmed gone
        ]);
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 100),
            socket_snapshot_empty(t1),
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket));

        let processes = engine.get_processes();
        assert_eq!(
            processes.data.as_ref().unwrap()[0].process_state,
            ProcessState::Running
        );

        let connections = engine.get_connections(100);
        assert_eq!(
            connections.data.as_ref().unwrap()[0].lifecycle_state,
            LifecycleState::Active
        );

        // Process exit detected.
        let processes = engine.get_processes();
        let info = &processes.data.unwrap()[0];
        assert_eq!(info.pid, 100);
        assert_eq!(info.process_state, ProcessState::Exited);

        // Its connection is retained and Closed, not silently dropped.
        let connections = engine.get_connections(100);
        let data = connections.data.unwrap();
        assert_eq!(data.len(), 1, "exited process's connection must be retained");
        assert_eq!(data[0].lifecycle_state, LifecycleState::Closed);
    }

    #[test]
    fn polling_gap_expires_not_closes() {
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);

        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[200])]);
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 200),
            socket_snapshot_empty(t1), // same PID still running, socket just gone
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket));

        let first = engine.get_connections(200);
        assert_eq!(
            first.data.unwrap()[0].lifecycle_state,
            LifecycleState::Active
        );

        let second = engine.get_connections(200);
        let data = second.data.unwrap();
        assert_eq!(data.len(), 1, "connection must be retained, not dropped");
        assert_eq!(
            data[0].lifecycle_state,
            LifecycleState::Expired,
            "absence from a successful snapshot is not positive evidence of closure"
        );
    }

    #[test]
    fn provider_failure_does_not_manufacture_state() {
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);

        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[300])]);
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 300),
            crate::models::SocketSnapshot {
                timestamp: t1,
                observations: vec![],
                status: ProviderStatus::transient_failure(t1, "netstat2 call failed"),
            },
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket));

        let first = engine.get_connections(300);
        let first_conn = first.data.unwrap()[0].clone();
        assert_eq!(first_conn.lifecycle_state, LifecycleState::Active);

        let second = engine.get_connections(300);
        assert_eq!(second.status.state, ObservationState::TransientFailure);
        let data = second.data.expect("transient_failure still carries last-known data");
        assert_eq!(data.len(), 1);
        assert_eq!(
            data[0].lifecycle_state,
            first_conn.lifecycle_state,
            "lifecycle_state must be unchanged on a failed poll"
        );
        assert_eq!(
            data[0].last_seen, first_conn.last_seen,
            "last_seen must be unchanged on a failed poll"
        );
        assert_eq!(
            data[0].status.last_successful_at,
            Some(t0),
            "must carry forward the last successful observation time"
        );
    }

    #[test]
    fn other_users_pid_is_permission_denied_not_empty() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[400])]);
        let socket =
            MockSocketProvider::new(vec![socket_snapshot_empty(t0)]).with_denied_pids(vec![999]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket));

        let result = engine.get_connections(999);
        assert_eq!(result.status.state, ObservationState::PermissionDenied);
        assert!(result.data.is_none());
    }

    #[test]
    fn discovered_promotes_to_active_on_second_match() {
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);

        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[500])]);
        let listen_snapshot = crate::models::SocketSnapshot {
            timestamp: t0,
            observations: vec![SocketObservation {
                pid: 500,
                protocol: Protocol::Tcp,
                local_addr: "0.0.0.0".to_string(),
                local_port: 8080,
                remote_addr: None,
                remote_port: None,
                state: "LISTEN".to_string(),
                bytes_sent: None,
                bytes_received: None,
            }],
            status: ProviderStatus::observed(t0),
        };
        let mut listen_snapshot_2 = listen_snapshot.clone();
        listen_snapshot_2.timestamp = t1;
        listen_snapshot_2.status = ProviderStatus::observed(t1);

        let socket = MockSocketProvider::new(vec![listen_snapshot, listen_snapshot_2]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket));

        let first = engine.get_connections(500);
        assert_eq!(
            first.data.unwrap()[0].lifecycle_state,
            LifecycleState::Discovered,
            "a LISTEN socket seen once is discovered, not active yet"
        );

        let second = engine.get_connections(500);
        assert_eq!(
            second.data.unwrap()[0].lifecycle_state,
            LifecycleState::Active,
            "matched a second consecutive successful snapshot"
        );
    }

    /// Against the real providers (`sysinfo`/`netstat2`), not mocks — the
    /// one test in this module that actually calls into the OS. Confirms
    /// the wiring works end to end, independent of the mock-based unit
    /// tests above.
    #[test]
    fn real_providers_see_this_test_process() {
        let mut engine = ObservationEngine::new(
            Box::new(crate::providers::SysinfoProcessProvider),
            Box::new(crate::providers::NetstatSocketProvider),
        );

        let own_pid = std::process::id();
        let processes = engine.get_processes();
        assert_eq!(processes.status.state, ObservationState::Observed);
        let data = processes.data.expect("get_processes must carry data when observed");
        assert!(
            data.iter().any(|p| p.pid == own_pid),
            "this test's own process should be visible to an unprivileged same-user enumeration"
        );

        // Our own pid is always permitted (same user) — must never come
        // back permission_denied.
        let connections = engine.get_connections(own_pid);
        assert_eq!(connections.status.state, ObservationState::Observed);
    }

    /// Automates the Phase 0.1 demo checkpoint (`docs/[9] TODO.md`): binds
    /// a real socket in this test process, confirms the Engine (via the
    /// real `netstat2` provider) reports it, and cross-checks the exact
    /// same fact against `lsof -p <pid> -i -n -P` run independently.
    #[test]
    fn real_socket_matches_lsof() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a real listening socket");
        let port = listener.local_addr().unwrap().port();
        let own_pid = std::process::id();

        let mut engine = ObservationEngine::new(
            Box::new(crate::providers::SysinfoProcessProvider),
            Box::new(crate::providers::NetstatSocketProvider),
        );
        let connections = engine.get_connections(own_pid);
        let data = connections.data.expect("get_connections must carry data when observed");
        let seen_by_engine = data
            .iter()
            .any(|c| c.local_port == port && c.state == "LISTEN");
        assert!(
            seen_by_engine,
            "Engine did not report the socket this test just bound on port {port}"
        );

        let lsof_output = std::process::Command::new("lsof")
            .args(["-p", &own_pid.to_string(), "-i", "-n", "-P"])
            .output()
            .expect("run lsof");
        let lsof_stdout = String::from_utf8_lossy(&lsof_output.stdout);
        assert!(
            lsof_stdout.contains(&format!(":{port} ")),
            "lsof independently disagrees with the Engine about port {port}:\n{lsof_stdout}"
        );

        drop(listener);
    }

    #[test]
    fn reused_local_port_after_gap_is_a_new_connection_id() {
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);
        let t2 = t0 + ChronoDuration::seconds(2);

        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[600])]);
        // Same exact tuple at t0 and t2, but absent at t1 in between — per
        // the matching rule, this must NOT resume the original connection.
        let mut snap2 = socket_snapshot_established(t1, 600);
        snap2.observations.clear(); // gap: socket briefly absent
        snap2.status = ProviderStatus::observed(t1);
        let mut snap3 = socket_snapshot_established(t2, 600);
        snap3.status = ProviderStatus::observed(t2);

        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 600),
            snap2,
            snap3,
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket));

        let first_id = engine.get_connections(600).data.unwrap()[0].connection_id.clone();
        let after_gap = engine.get_connections(600);
        let gap_data = after_gap.data.unwrap();
        assert_eq!(gap_data.len(), 1, "the connection is retained, not dropped, during the gap");
        assert_eq!(
            gap_data[0].lifecycle_state,
            LifecycleState::Expired,
            "no new evidence during the gap, so it's expired, not actively present"
        );
        let reappeared = engine.get_connections(600);
        let reappeared_data = reappeared.data.unwrap();
        assert_eq!(reappeared_data.len(), 2, "old (now expired) + new connection both retained");
        let new_conn = reappeared_data
            .iter()
            .find(|c| c.lifecycle_state != LifecycleState::Expired)
            .expect("the reappearing observation should produce a fresh, non-expired connection");
        assert_ne!(
            new_conn.connection_id, first_id,
            "port reuse after a gap must mint a new connection_id, not resume the old one"
        );
        let old_conn = reappeared_data
            .iter()
            .find(|c| c.connection_id == first_id)
            .expect("the original connection must still be retained, not dropped");
        assert_eq!(old_conn.lifecycle_state, LifecycleState::Expired);
    }
}
