mod commands;
mod engine;
mod models;
mod providers;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use commands::{EngineHandle, MonitoringController, MonitoringHandle};
use engine::ObservationEngine;
use providers::{NetstatSocketProvider, ReverseDnsProvider, SysinfoProcessProvider};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = ObservationEngine::new(
        Box::new(SysinfoProcessProvider::new()),
        Box::new(NetstatSocketProvider),
        Arc::new(ReverseDnsProvider),
    );

    tauri::Builder::default()
        .manage(EngineHandle::new(tokio::sync::Mutex::new(engine)))
        .manage(MonitoringHandle::new(MonitoringController::default()))
        .invoke_handler(tauri::generate_handler![
            commands::get_processes,
            commands::get_connections,
            commands::get_hostnames,
            commands::get_timeline,
            commands::monitoring::start_monitoring,
            commands::monitoring::stop_monitoring,
            commands::monitoring::get_monitoring_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
