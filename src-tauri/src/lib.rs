// This crate calls macOS-only libproc/proc_pidinfo FFI unconditionally
// (providers/system.rs, providers/process.rs) — a non-macOS build would
// otherwise fail with opaque linker errors far from the actual cause.
// See docs/PERMISSIONS_AND_PLATFORM.md: this project is macOS-only by
// design, not an oversight that happens to work only on one platform.
#[cfg(not(target_os = "macos"))]
compile_error!("this crate only supports macOS — see docs/PERMISSIONS_AND_PLATFORM.md");

mod commands;
mod engine;
mod models;
mod providers;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use commands::{EngineHandle, MonitoringController, MonitoringHandle};
use engine::ObservationEngine;
use providers::{
    MitmproxyTrafficProvider, NetstatSocketProvider, ReverseDnsProvider, SysinfoProcessProvider,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = ObservationEngine::new(
        Box::new(SysinfoProcessProvider::new()),
        Box::new(NetstatSocketProvider),
        Arc::new(ReverseDnsProvider),
        Arc::new(MitmproxyTrafficProvider::default()),
    );

    tauri::Builder::default()
        .manage(EngineHandle::new(tokio::sync::Mutex::new(engine)))
        .manage(MonitoringHandle::new(MonitoringController::default()))
        .invoke_handler(tauri::generate_handler![
            commands::get_processes,
            commands::get_connections,
            commands::get_hostnames,
            commands::get_timeline,
            commands::get_capabilities,
            commands::start_traffic_capture,
            commands::stop_traffic_capture,
            commands::get_traffic_status,
            commands::poll_traffic_flows,
            commands::monitoring::start_monitoring,
            commands::monitoring::stop_monitoring,
            commands::monitoring::get_monitoring_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
