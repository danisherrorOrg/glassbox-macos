//! The Observation Engine: consumes provider snapshots, diffs them into
//! `NetworkConnection` lifecycle state, merges in process data, and is the
//! single owner of in-memory domain state. See `docs/ARCHITECTURE.md`.
//!
//! Connection-identity matching, the `closed`/`expired` rule, and the
//! `discovered`/`active` transition all follow `docs/DATA_MODEL.md`
//! exactly — see that document for the reasoning, this module for the
//! mechanism.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use crate::models::{
    polling_stale_threshold, CorrelationEvidence, Envelope, HostnameObservation, LifecycleState,
    NetworkConnection, ObservationCapabilities, ObservationState, ObservationStatus, ProcessInfo,
    ProcessState, Protocol, Provider, ProviderState, ProviderStatus, ResolvedHostname,
    TrafficEvent, TrafficEventType, PHASE_0_1_STALE_THRESHOLD_SECONDS,
};
use crate::providers::{CapturedFlow, DNSProvider, ProcessProvider, SocketProvider, TrafficProvider};

/// Engine-tracked record for one process — the subset of `ProcessInfo`
/// that's persisted across refreshes; `status`/`active_connection_count`
/// are computed at query time from the engine's current layer statuses.
struct TrackedProcess {
    name: String,
    executable_path: Option<String>,
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
    dns_provider: Arc<dyn DNSProvider>,
    traffic_provider: Box<dyn TrafficProvider>,
    processes: HashMap<u32, TrackedProcess>,
    connections: HashMap<String, NetworkConnection>,
    next_connection_seq: u64,
    process_layer_status: ObservationStatus,
    socket_layer_status: ObservationStatus,
    /// `None` until live monitoring (Phase 0.2) has configured a real
    /// interval at least once this session — see `stale_threshold`.
    poll_interval_ms: Option<u64>,
    /// `None` = "looked up, no PTR record" (a real, negative result — not
    /// retried every tick); absent entirely = never looked up yet.
    hostname_cache: HashMap<String, Option<HostnameObservation>>,
    dns_in_flight: HashSet<String>,
    /// Addresses newly seen this refresh that need a lookup — drained by
    /// `take_pending_dns_lookups` so the async caller can resolve them via
    /// `spawn_blocking` without holding the engine lock during the call.
    pending_dns: Vec<String>,
    /// Full session history, queried by `get_timeline`.
    timeline: Vec<TrafficEvent>,
    next_event_seq: u64,
    /// Drained by `take_pending_events` for live per-tick event emission,
    /// separate from `timeline` (which never shrinks).
    pending_events: Vec<TrafficEvent>,
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
        dns_provider: Arc<dyn DNSProvider>,
        traffic_provider: Box<dyn TrafficProvider>,
    ) -> Self {
        Self {
            process_provider,
            socket_provider,
            dns_provider,
            traffic_provider,
            processes: HashMap::new(),
            connections: HashMap::new(),
            next_connection_seq: 0,
            process_layer_status: not_yet_queried(Provider::Process),
            socket_layer_status: not_yet_queried(Provider::Socket),
            poll_interval_ms: None,
            hostname_cache: HashMap::new(),
            dns_in_flight: HashSet::new(),
            pending_dns: Vec::new(),
            timeline: Vec::new(),
            next_event_seq: 0,
            pending_events: Vec::new(),
        }
    }

    /// `docs/OBSERVATION_CONTRACT.md`'s staleness formula: Phase 0.1's flat
    /// 30s until live monitoring (`set_poll_interval_ms`) has configured a
    /// real interval this session, `3 × that interval` after.
    fn stale_threshold(&self) -> Duration {
        match self.poll_interval_ms {
            Some(ms) => polling_stale_threshold(ms),
            None => Duration::seconds(PHASE_0_1_STALE_THRESHOLD_SECONDS),
        }
    }

    /// Called once live monitoring starts (`docs/[9] TODO.md` Phase 0.2).
    pub fn set_poll_interval_ms(&mut self, ms: u64) {
        self.poll_interval_ms = Some(ms);
    }

    fn record_event(&mut self, event_type: TrafficEventType, connection_id: &str, now: DateTime<Utc>) {
        self.next_event_seq += 1;
        let event = TrafficEvent {
            event_id: format!("e-{}", self.next_event_seq),
            timestamp: now,
            event_type,
            connection_id: Some(connection_id.to_string()),
            request_id: None,
            response_id: None,
        };
        self.timeline.push(event.clone());
        self.pending_events.push(event);
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
            self.stale_threshold(),
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
                let mut newly_closed = Vec::new();
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
                        newly_closed.push(conn.connection_id.clone());
                    }
                }
                for id in newly_closed {
                    self.record_event(TrafficEventType::ConnectionClosed, &id, now);
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
            self.stale_threshold(),
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
            let mut newly_opened: Vec<String> = Vec::new();
            let mut candidate_addrs: Vec<String> = Vec::new();

            for obs in snapshot.observations {
                if let Some(addr) = &obs.remote_addr {
                    candidate_addrs.push(addr.clone());
                }

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
                                newly_opened.push(id);
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
                        if lifecycle_state == LifecycleState::Active {
                            newly_opened.push(id.clone());
                        }
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

            for id in newly_opened {
                self.record_event(TrafficEventType::ConnectionOpened, &id, now);
            }

            for addr in candidate_addrs {
                if !self.hostname_cache.contains_key(&addr) && !self.dns_in_flight.contains(&addr) {
                    self.dns_in_flight.insert(addr.clone());
                    self.pending_dns.push(addr);
                }
            }

            // Previously-tracked, unmatched this round: no positive
            // evidence of closure, so `expired` — never `closed` — per the
            // closed/expired rule. (A `closed` transition only ever
            // happens via confirmed process exit, in `refresh_processes`.)
            let mut newly_expired: Vec<String> = Vec::new();
            for (id, conn) in self.connections.iter_mut() {
                if matches!(
                    conn.lifecycle_state,
                    LifecycleState::Discovered | LifecycleState::Active
                ) && !seen_ids.contains(id)
                {
                    conn.lifecycle_state = LifecycleState::Expired;
                    newly_expired.push(id.clone());
                }
            }
            for id in newly_expired {
                self.record_event(TrafficEventType::ConnectionExpired, &id, now);
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

    /// Addresses newly seen this refresh that still need a reverse-DNS
    /// lookup. The caller resolves each via `dns_provider()` (typically in
    /// a `spawn_blocking` task, since the lookup is a blocking OS call) and
    /// reports the result back through `record_hostname`, without holding
    /// the engine lock for the duration of the lookup itself.
    pub fn take_pending_dns_lookups(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_dns)
    }

    pub fn dns_provider(&self) -> Arc<dyn DNSProvider> {
        self.dns_provider.clone()
    }

    pub fn record_hostname(&mut self, addr: String, observation: Option<HostnameObservation>) {
        self.dns_in_flight.remove(&addr);
        self.hostname_cache.insert(addr, observation);
    }

    /// `get_hostnames(pid)` — the highest-confidence `ResolvedHostname` per
    /// connection when sources disagree, both shown if more than one
    /// scores above 0.85 (`docs/DATA_MODEL.md`'s display rule). Only ever
    /// one source (`ReverseDns`) exists until Phase 0.4 adds `Sni`/
    /// `HttpHost`, so today this always returns at most one per connection
    /// — the grouping logic is here now so 0.4 doesn't have to add it.
    pub fn get_hostnames(&self, pid: u32) -> Envelope<Vec<ResolvedHostname>> {
        let now = Utc::now();
        let mut data = Vec::new();
        for conn in self.connections.values().filter(|c| c.pid == pid) {
            let Some(addr) = &conn.remote_addr else {
                continue;
            };
            let Some(Some(obs)) = self.hostname_cache.get(addr) else {
                continue;
            };
            data.push(ResolvedHostname {
                connection_id: conn.connection_id.clone(),
                source: obs.source,
                hostname: obs.hostname.clone(),
                confidence: obs.confidence,
                status: ObservationStatus {
                    state: ObservationState::Observed,
                    observed_at: now,
                    last_successful_at: Some(obs.observed_at),
                    reason: None,
                    provider: Some(Provider::Dns),
                },
            });
        }
        let status = ObservationStatus {
            state: ObservationState::Observed,
            observed_at: now,
            last_successful_at: Some(now),
            reason: None,
            provider: Some(Provider::Dns),
        };
        Envelope::ok(status, data)
    }

    /// `get_timeline(pid)` — every `TrafficEvent` for a connection that has
    /// ever belonged to `pid`, including closed/expired ones (the Engine
    /// retains those, same as `get_connections`).
    pub fn get_timeline(&self, pid: u32) -> Envelope<Vec<TrafficEvent>> {
        let connection_ids: HashSet<&str> = self
            .connections
            .values()
            .filter(|c| c.pid == pid)
            .map(|c| c.connection_id.as_str())
            .collect();
        let data = self
            .timeline
            .iter()
            .filter(|e| {
                e.connection_id
                    .as_deref()
                    .is_some_and(|id| connection_ids.contains(id))
            })
            .cloned()
            .collect();
        let now = Utc::now();
        let status = ObservationStatus {
            state: ObservationState::Observed,
            observed_at: now,
            last_successful_at: Some(now),
            reason: None,
            provider: Some(Provider::Engine),
        };
        Envelope::ok(status, data)
    }

    /// Events generated by the most recent `refresh_connections`/
    /// `refresh_processes` calls, for live per-tick emission. Does not
    /// affect `get_timeline`, which reads the full retained history.
    pub fn take_pending_events(&mut self) -> Vec<TrafficEvent> {
        std::mem::take(&mut self.pending_events)
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

    /// Starts traffic capture for `pid` (`docs/[9] TODO.md` Phase 0.3).
    pub fn start_traffic_capture(&mut self, pid: u32) -> ProviderStatus {
        self.traffic_provider.start(pid)
    }

    pub fn stop_traffic_capture(&mut self) {
        self.traffic_provider.stop();
    }

    /// Drains whatever flows the traffic provider has captured since the
    /// last call, pairing each with the `connection_id` it correlates to
    /// (if any) — the Phase 0.3 "spike, independently of the UI" item.
    /// Does **not** attach anything to `NetworkConnection` itself or emit
    /// `TrafficEvent`s for `Request`/`Response` — that Engine-wiring step
    /// is explicitly Phase 0.4 (`docs/[9] TODO.md`), once `HTTPRequest`/
    /// `HTTPResponse` and the `Redactor` exist to produce what would
    /// actually get attached.
    pub fn poll_traffic_flows(&self) -> Vec<(CapturedFlow, Option<String>)> {
        self.traffic_provider
            .take_flows()
            .into_iter()
            .map(|flow| {
                let matched = self.correlate(&flow.evidence);
                (flow, matched)
            })
            .collect()
    }

    /// `CorrelationEvidence` → `NetworkConnection` matching. Matches on
    /// `(pid, remote_addr, remote_port)` only — **not** the full 6-tuple
    /// `docs/DATA_MODEL.md`'s socket-to-socket matching rule uses. This is
    /// a deliberate, empirically-driven difference: the Phase 0.3 spike
    /// (`docs/PERMISSIONS_AND_PLATFORM.md`) found that `mitmproxy`'s
    /// `local:<pid>` capture never exposes a usable `local_addr`/
    /// `local_port` — `client_conn` reflects the local redirector's own
    /// loopback stub connection, not the real originating socket — so
    /// `CorrelationEvidence.local_addr`/`local_port` are always `None` in
    /// practice for this provider. `pid` is filled in independently by the
    /// Rust core (it already knows which pid a capture session targets),
    /// not read from the evidence the provider itself produced.
    ///
    /// Per the split-over-merge policy already established for socket
    /// matching: zero or more-than-one equally-good candidate is treated
    /// as no match — a `connection_id` is never fabricated when confidence
    /// is insufficient (`docs/[9] TODO.md`'s explicit review principle).
    ///
    /// **Candidates are not restricted to `Discovered`/`Active`, unlike the
    /// socket-to-socket matching rule.** An earlier version of this method
    /// copied that restriction over, reasoning by analogy — and the Phase
    /// 0.3 spike's own integration test (`real_traffic_capture_correlates_
    /// to_real_connection`) caught it as wrong empirically: real HTTP/1.0
    /// requests against `test-target` routinely complete and their
    /// connection closes (transitioning to `Expired`) *faster* than the
    /// socket-polling interval, so by the time a captured flow is actually
    /// available to correlate, the connection it belongs to has often
    /// already left `Active`. Excluding `Expired`/`Closed` candidates here
    /// doesn't prevent the same kind of ambiguity the socket rule guards
    /// against (this method's own zero-or-multiple-candidates check
    /// already does that, independent of lifecycle state) — it just
    /// silently produced `unmatched` for the *ordinary*, expected case.
    /// `connection_id` stays valid and meaningful for the life of the
    /// tracked connection object regardless of its current
    /// `lifecycle_state`, so there's no correctness reason to exclude any
    /// state here.
    pub fn correlate(&self, evidence: &CorrelationEvidence) -> Option<String> {
        let pid = evidence.pid?;
        let remote_addr = evidence.remote_addr.as_deref()?;
        let remote_port = evidence.remote_port?;

        let mut candidates = self.connections.values().filter(|c| {
            c.pid == pid
                && c.remote_addr.as_deref() == Some(remote_addr)
                && c.remote_port == Some(remote_port)
        });

        let first = candidates.next()?;
        if candidates.next().is_some() {
            return None; // ambiguous — more than one equally-good match
        }
        Some(first.connection_id.clone())
    }

    /// `get_capabilities()` (`docs/[9] TODO.md` Phase 0.3): Engine-aggregated
    /// from each provider's own self-report. Each provider only ever sets
    /// the fields it owns (see `docs/DATA_MODEL.md`'s `ObservationCapabilities`)
    /// — this method reads exactly those fields from each and defaults
    /// everything else to the least-capable value, rather than attempting a
    /// generic merge across providers that could let one's silence
    /// overwrite another's real answer.
    pub fn get_capabilities(&self) -> ObservationCapabilities {
        let process = self.process_provider.capabilities();
        let socket = self.socket_provider.capabilities();
        let dns = self.dns_provider.capabilities();
        let traffic = self.traffic_provider.capabilities();

        use crate::models::{Availability, LimitedAvailability};
        ObservationCapabilities {
            process: process.process.unwrap_or(Availability::Unavailable),
            sockets: socket.sockets.unwrap_or(Availability::Unavailable),
            remote_addresses: socket.remote_addresses.unwrap_or(Availability::Unavailable),
            dns: dns.dns.unwrap_or(Availability::Unavailable),
            http_metadata: traffic.http_metadata.unwrap_or(LimitedAvailability::Unsupported),
            https_metadata: traffic.https_metadata.unwrap_or(LimitedAvailability::Unsupported),
            request_body: traffic.request_body.unwrap_or(LimitedAvailability::Unsupported),
            response_body: traffic.response_body.unwrap_or(LimitedAvailability::Unsupported),
            raw_packet_data: traffic.raw_packet_data.unwrap_or(Availability::Unavailable),
        }
    }
}

/// Engine tests for the mandatory scenarios in `docs/TESTING_STRATEGY.md`
/// (process termination, polling-gap-vs-closed, provider-failure-must-not-
/// manufacture-state), run against mock providers — no real OS calls.
#[cfg(test)]
mod engine_tests {
    use super::*;
    use crate::models::{ProcessObservation, ProviderStatus, SocketObservation};
    use crate::tests::{MockDnsProvider, MockProcessProvider, MockSocketProvider};
    use chrono::Duration as ChronoDuration;

    fn process_snapshot(now: chrono::DateTime<Utc>, pids: &[u32]) -> crate::models::ProcessSnapshot {
        crate::models::ProcessSnapshot {
            timestamp: now,
            observations: pids
                .iter()
                .map(|&pid| ProcessObservation {
                    pid,
                    name: format!("proc-{pid}"),
                    executable_path: Some(format!("/usr/bin/proc-{pid}")),
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
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

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
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

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
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

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
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

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
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

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
            Box::new(crate::providers::SysinfoProcessProvider::new()),
            Box::new(crate::providers::NetstatSocketProvider),
            Arc::new(crate::providers::ReverseDnsProvider),
            Box::new(crate::providers::MitmproxyTrafficProvider::default()),
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
            Box::new(crate::providers::SysinfoProcessProvider::new()),
            Box::new(crate::providers::NetstatSocketProvider),
            Arc::new(crate::providers::ReverseDnsProvider),
            Box::new(crate::providers::MitmproxyTrafficProvider::default()),
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
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

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

    #[test]
    fn traffic_events_fire_on_lifecycle_transitions() {
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);
        let t2 = t0 + ChronoDuration::seconds(2);

        let process = MockProcessProvider::new(vec![
            process_snapshot(t0, &[700]),
            process_snapshot(t2, &[]), // pid 700 exits at t2
        ]);
        // t0: ESTABLISHED at first sight -> immediately Active -> Opened.
        // t1: gone -> Expired.
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 700),
            socket_snapshot_empty(t1),
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

        engine.get_processes(); // t0: consumes the "running" process snapshot
        engine.get_connections(700); // t0: opened
        engine.get_connections(700); // t1: expired
        engine.get_processes(); // t2: process exits -> closes the (expired) connection

        let timeline = engine.get_timeline(700);
        let events = timeline.data.unwrap();
        let types: Vec<TrafficEventType> = events.iter().map(|e| e.event_type).collect();
        assert!(types.contains(&TrafficEventType::ConnectionOpened));
        assert!(types.contains(&TrafficEventType::ConnectionExpired));
        assert!(types.contains(&TrafficEventType::ConnectionClosed));
        for e in &events {
            assert!(e.connection_id.is_some());
        }
    }

    #[test]
    fn take_pending_events_drains_without_affecting_timeline() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[710])]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_established(t0, 710)]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

        engine.get_connections(710);
        let pending = engine.take_pending_events();
        assert_eq!(pending.len(), 1);
        assert!(engine.take_pending_events().is_empty(), "drained once, empty the second time");

        // The full timeline is unaffected by draining pending_events.
        let timeline = engine.get_timeline(710).data.unwrap();
        assert_eq!(timeline.len(), 1);
    }

    #[test]
    fn pending_dns_lookups_are_deduplicated_across_refreshes() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[720])]);
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 720),
            socket_snapshot_established(t0, 720),
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

        engine.get_connections(720);
        let first_pending = engine.take_pending_dns_lookups();
        assert_eq!(first_pending, vec!["93.184.216.34".to_string()]);

        // Same remote address seen again — already in-flight, not re-queued.
        engine.get_connections(720);
        assert!(engine.take_pending_dns_lookups().is_empty());
    }

    #[test]
    fn resolved_hostnames_surface_by_connection() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[730])]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_established(t0, 730)]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

        let conn = engine.get_connections(730).data.unwrap()[0].clone();
        assert!(engine.get_hostnames(730).data.unwrap().is_empty(), "nothing resolved yet");

        engine.record_hostname(
            "93.184.216.34".to_string(),
            Some(crate::models::HostnameObservation {
                queried_addr: "93.184.216.34".to_string(),
                source: crate::models::HostnameSource::ReverseDns,
                hostname: "example.com".to_string(),
                confidence: 0.5,
                observed_at: t0,
            }),
        );

        let resolved = engine.get_hostnames(730).data.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].connection_id, conn.connection_id);
        assert_eq!(resolved[0].hostname, "example.com");
    }

    #[test]
    fn configured_poll_interval_changes_the_stale_threshold() {
        let t0 = Utc::now();
        // 3 * 1000ms = 3000ms threshold. t1 is 2s later (not yet stale),
        // t2 is 4s later (stale).
        let t1 = t0 + ChronoDuration::seconds(2);
        let t2 = t0 + ChronoDuration::seconds(4);

        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[740])]);
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 740),
            crate::models::SocketSnapshot {
                timestamp: t1,
                observations: vec![],
                status: ProviderStatus::transient_failure(t1, "still down"),
            },
            crate::models::SocketSnapshot {
                timestamp: t2,
                observations: vec![],
                status: ProviderStatus::transient_failure(t2, "still down"),
            },
        ]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));
        engine.set_poll_interval_ms(1000);

        engine.get_connections(740); // t0: observed
        let at_t1 = engine.get_connections(740);
        assert_eq!(
            at_t1.status.state,
            ObservationState::TransientFailure,
            "2s < 3s threshold, not stale yet"
        );
        let at_t2 = engine.get_connections(740);
        assert_eq!(
            at_t2.status.state,
            ObservationState::Stale,
            "4s > 3s threshold (3 * 1000ms poll interval)"
        );
    }

    /// A provider that couldn't determine `executable_path` (e.g. both
    /// `sysinfo::Process::exe()` and the `proc_pidpath` fallback failed —
    /// genuinely happens for `pid 0`/`kernel_task`) must surface `None`,
    /// never a silent `""` that would look like a real, observed empty path.
    #[test]
    fn unknown_executable_path_is_none_not_empty_string() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![crate::models::ProcessSnapshot {
            timestamp: t0,
            observations: vec![crate::models::ProcessObservation {
                pid: 0,
                name: "kernel_task".to_string(),
                executable_path: None,
                cpu_percent: Some(0.0),
                memory_bytes: Some(0),
            }],
            status: ProviderStatus::observed(t0),
        }]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_empty(t0)]);
        let mut engine = ObservationEngine::new(Box::new(process), Box::new(socket), Arc::new(MockDnsProvider), Box::new(crate::tests::MockTrafficProvider::default()));

        let processes = engine.get_processes();
        let info = &processes.data.unwrap()[0];
        assert_eq!(info.pid, 0);
        assert_eq!(
            info.executable_path, None,
            "unknown path must be None, never a silent empty string standing in for real data"
        );
    }

    fn evidence(pid: Option<u32>, remote_addr: &str, remote_port: u16) -> CorrelationEvidence {
        CorrelationEvidence {
            pid,
            protocol: Some("tcp".to_string()),
            local_addr: None,
            local_port: None,
            remote_addr: Some(remote_addr.to_string()),
            remote_port: Some(remote_port),
            hostname: Some("example.com".to_string()),
            timestamp: Utc::now(),
            source: "mitmproxy-local".to_string(),
        }
    }

    /// The Phase 0.3 "spike, independently of the UI" item: confirms the
    /// pid/remote_addr/remote_port matching strategy actually finds the
    /// right connection — the same tuple `socket_snapshot_established`'s
    /// fixture connections always use (`93.184.216.34:443`).
    #[test]
    fn correlate_matches_by_pid_and_remote_addr_port() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[800])]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_established(t0, 800)]);
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(MockDnsProvider),
            Box::new(crate::tests::MockTrafficProvider::default()),
        );
        let conn = engine.get_connections(800).data.unwrap()[0].clone();

        let matched = engine.correlate(&evidence(Some(800), "93.184.216.34", 443));
        assert_eq!(matched, Some(conn.connection_id));
    }

    /// `poll_traffic_flows` end to end: drains the (mocked) traffic
    /// provider and pairs each flow with `correlate`'s result, using a
    /// scripted `MockTrafficProvider` rather than a real `mitmdump`
    /// session — `real_traffic_capture_correlates_to_real_connection`
    /// covers the real-provider path.
    #[test]
    fn poll_traffic_flows_pairs_each_flow_with_its_correlation() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[810])]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_established(t0, 810)]);
        let scripted_flow = crate::providers::CapturedFlow {
            request: crate::models::RawHTTPRequest {
                method: "GET".to_string(),
                host: "93.184.216.34".to_string(),
                path: "/".to_string(),
                headers: std::collections::HashMap::new(),
                body: None,
                timestamp: t0,
            },
            response: None,
            evidence: evidence(Some(810), "93.184.216.34", 443),
        };
        let traffic = crate::tests::MockTrafficProvider::with_flows(vec![scripted_flow]);
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(MockDnsProvider),
            Box::new(traffic),
        );
        let conn = engine.get_connections(810).data.unwrap()[0].clone();

        let polled = engine.poll_traffic_flows();
        assert_eq!(polled.len(), 1);
        assert_eq!(polled[0].0.request.method, "GET");
        assert_eq!(polled[0].1, Some(conn.connection_id));

        // Draining is destructive, same contract as take_pending_events.
        assert!(engine.poll_traffic_flows().is_empty());
    }

    #[test]
    fn correlate_no_match_for_wrong_pid() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[801])]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_established(t0, 801)]);
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(MockDnsProvider),
            Box::new(crate::tests::MockTrafficProvider::default()),
        );
        engine.get_connections(801);

        // Same remote_addr/port, but the evidence's pid doesn't match any
        // tracked connection for that address.
        let matched = engine.correlate(&evidence(Some(999), "93.184.216.34", 443));
        assert_eq!(matched, None);
    }

    #[test]
    fn correlate_no_match_when_evidence_incomplete() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[802])]);
        let socket = MockSocketProvider::new(vec![socket_snapshot_established(t0, 802)]);
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(MockDnsProvider),
            Box::new(crate::tests::MockTrafficProvider::default()),
        );
        engine.get_connections(802);

        let mut no_pid = evidence(Some(802), "93.184.216.34", 443);
        no_pid.pid = None;
        assert_eq!(
            engine.correlate(&no_pid),
            None,
            "must never guess a connection_id when pid itself is unknown"
        );

        let mut no_remote = evidence(Some(802), "93.184.216.34", 443);
        no_remote.remote_addr = None;
        assert_eq!(engine.correlate(&no_remote), None);
    }

    #[test]
    fn correlate_still_matches_closed_and_expired_connections() {
        // Not a restatement of the socket-matching rule's "prefer split
        // over merge" policy — this asserts the opposite of what an
        // earlier version of `correlate` assumed by analogy with that
        // rule, and got wrong (see `correlate`'s doc comment): a traffic
        // flow captured for a connection that has since expired or closed
        // must still correlate to it. This is the ordinary case in
        // practice, not an edge case — `real_traffic_capture_correlates_
        // to_real_connection` hit exactly this with real HTTP/1.0 requests
        // that close faster than the socket-polling interval.
        let t0 = Utc::now();
        let t1 = t0 + ChronoDuration::seconds(1);
        let t2 = t0 + ChronoDuration::seconds(2);
        let process = MockProcessProvider::new(vec![
            process_snapshot(t0, &[803]),
            process_snapshot(t2, &[]), // pid 803 exits -> its connection closes
        ]);
        let socket = MockSocketProvider::new(vec![
            socket_snapshot_established(t0, 803),
            socket_snapshot_empty(t1), // t1: connection expires (no exit yet)
        ]);
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(MockDnsProvider),
            Box::new(crate::tests::MockTrafficProvider::default()),
        );
        engine.get_processes(); // t0: running
        engine.get_connections(803); // t0: active
        engine.get_connections(803); // t1: expired

        let matched_when_expired = engine.correlate(&evidence(Some(803), "93.184.216.34", 443));
        assert!(
            matched_when_expired.is_some(),
            "an expired connection is still a valid, meaningful correlation target"
        );

        engine.get_processes(); // t2: exited -> connection closed
        let matched_when_closed = engine.correlate(&evidence(Some(803), "93.184.216.34", 443));
        assert_eq!(
            matched_when_closed, matched_when_expired,
            "a closed connection must still correlate too, to the same connection_id"
        );
    }

    #[test]
    fn correlate_ambiguous_match_returns_none() {
        let t0 = Utc::now();
        let process = MockProcessProvider::new(vec![process_snapshot(t0, &[804])]);
        // Two simultaneous connections from the same pid to the exact same
        // remote_addr/port (e.g. connection-pooled HTTP/2) — evidence alone
        // can't tell them apart, so this must not guess.
        let snapshot = crate::models::SocketSnapshot {
            timestamp: t0,
            observations: vec![
                crate::models::SocketObservation {
                    pid: 804,
                    protocol: Protocol::Tcp,
                    local_addr: "192.168.1.5".to_string(),
                    local_port: 50001,
                    remote_addr: Some("93.184.216.34".to_string()),
                    remote_port: Some(443),
                    state: "ESTABLISHED".to_string(),
                    bytes_sent: None,
                    bytes_received: None,
                },
                crate::models::SocketObservation {
                    pid: 804,
                    protocol: Protocol::Tcp,
                    local_addr: "192.168.1.5".to_string(),
                    local_port: 50002,
                    remote_addr: Some("93.184.216.34".to_string()),
                    remote_port: Some(443),
                    state: "ESTABLISHED".to_string(),
                    bytes_sent: None,
                    bytes_received: None,
                },
            ],
            status: ProviderStatus::observed(t0),
        };
        let socket = MockSocketProvider::new(vec![snapshot]);
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(MockDnsProvider),
            Box::new(crate::tests::MockTrafficProvider::default()),
        );
        let data = engine.get_connections(804).data.unwrap();
        assert_eq!(data.len(), 2);

        let matched = engine.correlate(&evidence(Some(804), "93.184.216.34", 443));
        assert_eq!(
            matched, None,
            "two equally-good candidates must be treated as no match, never a guessed connection_id"
        );
    }

    /// Real end-to-end proof of the Phase 0.3 demo checkpoint: a real
    /// `mitmdump` capture (via `MitmproxyTrafficProvider`, not a mock)
    /// against a real `NetworkTestTarget` process, correctly correlated to
    /// the `NetworkConnection` the real `SocketProvider` independently
    /// observed for the same connection. Requires `mitmproxy` installed and
    /// the one-time macOS approvals already granted (`docs/
    /// PERMISSIONS_AND_PLATFORM.md`) — same environment-dependent-but-real
    /// testing standard as `real_socket_matches_lsof`. Also verifies PID
    /// scoping: a request from an *unrelated* process during the same
    /// capture window must never appear (`docs/[9] TODO.md`'s "verify
    /// traffic from unrelated processes never leaks into a session").
    #[tokio::test]
    async fn real_traffic_capture_correlates_to_real_connection() {
        use std::process::{Command, Stdio};
        use std::time::Duration;

        let test_target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("test-target")
            .join("network_test_target.py");
        if !test_target.exists() {
            eprintln!("skipping: {} not found", test_target.display());
            return;
        }
        if Command::new("which").arg("mitmdump").output().map(|o| !o.status.success()).unwrap_or(true) {
            eprintln!("skipping: mitmdump not on PATH");
            return;
        }

        // The target process under test.
        let mut target = Command::new("python3")
            .arg(&test_target)
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn network_test_target.py");
        let target_pid = target.id();

        // An *unrelated* process making its own HTTP request during the
        // same capture window — must never show up in the target's flows.
        let mut unrelated = Command::new("python3")
            .arg(&test_target)
            .arg("serve")
            // Different ports so this doesn't crash on a bind conflict
            // with the primary target's own servers.
            .env("NT_HTTP_PORT", "18765")
            .env("NT_HTTPS_PORT", "18766")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn unrelated network_test_target.py");

        tokio::time::sleep(Duration::from_millis(800)).await;

        let process = crate::providers::SysinfoProcessProvider::new();
        let socket = crate::providers::NetstatSocketProvider;
        let dns = crate::providers::ReverseDnsProvider;
        let traffic = crate::providers::MitmproxyTrafficProvider::default();
        let mut engine = ObservationEngine::new(
            Box::new(process),
            Box::new(socket),
            Arc::new(dns),
            Box::new(traffic),
        );

        let start_status = engine.start_traffic_capture(target_pid);
        assert_eq!(
            start_status.state,
            crate::models::ProviderState::Observed,
            "capture must start cleanly: {:?}",
            start_status.reason
        );

        // Let the addon's IPC connection establish, then observe both
        // processes' sockets (populates NetworkConnection for correlation)
        // while their scenario loops run several real HTTP requests each.
        // `network_test_target.py serve` cycles through 9 scenarios at 3s
        // apart, and `long_lived` alone sleeps 6s internally before it does
        // anything — the `http` scenario (the first one this test can
        // actually correlate) isn't reached until roughly 21s in, so this
        // has to be patient, not fast.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut found_match = false;
        let mut saw_any_flow = false;
        let mut saw_real_http_method = false;
        for _ in 0..45 {
            engine.get_connections(target_pid);
            let polled = engine.poll_traffic_flows();
            for (flow, matched) in polled {
                saw_any_flow = true;
                // "see the raw flow show up in a debug log" — the actual
                // Phase 0.3 demo checkpoint wording (docs/[9] TODO.md).
                println!(
                    "DEBUG LOG: {} {}{} -> matched connection: {:?} (evidence: pid={:?} remote={:?}:{:?})",
                    flow.request.method,
                    flow.request.host,
                    flow.request.path,
                    matched,
                    flow.evidence.pid,
                    flow.evidence.remote_addr,
                    flow.evidence.remote_port,
                );
                assert_eq!(
                    flow.evidence.pid,
                    Some(target_pid),
                    "every captured flow must be attributed to the targeted pid only"
                );
                if ["GET", "POST"].contains(&flow.request.method.as_str()) {
                    saw_real_http_method = true;
                }
                if matched.is_some() {
                    found_match = true;
                }
            }
            if found_match {
                break;
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }

        engine.stop_traffic_capture();
        let _ = target.kill();
        let _ = unrelated.kill();
        let _ = target.wait();
        let _ = unrelated.wait();
        // best-effort cleanup of the redirector helper this test's capture
        // session started, mirroring the manual spike's cleanup.
        let _ = Command::new("pkill").arg("-f").arg("Mitmproxy Redirector").status();

        assert!(saw_any_flow, "expected at least one real captured HTTP flow");
        assert!(saw_real_http_method, "captured flow's request.method must be a real HTTP verb");
        assert!(
            found_match,
            "expected at least one captured flow to correlate to a real tracked NetworkConnection"
        );
    }
}
