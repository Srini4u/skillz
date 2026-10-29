use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use skillz_core::edit::{DiffLine, FileState, JournalEntry, Store};
use skillz_core::health::Health;
use skillz_core::sync::CopyRequest;
use skillz_core::{ops, sync, Agent, AssetKind, LoadStack, ScanContext, SearchHit};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

type Watcher = Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>;

#[derive(Default)]
struct AppState {
    agents: Mutex<Vec<Agent>>,
    /// Project folders from the last scan; writes are allowed inside them.
    projects: Mutex<Vec<PathBuf>>,
    watcher: Mutex<Option<Watcher>>,
}

impl AppState {
    /// Only files that belong to an agent may be written: its config folders, and the
    /// projects it (or the user) pointed at. Skillz's own backups are never targets.
    fn check(&self, ctx: &ScanContext, path: &Path) -> Result<(), String> {
        let path = normalize(path);
        let codex = ctx.codex_home.clone().unwrap_or_else(|| ctx.home.join(".codex"));
        let mut allowed = vec![ctx.home.join(".claude"), ctx.home.join(".claude.json"), codex, ctx.home.join(".agents")];
        allowed.extend(self.projects.lock().unwrap().iter().cloned());
        allowed.extend(self.agents.lock().unwrap().iter().flat_map(|a| a.known_projects.iter().cloned()));
        if path.components().any(|c| c.as_os_str() == "..") || !allowed.iter().any(|root| path.starts_with(root)) {
            return Err(format!("Skillz only changes agent files; {} is outside them", path.display()));
        }
        Ok(())
    }
}

fn normalize(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn ctx() -> ScanContext {
    ScanContext::from_env(Vec::new())
}

fn store() -> Store {
    Store::default_for(&ctx().home)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Scans every supported agent and re-arms the file watcher on what was found.
#[tauri::command]
async fn scan_agents<R: tauri::Runtime>(app: AppHandle<R>, state: State<'_, AppState>, projects: Vec<String>) -> Result<Vec<Agent>, String> {
    let projects: Vec<PathBuf> = projects.into_iter().map(PathBuf::from).collect();
    let ctx = ScanContext::from_env(projects.clone());
    let agents = skillz_core::scan(&ctx);
    watch(&app, &state, &agents, &ctx.home);
    *state.agents.lock().unwrap() = agents.clone();
    *state.projects.lock().unwrap() = projects;
    Ok(agents)
}

/// The instruction and memory files each agent loads in `project`, in order.
#[tauri::command]
fn load_order(project: String) -> Vec<LoadStack> {
    skillz_core::load_order(&ctx(), Path::new(&project))
}

/// Searches names, descriptions and file contents from the last scan.
#[tauri::command]
fn search(state: State<'_, AppState>, query: String) -> Vec<SearchHit> {
    skillz_core::search(&state.agents.lock().unwrap(), &query)
}

/// A file's content and the fingerprint to send back when saving it.
#[tauri::command]
fn read_file(path: String) -> Result<FileState, String> {
    skillz_core::edit::read_state(Path::new(&path)).map_err(err)
}

#[tauri::command]
fn diff_text(old: String, new: String) -> Vec<DiffLine> {
    skillz_core::edit::diff(&old, &new)
}

#[tauri::command]
fn save_file(state: State<'_, AppState>, path: String, content: String, hash: Option<String>) -> Result<JournalEntry, String> {
    let path = PathBuf::from(path);
    state.check(&ctx(), &path)?;
    store().write(&path, &content, hash.as_deref(), &format!("Edit {}", file_label(&path))).map_err(err)
}

/// Deletes a file, or a whole folder such as a skill with its scripts.
#[tauri::command]
fn delete_path(state: State<'_, AppState>, path: String, hash: Option<String>) -> Result<JournalEntry, String> {
    let path = PathBuf::from(path);
    state.check(&ctx(), &path)?;
    store().delete(&path, hash.as_deref(), &format!("Delete {}", file_label(&path))).map_err(err)
}

#[tauri::command]
fn create_asset(state: State<'_, AppState>, agent_id: String, kind: AssetKind, project: Option<String>, name: String, description: String) -> Result<JournalEntry, String> {
    let ctx = ctx();
    let project = project.map(PathBuf::from);
    let path = ops::new_asset_path(&ctx, &agent_id, kind, project.as_deref(), &name).map_err(err)?;
    state.check(&ctx, &path)?;
    ops::create_asset(&store(), &ctx, &agent_id, kind, project.as_deref(), &name, &description).map_err(err)
}

#[tauri::command]
fn save_mcp(state: State<'_, AppState>, file: String, project: Option<String>, name: String, config: serde_json::Value, rename_from: Option<String>) -> Result<JournalEntry, String> {
    let file = PathBuf::from(file);
    state.check(&ctx(), &file)?;
    let project = project.map(PathBuf::from);
    ops::save_mcp_server(&store(), &file, project.as_deref(), &name, config, rename_from.as_deref()).map_err(err)
}

#[tauri::command]
fn delete_mcp(state: State<'_, AppState>, file: String, project: Option<String>, name: String) -> Result<JournalEntry, String> {
    let file = PathBuf::from(file);
    state.check(&ctx(), &file)?;
    let project = project.map(PathBuf::from);
    ops::delete_mcp_server(&store(), &file, project.as_deref(), &name).map_err(err)
}

/// Codex keeps on/off switches in config.toml; Claude Code has none for skills or servers.
#[tauri::command]
fn set_enabled(state: State<'_, AppState>, agent_id: String, kind: AssetKind, path: String, name: String, enabled: bool) -> Result<JournalEntry, String> {
    if agent_id != "codex" {
        return Err("Only Codex can switch skills and MCP servers off without deleting them".into());
    }
    let ctx = ctx();
    let config = ctx.codex_home.clone().unwrap_or_else(|| ctx.home.join(".codex")).join("config.toml");
    state.check(&ctx, &config)?;
    match kind {
        AssetKind::Skill => ops::set_codex_skill_enabled(&store(), &config, Path::new(&path), enabled),
        AssetKind::McpServer => ops::set_codex_mcp_enabled(&store(), &config, &name, enabled),
        _ => return Err("Only skills and MCP servers can be switched off".into()),
    }
    .map_err(err)
}

#[tauri::command]
fn rename_skill(state: State<'_, AppState>, path: String, new_name: String) -> Result<Vec<JournalEntry>, String> {
    let path = PathBuf::from(path);
    state.check(&ctx(), &path)?;
    ops::rename_skill(&store(), &path, &new_name).map_err(err)
}

/// Copies a skill, command, subagent, instruction file or MCP server to another agent.
#[tauri::command]
fn copy_asset(state: State<'_, AppState>, request: CopyRequest) -> Result<JournalEntry, String> {
    let ctx = ctx();
    if let Some(p) = &request.to_project {
        state.check(&ctx, &p.join("CLAUDE.md"))?;
    }
    sync::copy_asset(&store(), &ctx, &request).map_err(err)
}

/// Where a copy lands by default: the source's project when the target keeps that kind per
/// project, else user level (`None`).
#[tauri::command]
fn copy_target(to_agent: String, kind: AssetKind, project: Option<String>, name: String) -> Option<PathBuf> {
    sync::default_target(&ctx(), &to_agent, kind, project.as_deref().map(Path::new), &name)
}

/// The well-rounded checklist for each installed agent, from the last scan.
#[tauri::command]
fn health(state: State<'_, AppState>) -> Vec<Health> {
    let ctx = ScanContext::from_env(state.projects.lock().unwrap().clone());
    skillz_core::health::health(&state.agents.lock().unwrap(), &ctx)
}

#[tauri::command]
fn history() -> Vec<JournalEntry> {
    store().history()
}

#[tauri::command]
fn undo(state: State<'_, AppState>, id: String) -> Result<JournalEntry, String> {
    let store = store();
    let entry = store.history().into_iter().find(|e| e.id == id).ok_or("That change is no longer in the history")?;
    state.check(&ctx(), &entry.path)?;
    store.undo(&id).map_err(err)
}

fn file_label(p: &Path) -> String {
    let name = p.file_name().unwrap_or_default().to_string_lossy();
    match p.parent().and_then(|d| d.file_name()) {
        Some(dir) if name == "SKILL.md" => format!("skill {}", dir.to_string_lossy()),
        _ => name.to_string(),
    }
}

/// Watches the folders the agents' files live in and emits `agents-changed` when one of
/// them changes. Session logs and other churn under ~/.claude/projects are ignored.
fn watch<R: tauri::Runtime>(app: &AppHandle<R>, state: &AppState, agents: &[Agent], home: &Path) {
    let noise = home.join(".claude").join("projects");
    let handle = app.clone();
    let debouncer = new_debouncer(Duration::from_millis(800), move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let relevant = events.iter().any(|e| {
            let p = &e.path;
            let in_noise = p.starts_with(&noise) && !p.components().any(|c| c.as_os_str() == "memory");
            let ext_ok = p.extension().map_or(true, |x| x == "md" || x == "json" || x == "toml");
            let temp = p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".skillz-tmp"));
            !in_noise && ext_ok && !temp
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

/// Registers state and commands; shared by the app and the IPC tests.
fn with_commands<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            scan_agents,
            load_order,
            search,
            read_file,
            diff_text,
            save_file,
            delete_path,
            create_asset,
            save_mcp,
            delete_mcp,
            set_enabled,
            rename_skill,
            copy_asset,
            copy_target,
            health,
            history,
            undo
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    with_commands(tauri::Builder::default()).run(tauri::generate_context!()).expect("error while running Skillz");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_are_limited_to_agent_files() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let ctx = ScanContext { home: home.clone(), codex_home: None, system_root: home.clone(), projects: vec![], probe_binaries: false };
        let state = AppState::default();
        state.projects.lock().unwrap().push(home.join("code/repo"));
        for ok in [".claude/skills/x/SKILL.md", ".claude.json", ".codex/config.toml", ".agents/skills/x/SKILL.md", "code/repo/CLAUDE.md"] {
            assert!(state.check(&ctx, &home.join(ok)).is_ok(), "{ok}");
        }
        for bad in [".bashrc", ".claude.json.bak", ".local/share/skillz/journal.json", "code/other/CLAUDE.md", "code/repo/../other/x.md"] {
            assert!(state.check(&ctx, &home.join(bad)).is_err(), "{bad}");
        }
    }

    /// Calls commands the way the UI does (camelCase JSON args) against a temp home.
    #[test]
    fn ui_calls_reach_the_commands() {
        use serde_json::{json, Value};
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", home.join(".local/share"));
        std::env::remove_var("CODEX_HOME");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(home.join(".codex/config.toml"), "# mine\nmodel = \"gpt-5\"\n").unwrap();

        let app = with_commands(tauri::test::mock_builder()).build(tauri::generate_context!(test = true)).unwrap();
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        let call = |cmd: &str, body: Value| -> Result<Value, Value> {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            )
            .map(|b| b.deserialize::<Value>().unwrap())
        };

        call("scan_agents", json!({"projects": []})).unwrap();
        let created = call("create_asset", json!({"agentId": "claude-code", "kind": "skill", "project": null, "name": "Write Tests", "description": "Add tests"})).unwrap();
        let skill = home.join(".claude/skills/write-tests/SKILL.md");
        assert_eq!(created["path"], json!(skill));

        let state = call("read_file", json!({"path": skill})).unwrap();
        let saved = call("save_file", json!({"path": skill, "content": "---\nname: write-tests\n---\nnew body\n", "hash": state["hash"]})).unwrap();
        assert_eq!(saved["action"], "write");
        assert!(call("save_file", json!({"path": skill, "content": "x", "hash": state["hash"]})).is_err(), "stale hash must be refused");

        call("save_mcp", json!({"file": home.join(".codex/config.toml"), "project": null, "name": "github", "config": {"command": "npx"}, "renameFrom": null})).unwrap();
        call("set_enabled", json!({"agentId": "codex", "kind": "mcp_server", "path": home.join(".codex/config.toml"), "name": "github", "enabled": false})).unwrap();
        let toml = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(toml.starts_with("# mine") && toml.contains("enabled = false"), "{toml}");

        let renamed = call("rename_skill", json!({"path": skill, "newName": "tests"})).unwrap();
        assert_eq!(renamed.as_array().unwrap().len(), 2);
        assert!(call("save_file", json!({"path": home.join(".bashrc"), "content": "x", "hash": null})).is_err());

        let hist = call("history", json!({})).unwrap();
        let newest = hist[0]["id"].as_str().unwrap().to_string();
        call("undo", json!({"id": newest})).unwrap();
        assert!(std::fs::read_to_string(home.join(".claude/skills/tests/SKILL.md")).unwrap().contains("name: write-tests"));
        // Phase 3: copy the skill to Codex, then read the health report.
        call("scan_agents", json!({"projects": []})).unwrap();
        let skill_now = home.join(".claude/skills/tests/SKILL.md");
        let target = call("copy_target", json!({"toAgent": "codex", "kind": "skill", "project": null, "name": "tests"})).unwrap();
        assert!(target.is_null());
        let req = json!({"fromAgent": "claude-code", "kind": "skill", "name": "tests", "path": skill_now, "project": null, "toAgent": "codex", "toProject": null});
        call("copy_asset", json!({"request": req})).unwrap();
        assert!(home.join(".agents/skills/tests/SKILL.md").exists());
        let bad = json!({"fromAgent": "claude-code", "kind": "instruction", "name": "x", "path": skill_now, "project": null, "toAgent": "codex", "toProject": "/etc"});
        assert!(call("copy_asset", json!({"request": bad})).is_err(), "copies only into known projects");
        let report = call("health", json!({})).unwrap();
        assert!(report.as_array().unwrap().iter().all(|h| h["score"].as_u64().unwrap() <= 100));

        let diff = call("diff_text", json!({"old": "a\n", "new": "b\n"})).unwrap();
        assert_eq!(diff[0]["tag"], "remove");
    }
}
