//! Live-monitoring commands and the polling loop itself. See
//! `docs/[9] TODO.md` Phase 0.2 ("Add polling loop with configurable
//! interval... start/stop controls in UI") and `docs/DATA_MODEL.md` for the
//! `TrafficEvent`s this loop emits on lifecycle transitions.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::models::{MonitoringState, MonitoringStatus, ProcessState};

use super::EngineHandle;

const DEFAULT_POLL_INTERVAL_MS: u64 = 2000;

pub struct MonitoringController {
    task: Mutex<Option<JoinHandle<()>>>,
    status: Mutex<MonitoringStatus>,
}

impl Default for MonitoringController {
    fn default() -> Self {
        Self {
            task: Mutex::new(None),
            status: Mutex::new(MonitoringStatus {
                state: MonitoringState::Idle,
                pid: None,
                reason: None,
            }),
        }
    }
}

/// `Arc`, same reasoning as `EngineHandle` — commands hand a clone of this
/// into the spawned polling task so it can update state after the command
/// that started it has already returned.
pub type MonitoringHandle = Arc<MonitoringController>;

async fn stop_task(controller: &MonitoringController) {
    if let Some(handle) = controller.task.lock().await.take() {
        handle.abort();
    }
}

async fn set_status(controller: &MonitoringController, app: &AppHandle, status: MonitoringStatus) {
    *controller.status.lock().await = status.clone();
    let _ = app.emit("monitoring-state-changed", &status);
}

#[tauri::command]
pub async fn start_monitoring(
    pid: u32,
    interval_ms: Option<u64>,
    engine: State<'_, EngineHandle>,
    controller: State<'_, MonitoringHandle>,
    app: AppHandle,
) -> Result<MonitoringStatus, ()> {
    // Only one monitored process at a time — starting a new session
    // implicitly replaces any previous one.
    stop_task(&controller).await;

    let interval_ms = interval_ms.unwrap_or(DEFAULT_POLL_INTERVAL_MS);
    engine.lock().await.set_poll_interval_ms(interval_ms);

    let engine_handle = engine.inner().clone();
    let app_handle = app.clone();
    let controller_handle = controller.inner().clone();

    let handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(StdDuration::from_millis(interval_ms)).await;

            let (processes, connections, events) = {
                let mut engine = engine_handle.lock().await;
                let processes = engine.get_processes();
                let connections = engine.get_connections(pid);
                let events = engine.take_pending_events();
                (processes, connections, events)
            };

            let _ = app_handle.emit("processes-updated", &processes);
            let _ = app_handle.emit("connections-updated", &connections);
            for event in &events {
                let _ = app_handle.emit("timeline-event", event);
            }
            spawn_dns_lookups(engine_handle.clone(), app_handle.clone());

            let monitored_process_exited = processes
                .data
                .as_ref()
                .and_then(|list| list.iter().find(|p| p.pid == pid))
                .map(|p| p.process_state == ProcessState::Exited)
                .unwrap_or(false);

            if monitored_process_exited {
                *controller_handle.task.lock().await = None;
                set_status(
                    &controller_handle,
                    &app_handle,
                    MonitoringStatus {
                        state: MonitoringState::Stopped,
                        pid: Some(pid),
                        reason: Some("process exited".to_string()),
                    },
                )
                .await;
                break;
            }
        }
    });

    *controller.task.lock().await = Some(handle);
    let status = MonitoringStatus {
        state: MonitoringState::Running,
        pid: Some(pid),
        reason: None,
    };
    set_status(&controller, &app, status.clone()).await;
    Ok(status)
}

#[tauri::command]
pub async fn stop_monitoring(
    controller: State<'_, MonitoringHandle>,
    app: AppHandle,
) -> Result<MonitoringStatus, ()> {
    stop_task(&controller).await;
    let status = MonitoringStatus {
        state: MonitoringState::Stopped,
        pid: None,
        reason: Some("stopped by user".to_string()),
    };
    set_status(&controller, &app, status.clone()).await;
    Ok(status)
}

#[tauri::command]
pub async fn get_monitoring_status(
    controller: State<'_, MonitoringHandle>,
) -> Result<MonitoringStatus, ()> {
    Ok(controller.status.lock().await.clone())
}

/// Resolves every pending reverse-DNS lookup in the background — each
/// address gets its own spawned task so a slow lookup for one address
/// doesn't delay the others, and the blocking OS resolver call
/// (`spawn_blocking`) never runs while holding the engine lock. Called
/// after every connections refresh, live-monitored or manual.
pub fn spawn_dns_lookups(engine: EngineHandle, app: AppHandle) {
    tokio::spawn(async move {
        let (addrs, dns_provider) = {
            let mut engine = engine.lock().await;
            (engine.take_pending_dns_lookups(), engine.dns_provider())
        };

        for addr in addrs {
            let engine = engine.clone();
            let app = app.clone();
            let dns_provider = dns_provider.clone();
            tokio::spawn(async move {
                let lookup_addr = addr.clone();
                let (observation, _status) =
                    tokio::task::spawn_blocking(move || dns_provider.resolve(&lookup_addr))
                        .await
                        .unwrap_or((None, crate::models::ProviderStatus::observed(chrono::Utc::now())));
                engine.lock().await.record_hostname(addr, observation);
                let _ = app.emit("hostnames-updated", ());
            });
        }
    });
}
