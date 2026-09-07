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
use crate::models::{Envelope, NetworkConnection, ProcessInfo, ResolvedHostname, TrafficEvent};

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
