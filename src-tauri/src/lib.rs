mod commands;
mod engine;
mod models;
mod providers;
#[cfg(test)]
mod tests;

use commands::EngineState;
use engine::ObservationEngine;
use providers::{NetstatSocketProvider, SysinfoProcessProvider};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = ObservationEngine::new(
        Box::new(SysinfoProcessProvider),
        Box::new(NetstatSocketProvider),
    );

    tauri::Builder::default()
        .manage(EngineState::new(engine))
        .invoke_handler(tauri::generate_handler![
            commands::get_processes,
            commands::get_connections,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
