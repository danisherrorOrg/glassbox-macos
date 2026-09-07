//! `TrafficProvider` — a `mitmdump`-backed implementation, per
//! `docs/[9] TODO.md` Phase 0.3 and `docs/ARCHITECTURE.md` ("a
//! mitmproxy-based helper process (Python), launched and managed by the
//! Rust core").
//!
//! The core-to-helper boundary: the Rust core binds a Unix domain socket,
//! writes the addon script + shared tier-1 field list (both embedded in
//! this binary via `include_str!`, not a separate packaged resource this
//! phase — see the module-level note in `start`) to a session-scoped temp
//! directory, and spawns `mitmdump -s <addon> --mode local:<pid>` with the
//! socket path passed via environment variable. The addon connects once
//! and streams one newline-delimited JSON object per captured flow. This
//! is deliberately one-directional (addon → core) and read-only in intent:
//! the core never sends anything back that could influence what the addon
//! does to a flow (`docs/[9] TODO.md`: "no `intercept()`, no `set()`, no
//! replay hooks, by construction").
//!
//! **Upstream TLS verification is left at mitmproxy's secure default (no
//! `--ssl-insecure`\)** — deliberately, even though that means captures
//! against a self-signed test server (e.g. `test-target`'s HTTPS server)
//! show as capture errors rather than decrypted flows. This is a passive
//! observation tool; silently disabling upstream certificate verification
//! would hide a genuine MITM attack on the very traffic being observed
//! from the user, in exchange for convenience testing against one script's
//! self-signed cert. Real external HTTPS (valid CA-signed certs) is
//! unaffected and was verified working during the Phase 0.3 spike
//! (`docs/PERMISSIONS_AND_PLATFORM.md`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use crate::models::{
    CorrelationEvidence, LimitedAvailability, ProviderCapabilities, ProviderStatus,
    RawHTTPRequest, RawHTTPResponse,
};

const ADDON_SCRIPT: &str = include_str!("../../resources/mitm_addon.py");
const TIER1_FIELDS_JSON: &str = include_str!("../../resources/tier1_redaction_fields.json");

#[derive(Debug, Clone, serde::Serialize)]
pub struct CapturedFlow {
    pub request: RawHTTPRequest,
    pub response: Option<RawHTTPResponse>,
    pub evidence: CorrelationEvidence,
}

pub trait TrafficProvider: Send + Sync {
    /// Starts capturing traffic for `pid`. Synchronous kickoff — spawns the
    /// helper process and a background reader task internally (needs an
    /// active `tokio` runtime, which every caller already runs inside).
    /// Replaces any previous session, same one-target-at-a-time semantics
    /// as live monitoring (`commands/monitoring.rs`).
    fn start(&self, pid: u32) -> ProviderStatus;

    fn stop(&self);

    /// Non-blocking: drains whatever flows have arrived since the last call.
    fn take_flows(&self) -> Vec<CapturedFlow>;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            http_metadata: Some(LimitedAvailability::Available),
            https_metadata: Some(LimitedAvailability::Available),
            request_body: Some(LimitedAvailability::Available),
            response_body: Some(LimitedAvailability::Available),
            // Not pursued unless Phase 2's Network Extension upgrade
            // happens (`docs/DATA_MODEL.md`) — this is the mitmproxy-based
            // ceiling, not a gap to close within this phase.
            raw_packet_data: Some(crate::models::Availability::Unavailable),
            ..Default::default()
        }
    }
}

struct CaptureSession {
    child: Child,
    reader_task: JoinHandle<()>,
    _temp_dir: tempfile::TempDir, // held for its Drop — cleans up the socket/addon files
}

pub struct MitmproxyTrafficProvider {
    session: Mutex<Option<CaptureSession>>,
    // `Arc`, not a bare `Mutex` field, so the spawned reader task
    // (`'static`, outliving any single `start()` call) can hold its own
    // clone of the handle safely — no unsafe lifetime tricks needed.
    flows: Arc<Mutex<Vec<CapturedFlow>>>,
}

impl Default for MitmproxyTrafficProvider {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            flows: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// ---- IPC wire format — mirrors resources/mitm_addon.py's JSON exactly ----

#[derive(Debug, Deserialize)]
struct IpcRequest {
    method: String,
    host: String,
    path: String,
    headers: HashMap<String, String>,
    body: Option<String>,
    timestamp: f64,
}

#[derive(Debug, Deserialize)]
struct IpcResponse {
    status_code: u16,
    headers: HashMap<String, String>,
    body: Option<String>,
    duration_ms: f64,
}

#[derive(Debug, Deserialize)]
struct IpcEvidence {
    protocol: Option<String>,
    local_addr: Option<String>,
    local_port: Option<u16>,
    remote_addr: Option<String>,
    remote_port: Option<u16>,
    hostname: Option<String>,
    timestamp: f64,
    source: String,
}

#[derive(Debug, Deserialize)]
struct IpcEvent {
    request: IpcRequest,
    response: Option<IpcResponse>,
    evidence: IpcEvidence,
}

fn unix_time_to_datetime(secs: f64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs.trunc() as i64, ((secs.fract()) * 1_000_000_000.0) as u32)
        .unwrap_or_else(Utc::now)
}

fn parse_ipc_line(line: &str, pid: u32) -> Option<CapturedFlow> {
    let event: IpcEvent = match serde_json::from_str(line) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("traffic provider: malformed IPC event ({e}): {line}");
            return None;
        }
    };

    let request = RawHTTPRequest {
        method: event.request.method,
        host: event.request.host,
        path: event.request.path,
        headers: event.request.headers,
        body: event.request.body,
        timestamp: unix_time_to_datetime(event.request.timestamp),
    };
    let response = event.response.map(|r| RawHTTPResponse {
        status_code: r.status_code,
        headers: r.headers,
        body: r.body,
        duration_ms: r.duration_ms,
    });
    // `pid` is filled in here, not read from the addon's event — the addon
    // has no reliable way to know it (see the module-level doc comment and
    // the Phase 0.3 spike finding in PERMISSIONS_AND_PLATFORM.md), but the
    // Rust core already knows it: it's the pid this whole capture session
    // was started for.
    let evidence = CorrelationEvidence {
        pid: Some(pid),
        protocol: event.evidence.protocol,
        local_addr: event.evidence.local_addr,
        local_port: event.evidence.local_port,
        remote_addr: event.evidence.remote_addr,
        remote_port: event.evidence.remote_port,
        hostname: event.evidence.hostname,
        timestamp: unix_time_to_datetime(event.evidence.timestamp),
        source: event.evidence.source,
    };

    Some(CapturedFlow {
        request,
        response,
        evidence,
    })
}

impl TrafficProvider for MitmproxyTrafficProvider {
    fn start(&self, pid: u32) -> ProviderStatus {
        let now = Utc::now();
        self.stop();

        let temp_dir = match tempfile::Builder::new().prefix("pni-traffic-").tempdir() {
            Ok(d) => d,
            Err(e) => return ProviderStatus::transient_failure(now, format!("temp dir: {e}")),
        };
        let addon_path = temp_dir.path().join("mitm_addon.py");
        let fields_path = temp_dir.path().join("tier1_redaction_fields.json");
        let socket_path: PathBuf = temp_dir.path().join("ipc.sock");

        if let Err(e) = std::fs::write(&addon_path, ADDON_SCRIPT) {
            return ProviderStatus::transient_failure(now, format!("write addon script: {e}"));
        }
        if let Err(e) = std::fs::write(&fields_path, TIER1_FIELDS_JSON) {
            return ProviderStatus::transient_failure(now, format!("write fields json: {e}"));
        }

        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => l,
            Err(e) => return ProviderStatus::transient_failure(now, format!("bind IPC socket: {e}")),
        };

        let mut child = match Command::new("mitmdump")
            .arg("-q")
            .arg("-s")
            .arg(&addon_path)
            .arg("--mode")
            .arg(format!("local:{pid}"))
            .env("PNI_IPC_SOCKET_PATH", &socket_path)
            .env("PNI_TIER1_FIELDS_PATH", &fields_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return ProviderStatus::unavailable(now, format!("spawn mitmdump: {e} — is mitmproxy installed?"))
            }
        };

        // Never leave a piped child stdio unconsumed: once the OS pipe
        // buffer fills, the child blocks on its next write to it and the
        // whole capture silently stalls. This also surfaces mitmdump's own
        // diagnostics (startup errors, the redirector's approval-state
        // messages) instead of discarding them.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("mitmdump: {line}");
                }
            });
        }

        let flows = self.flows.clone();
        let reader_task = tokio::spawn(async move {
            let stream = match listener.accept().await {
                Ok((s, _addr)) => s,
                Err(e) => {
                    eprintln!("traffic provider: IPC accept failed: {e}");
                    return;
                }
            };
            let mut lines = BufReader::new(stream).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if let Some(flow) = parse_ipc_line(&line, pid) {
                            flows.lock().unwrap().push(flow);
                        }
                    }
                    Ok(None) => break, // addon closed the connection (mitmdump exited)
                    Err(e) => {
                        eprintln!("traffic provider: IPC read error: {e}");
                        break;
                    }
                }
            }
        });

        *self.session.lock().unwrap() = Some(CaptureSession {
            child,
            reader_task,
            _temp_dir: temp_dir,
        });

        ProviderStatus::observed(now)
    }

    fn stop(&self) {
        if let Some(session) = self.session.lock().unwrap().take() {
            session.reader_task.abort();
            let mut child = session.child;
            tokio::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }

    fn take_flows(&self) -> Vec<CapturedFlow> {
        std::mem::take(&mut self.flows.lock().unwrap())
    }
}
