use skillz_core::{Agent, ScanContext};
use std::path::PathBuf;

/// Scans every supported agent. Runs off the main thread because it probes binaries.
#[tauri::command]
async fn scan_agents(projects: Vec<String>) -> Vec<Agent> {
    let ctx = ScanContext::from_env(projects.into_iter().map(PathBuf::from).collect());
    skillz_core::scan(&ctx)
}

/// Reads one asset file for the viewer. Editing comes in Phase 2.
#[tauri::command]
fn read_asset(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan_agents, read_asset])
        .run(tauri::generate_context!())
        .expect("error while running Skillz");
}
