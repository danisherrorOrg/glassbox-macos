//! Tauri `invoke` command handlers exposed to the frontend. Per
//! `docs/ARCHITECTURE.md`: command handlers contain no provider or
//! correlation logic — call the Engine, serialize, return.

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::engine::ObservationEngine;
use crate::models::{Envelope, NetworkConnection, ProcessInfo};

pub type EngineState = Mutex<ObservationEngine>;

#[tauri::command]
pub async fn get_processes(state: State<'_, EngineState>) -> Result<Envelope<Vec<ProcessInfo>>, ()> {
    // The `Mutex` also satisfies Phase 0.1's "prevent overlapping polling
    // cycles" requirement (`docs/[9] TODO.md`): a second concurrent refresh
    // queues behind this lock rather than racing the first.
    let mut engine = state.lock().await;
    Ok(engine.get_processes())
}

#[tauri::command]
pub async fn get_connections(
    pid: u32,
    state: State<'_, EngineState>,
    app: AppHandle,
) -> Result<Envelope<Vec<NetworkConnection>>, ()> {
    let envelope = {
        let mut engine = state.lock().await;
        engine.get_connections(pid)
    };
    // Live polling isn't wired up until Phase 0.2 ("Live Monitoring") — this
    // emit exercises the event-stream mechanism `docs/[9] TODO.md` Phase 0.1
    // asks for now, scoped to the selected process, so 0.2 only has to add
    // the polling loop that calls it repeatedly, not invent it.
    let _ = app.emit("connections-updated", &envelope);
    Ok(envelope)
}
