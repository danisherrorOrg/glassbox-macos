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

    /// Ongoing health of the current (or most recent) capture session —
    /// distinct from `start`'s return value, which only reports whether
    /// the helper process was *spawned* successfully. Phase 0.3
    /// code-review gap 4/6 (`docs/[9] TODO.md`): `start()` used to report
    /// `Observed` as soon as `mitmdump` spawned, before the addon had
    /// actually connected over IPC — so a session stuck on an unapproved
    /// macOS system-extension/VPN/Keychain prompt looked identical to a
    /// working one, and a mid-session crash was invisible to any caller.
    /// This reflects the reader task's real state: still `Observed` only
    /// once the IPC `accept()` has actually succeeded, `Unavailable`/
    /// `TransientFailure` once the reader task has exited for any reason.
    fn status(&self) -> ProviderStatus;

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
    // Same `Arc` reasoning as `flows` — the reader task updates this as the
    // session's real state changes (gap 4/6, see `TrafficProvider::status`).
    status: Arc<Mutex<ProviderStatus>>,
}

impl MitmproxyTrafficProvider {
    /// How long to give `mitmdump` to exit cleanly after `SIGTERM` before
    /// escalating to `SIGKILL`. Phase 0.3 code-review gap 3/6
    /// (`docs/[9] TODO.md`): the previous implementation went straight to
    /// `SIGKILL` (via `Child::kill`) and didn't wait for it at all, so
    /// `start()` could bind a new socket and spawn a fresh `mitmdump`
    /// while the old one (and its `Mitmproxy Redirector.app` child, which
    /// `SIGKILL` never gives a chance to clean up after itself) was still
    /// alive — confirmed as a real, not theoretical, problem: building
    /// `real_traffic_capture_correlates_to_real_connection` required a
    /// manual `pkill -f "Mitmproxy Redirector"` as test cleanup because of
    /// exactly this.
    const SIGTERM_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

    /// Terminates `child` and blocks until it has actually exited (or the
    /// grace period elapses and `SIGKILL` is used instead), so that by the
    /// time this returns, the pid is free and `mitmdump`'s own child
    /// process is gone too. `Child::try_wait` is non-blocking, so this
    /// polls it with small sleeps rather than needing `.await` — `stop()`
    /// stays a plain synchronous `&self` method on the `TrafficProvider`
    /// trait rather than requiring `async-trait` for this alone (the
    /// "genuinely await the previous session's exit" alternative the gap
    /// write-up flagged as the bigger, not-yet-justified change).
    ///
    /// This blocks whatever thread calls it for up to roughly
    /// `SIGTERM_GRACE` (worst case, if `mitmdump` ignores `SIGTERM`
    /// entirely). Deliberately not offloaded via `block_in_place` —that
    /// requires a multi-threaded Tokio runtime and panics on a
    /// current-thread one (as `#[tokio::test]` defaults to), which would
    /// make this helper's behavior depend on which runtime happened to be
    /// active. `start`/`stop` are rare, user-initiated actions (not a hot
    /// path), so a plain bounded block is the simpler, more robust choice.
    fn terminate_child(mut child: Child) {
        if let Some(pid) = child.id() {
            // SAFETY: `pid` is this child's own pid, still alive (we
            // haven't reaped it yet) and owned by this process's
            // process-group as a direct child — a valid target for
            // `kill(2)`.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        }
        let deadline = std::time::Instant::now() + Self::SIGTERM_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return, // exited cleanly on its own
                Err(_) => return,      // already gone / unwaitable — nothing more to do
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        // Grace period elapsed and it's still alive -- escalate.
        let _ = child.start_kill();
        // Reap it so it doesn't linger as a zombie; bounded the same way,
        // though SIGKILL should be near-instant in practice.
        let reap_deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < reap_deadline {
            if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

impl Default for MitmproxyTrafficProvider {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            flows: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(Mutex::new(ProviderStatus::unavailable(
                Utc::now(),
                "capture not started",
            ))),
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
    duration_ms: Option<f64>,
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
            // Never log the raw line: it can carry tier-2 content (cookies,
            // bodies, query strings) that's deliberately not redacted at
            // the addon layer, only tier-1 is -- logging it here would
            // violate `PRIVACY_AND_SECURITY.md`'s "no captured request/
            // response body or header content goes into println!/log/
            // application logging at any level" rule (Phase 0.3
            // code-review gap 5/6, docs/[9] TODO.md). The parse error and
            // the line's length are enough to debug a malformed-event bug
            // without ever printing captured content.
            eprintln!("traffic provider: malformed IPC event ({e}), {} bytes", line.len());
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
            Err(e) => {
                let status = ProviderStatus::transient_failure(now, format!("temp dir: {e}"));
                *self.status.lock().unwrap() = status.clone();
                return status;
            }
        };
        let addon_path = temp_dir.path().join("mitm_addon.py");
        let fields_path = temp_dir.path().join("tier1_redaction_fields.json");
        let socket_path: PathBuf = temp_dir.path().join("ipc.sock");

        if let Err(e) = std::fs::write(&addon_path, ADDON_SCRIPT) {
            let status = ProviderStatus::transient_failure(now, format!("write addon script: {e}"));
            *self.status.lock().unwrap() = status.clone();
            return status;
        }
        if let Err(e) = std::fs::write(&fields_path, TIER1_FIELDS_JSON) {
            let status = ProviderStatus::transient_failure(now, format!("write fields json: {e}"));
            *self.status.lock().unwrap() = status.clone();
            return status;
        }

        let listener = match UnixListener::bind(&socket_path) {
            Ok(l) => l,
            Err(e) => {
                let status = ProviderStatus::transient_failure(now, format!("bind IPC socket: {e}"));
                *self.status.lock().unwrap() = status.clone();
                return status;
            }
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
                let status =
                    ProviderStatus::unavailable(now, format!("spawn mitmdump: {e} — is mitmproxy installed?"));
                *self.status.lock().unwrap() = status.clone();
                return status;
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

        // Not yet `Observed` -- the helper has spawned but hasn't proven it
        // can actually stream anything yet (the addon's IPC `connect()`
        // hasn't reached this side's `accept()`). A session stuck on an
        // unapproved macOS system-extension/VPN/Keychain prompt looks
        // exactly like this until (if ever) it resolves.
        *self.status.lock().unwrap() =
            ProviderStatus::transient_failure(now, "waiting for helper process to connect");

        let flows = self.flows.clone();
        let status = self.status.clone();
        let reader_task = tokio::spawn(async move {
            let stream = match listener.accept().await {
                Ok((s, _addr)) => s,
                Err(e) => {
                    eprintln!("traffic provider: IPC accept failed: {e}");
                    *status.lock().unwrap() =
                        ProviderStatus::unavailable(Utc::now(), format!("IPC accept failed: {e}"));
                    return;
                }
            };
            *status.lock().unwrap() = ProviderStatus::observed(Utc::now());
            let mut lines = BufReader::new(stream).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if let Some(flow) = parse_ipc_line(&line, pid) {
                            flows.lock().unwrap().push(flow);
                        }
                    }
                    Ok(None) => {
                        // addon closed the connection (mitmdump exited)
                        *status.lock().unwrap() = ProviderStatus::transient_failure(
                            Utc::now(),
                            "mitmdump exited (IPC connection closed)",
                        );
                        break;
                    }
                    Err(e) => {
                        eprintln!("traffic provider: IPC read error: {e}");
                        *status.lock().unwrap() =
                            ProviderStatus::transient_failure(Utc::now(), format!("IPC read error: {e}"));
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
            // `abort()` cancels the reader task at its next await point --
            // it never reaches its own exit-path status updates, so this
            // is the only place a deliberate stop gets reflected in
            // `status()` rather than the task's last update lingering
            // (e.g. still `Observed`) after capture has actually ended.
            *self.status.lock().unwrap() = ProviderStatus::unavailable(Utc::now(), "capture stopped");
            Self::terminate_child(session.child);
        }
    }

    fn take_flows(&self) -> Vec<CapturedFlow> {
        std::mem::take(&mut self.flows.lock().unwrap())
    }

    fn status(&self) -> ProviderStatus {
        self.status.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real, not mocked: exercises the actual `stop()`/`start()` race from
    /// Phase 0.3 code-review gap 3/6 (`docs/[9] TODO.md`) against a real
    /// `mitmdump` child process. Before the `SIGTERM`-then-wait fix in
    /// `terminate_child`, `stop()` fired an unawaited `SIGKILL` and
    /// returned immediately, so a rapid stop/start/stop cycle like this
    /// one reliably left `mitmdump`/`Mitmproxy Redirector.app` processes
    /// running — confirmed during that gap's implementation, which needed
    /// a manual `pkill -f "Mitmproxy Redirector"` as test cleanup because
    /// of exactly this. Requires `mitmdump` installed and the one-time
    /// macOS approvals already granted (`docs/PERMISSIONS_AND_PLATFORM.md`),
    /// same environment-dependent-but-real standard as the engine's
    /// `real_traffic_capture_correlates_to_real_connection`.
    #[tokio::test]
    async fn stop_leaves_no_lingering_mitmdump_process() {
        if std::process::Command::new("which")
            .arg("mitmdump")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            eprintln!("skipping: mitmdump not on PATH");
            return;
        }

        let provider = MitmproxyTrafficProvider::default();
        let pid = std::process::id();
        for _ in 0..3 {
            let status = provider.start(pid);
            assert_eq!(
                status.state,
                crate::models::ProviderState::Observed,
                "capture must start cleanly: {:?}",
                status.reason
            );
            // Let mitmdump actually spawn (and the redirector come up)
            // before tearing it down again -- stopping instantly after
            // spawn wouldn't exercise the same shutdown path a real
            // session does.
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            provider.stop();
            // `stop()` now blocks (via `terminate_child`) until the child
            // has actually exited or been force-killed, so nothing further
            // to wait for here -- that's exactly the guarantee under test.
        }

        let output = std::process::Command::new("pgrep").arg("-f").arg("mitmdump").output();
        if let Ok(out) = output {
            let leftover = String::from_utf8_lossy(&out.stdout);
            assert!(
                leftover.trim().is_empty(),
                "expected no leftover mitmdump processes after stop(), found pid(s): {leftover}"
            );
        }
    }

    /// Phase 0.3 code-review gap 4/6 (`docs/[9] TODO.md`): `status()` must
    /// track the session's real lifecycle, not just mirror `start()`'s
    /// one-shot "did it spawn" answer. Before this gap's fix there was no
    /// `status()` at all, so a session stuck on an unapproved macOS
    /// prompt was indistinguishable from a working one.
    #[tokio::test]
    async fn status_reflects_real_session_lifecycle() {
        if std::process::Command::new("which")
            .arg("mitmdump")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            eprintln!("skipping: mitmdump not on PATH");
            return;
        }

        let provider = MitmproxyTrafficProvider::default();
        assert_eq!(
            provider.status().state,
            crate::models::ProviderState::Unavailable,
            "before any session, status must not claim Observed"
        );

        let pid = std::process::id();
        let start_status = provider.start(pid);
        assert_eq!(start_status.state, crate::models::ProviderState::Observed);

        // Give the addon time to actually connect over IPC.
        let mut became_observed = false;
        for _ in 0..20 {
            if provider.status().state == crate::models::ProviderState::Observed {
                became_observed = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        assert!(
            became_observed,
            "status() must become Observed once the reader task's accept() succeeds, got {:?}",
            provider.status()
        );

        provider.stop();
        assert_ne!(
            provider.status().state,
            crate::models::ProviderState::Observed,
            "status() must not still claim Observed once the session has been deliberately stopped"
        );
    }
}
