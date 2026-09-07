//! Tauri `invoke` command handlers exposed to the frontend. Per
//! `docs/ARCHITECTURE.md`: command handlers contain no provider or
//! correlation logic — call the Engine, serialize, return.

// `pub(crate)`, not private: `tauri::generate_handler!` in lib.rs needs to
// reference `commands::monitoring::start_monitoring` etc. by their exact
// definition path — the `#[tauri::command]` macro generates hidden sibling
// items alongside each function that a `pub use` re-export would not carry.
pub(crate) mod monitoring;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::engine::ObservationEngine;
use crate::models::{
    Envelope, NetworkConnection, ObservationCapabilities, ProcessInfo, ProviderStatus,
    ResolvedHostname, TrafficEvent,
};

pub use monitoring::{MonitoringController, MonitoringHandle};

/// `Arc` (not a bare `Mutex`) because the live-monitoring task
/// (`monitoring.rs`) needs to hold it across `.await` points in a spawned
/// `'static` task, outliving any single command invocation's borrowed
/// `State`.
pub type EngineHandle = Arc<Mutex<ObservationEngine>>;

#[tauri::command]
pub async fn get_processes(state: State<'_, EngineHandle>) -> Result<Envelope<Vec<ProcessInfo>>, ()> {
    // The `Mutex` also satisfies Phase 0.1's "prevent overlapping polling
    // cycles" requirement (`docs/[9] TODO.md`): a second concurrent refresh
    // queues behind this lock rather than racing the first.
    let mut engine = state.lock().await;
    Ok(engine.get_processes())
}

#[tauri::command]
pub async fn get_connections(
    pid: u32,
    state: State<'_, EngineHandle>,
    app: AppHandle,
) -> Result<Envelope<Vec<NetworkConnection>>, ()> {
    let envelope = {
        let mut engine = state.lock().await;
        engine.get_connections(pid)
    };
    let _ = app.emit("connections-updated", &envelope);
    // Any newly-seen remote addresses are resolved in the background
    // regardless of whether live monitoring is running — a manual refresh
    // should populate hostnames too, just without live updates afterward.
    monitoring::spawn_dns_lookups(state.inner().clone(), app);
    Ok(envelope)
}

#[tauri::command]
pub async fn get_hostnames(
    pid: u32,
    state: State<'_, EngineHandle>,
) -> Result<Envelope<Vec<ResolvedHostname>>, ()> {
    let engine = state.lock().await;
    Ok(engine.get_hostnames(pid))
}

#[tauri::command]
pub async fn get_timeline(
    pid: u32,
    state: State<'_, EngineHandle>,
) -> Result<Envelope<Vec<TrafficEvent>>, ()> {
    let engine = state.lock().await;
    Ok(engine.get_timeline(pid))
}

/// `get_capabilities()` (`docs/[9] TODO.md` Phase 0.3) — what each provider
/// can *ever* observe, aggregated per provider, distinct from any single
/// connection's `ObservationStatus` (`docs/DATA_MODEL.md`).
#[tauri::command]
pub async fn get_capabilities(state: State<'_, EngineHandle>) -> Result<ObservationCapabilities, ()> {
    let engine = state.lock().await;
    Ok(engine.get_capabilities())
}

/// `start`/`stop` on the traffic provider block their calling thread for up
/// to a few seconds (`MitmproxyTrafficProvider::terminate_child` tearing
/// down any previous `mitmdump` session) — same "never block while holding
/// the engine lock" rule `spawn_dns_lookups` follows for the blocking DNS
/// resolver call. The engine lock is held only long enough to clone the
/// provider handle; the actual start/stop runs on a `spawn_blocking` thread
/// afterward, so it no longer stalls every other command (including a
/// concurrent `get_processes`/`get_connections`) for the duration.
#[tauri::command]
pub async fn start_traffic_capture(pid: u32, state: State<'_, EngineHandle>) -> Result<ProviderStatus, ()> {
    let provider = state.lock().await.traffic_provider();
    tokio::task::spawn_blocking(move || provider.start(pid))
        .await
        .map_err(|_| ())
}

#[tauri::command]
pub async fn stop_traffic_capture(state: State<'_, EngineHandle>) -> Result<(), ()> {
    let provider = state.lock().await.traffic_provider();
    tokio::task::spawn_blocking(move || provider.stop())
        .await
        .map_err(|_| ())
}

/// Ongoing health of the current (or most recent) capture session — Phase
/// 0.3 code-review gap 4/6 (`docs/[9] TODO.md`). Distinct from
/// `start_traffic_capture`'s return value, which only reports whether the
/// helper process was spawned; this reflects whether it's actually still
/// connected and streaming, so the frontend can distinguish a session
/// stuck on an unapproved macOS prompt (or one that crashed mid-session)
/// from one that's genuinely working.
#[tauri::command]
pub async fn get_traffic_status(state: State<'_, EngineHandle>) -> Result<ProviderStatus, ()> {
    let engine = state.lock().await;
    Ok(engine.traffic_status())
}

/// Drains and correlates whatever the traffic provider has captured since
/// the last poll — the Phase 0.3 "debug log" the demo checkpoint asks for
/// (`docs/[9] TODO.md`). Not yet wired into `NetworkConnection`/
/// `get_connections` output or emitted as an event; that Engine-attachment
/// step is explicitly Phase 0.4, once `HTTPRequest`/`HTTPResponse` and the
/// `Redactor` exist to produce what would actually get attached.
#[tauri::command]
pub async fn poll_traffic_flows(
    state: State<'_, EngineHandle>,
) -> Result<Vec<(crate::providers::CapturedFlow, Option<String>)>, ()> {
    let engine = state.lock().await;
    Ok(engine.poll_traffic_flows())
}
