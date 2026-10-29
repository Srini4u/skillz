use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use skillz_core::{Agent, LoadStack, ScanContext, SearchHit};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

type Watcher = Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>;

#[derive(Default)]
struct AppState {
    agents: Mutex<Vec<Agent>>,
    watcher: Mutex<Option<Watcher>>,
}

/// Scans every supported agent and re-arms the file watcher on what was found.
#[tauri::command]
async fn scan_agents(app: AppHandle, state: State<'_, AppState>, projects: Vec<String>) -> Result<Vec<Agent>, String> {
    let ctx = ScanContext::from_env(projects.into_iter().map(PathBuf::from).collect());
    let agents = skillz_core::scan(&ctx);
    watch(&app, &state, &agents, &ctx.home);
    *state.agents.lock().unwrap() = agents.clone();
    Ok(agents)
}

/// The instruction and memory files each agent loads in `project`, in order.
#[tauri::command]
fn load_order(project: String) -> Vec<LoadStack> {
    skillz_core::load_order(&ScanContext::from_env(Vec::new()), Path::new(&project))
}

/// Searches names, descriptions and file contents from the last scan.
#[tauri::command]
fn search(state: State<'_, AppState>, query: String) -> Vec<SearchHit> {
    skillz_core::search(&state.agents.lock().unwrap(), &query)
}

/// Reads one asset file for the viewer. Editing comes in Phase 2.
#[tauri::command]
fn read_asset(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))
}

/// Watches the folders the agents' files live in and emits `agents-changed` when one of
/// them changes. Session logs and other churn under ~/.claude/projects are ignored.
fn watch(app: &AppHandle, state: &AppState, agents: &[Agent], home: &Path) {
    let noise = home.join(".claude").join("projects");
    let handle = app.clone();
    let debouncer = new_debouncer(Duration::from_millis(800), move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let relevant = events.iter().any(|e| {
            let p = &e.path;
            let in_noise = p.starts_with(&noise) && !p.components().any(|c| c.as_os_str() == "memory");
            let ext_ok = p.extension().map_or(true, |x| x == "md" || x == "json" || x == "toml");
            !in_noise && ext_ok
        });
        if relevant {
            let _ = handle.emit("agents-changed", ());
        }
    });
    let Ok(mut debouncer) = debouncer else { return };
    for dir in skillz_core::watch_dirs(agents) {
        let _ = debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive);
    }
    *state.watcher.lock().unwrap() = Some(debouncer);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![scan_agents, load_order, search, read_asset])
        .run(tauri::generate_context!())
        .expect("error while running Skillz");
}
