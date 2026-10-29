//! Read-only discovery of AI coding agents and everything they load from disk.
//!
//! Each agent has an adapter that knows its paths and file formats and turns them
//! into the common [`model`]. The agents' own files stay the source of truth.

pub mod adapters;
pub mod model;
mod search;
mod util;

pub use model::{Agent, Asset, AssetKind, LoadStack, LoadedFile, Scope, SearchHit};
pub use search::search;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Where to look. Tests point `home` and `system_root` at temp folders.
#[derive(Debug, Clone)]
pub struct ScanContext {
    pub home: PathBuf,
    /// `$CODEX_HOME`, when set; otherwise `~/.codex`.
    pub codex_home: Option<PathBuf>,
    /// Usually `/`; machine-wide config such as `/etc/codex` lives under it.
    pub system_root: PathBuf,
    /// Repos to scan for project-level files, on top of the ones agents already know.
    pub projects: Vec<PathBuf>,
    /// Look for agent binaries on `PATH` and run `--version`.
    pub probe_binaries: bool,
}

impl ScanContext {
    pub fn from_env(projects: Vec<PathBuf>) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let codex_home = std::env::var_os("CODEX_HOME").map(PathBuf::from);
        Self { home, codex_home, system_root: PathBuf::from("/"), projects, probe_binaries: true }
    }
}

pub trait Adapter {
    fn id(&self) -> &'static str;
    fn scan(&self, ctx: &ScanContext) -> Agent;
    /// The instruction and memory files the agent loads when started in `project`, in order.
    fn load_order(&self, ctx: &ScanContext, project: &Path) -> Vec<LoadedFile>;
}

pub fn adapters() -> Vec<Box<dyn Adapter>> {
    vec![Box::new(adapters::claude::ClaudeCode), Box::new(adapters::codex::Codex)]
}

/// Scans every supported agent.
pub fn scan(ctx: &ScanContext) -> Vec<Agent> {
    let mut agents: Vec<Agent> = adapters().iter().map(|a| a.scan(ctx)).collect();
    for asset in agents.iter_mut().flat_map(|a| a.assets.iter_mut()) {
        if asset.kind != AssetKind::McpServer {
            asset.bytes = std::fs::metadata(&asset.path).ok().map(|m| m.len());
        }
    }
    agents
}

/// What each agent loads when started in `project`.
pub fn load_order(ctx: &ScanContext, project: &Path) -> Vec<LoadStack> {
    adapters()
        .iter()
        .map(|a| LoadStack { agent_id: a.id().to_string(), files: a.load_order(ctx, project) })
        .collect()
}

/// Folders to watch for changes: the agents' roots and every folder an asset was found in,
/// plus the parent of each skill folder so new skills show up. Only existing folders.
pub fn watch_dirs(agents: &[Agent]) -> BTreeSet<PathBuf> {
    let mut dirs = BTreeSet::new();
    for agent in agents {
        for root in &agent.roots {
            let dir = if root.is_dir() { Some(root.as_path()) } else { root.parent() };
            dirs.extend(dir.map(Path::to_path_buf));
        }
        for asset in &agent.assets {
            let parent = asset.path.parent();
            dirs.extend(parent.map(Path::to_path_buf));
            if asset.kind == AssetKind::Skill {
                dirs.extend(parent.and_then(Path::parent).map(Path::to_path_buf));
            }
        }
        for project in &agent.known_projects {
            dirs.insert(project.clone());
        }
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// `dir` and its ancestors from the top down, stopping below `/`.
pub(crate) fn ancestors_top_down(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = dir.ancestors().filter(|p| p.parent().is_some()).map(Path::to_path_buf).collect();
    v.reverse();
    v
}

pub(crate) fn loaded(path: PathBuf, kind: AssetKind, scope: Scope, reason: &str) -> Option<LoadedFile> {
    let bytes = std::fs::metadata(&path).ok().filter(|m| m.is_file())?.len();
    Some(LoadedFile { path, kind, scope, bytes, reason: reason.to_string() })
}

/// Project folders to scan: the ones the caller asked for plus the ones the agent knows,
/// deduplicated, keeping only folders that still exist.
pub(crate) fn project_dirs(ctx: &ScanContext, known: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for p in ctx.projects.iter().chain(known) {
        if p.is_dir() && !out.contains(p) && *p != ctx.home {
            out.push(p.clone());
        }
    }
    out
}
