//! Read-only discovery of AI coding agents and everything they load from disk.
//!
//! Each agent has an adapter that knows its paths and file formats and turns them
//! into the common [`model`]. The agents' own files stay the source of truth.

pub mod adapters;
pub mod model;
mod util;

pub use model::{Agent, Asset, AssetKind, Scope};
use std::path::PathBuf;

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
    fn scan(&self, ctx: &ScanContext) -> Agent;
}

pub fn adapters() -> Vec<Box<dyn Adapter>> {
    vec![Box::new(adapters::claude::ClaudeCode), Box::new(adapters::codex::Codex)]
}

/// Scans every supported agent.
pub fn scan(ctx: &ScanContext) -> Vec<Agent> {
    adapters().iter().map(|a| a.scan(ctx)).collect()
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
