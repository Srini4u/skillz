//! The common shape every agent's files are translated into.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Skill,
    Instruction,
    Memory,
    McpServer,
    Settings,
    Subagent,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scope {
    User,
    System,
    Project { path: PathBuf },
}

#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    pub kind: AssetKind,
    pub name: String,
    pub scope: Scope,
    /// The file the asset was read from.
    pub path: PathBuf,
    pub description: Option<String>,
    pub enabled: bool,
    /// Size of the file on disk; not set for MCP servers, which share a config file.
    pub bytes: Option<u64>,
    /// Kind-specific fields, e.g. an MCP server's command and (masked) env.
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

impl Asset {
    pub fn new(kind: AssetKind, name: impl Into<String>, scope: Scope, path: PathBuf) -> Self {
        Self {
            kind,
            name: name.into(),
            scope,
            path,
            description: None,
            enabled: true,
            bytes: None,
            details: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub binary: Option<PathBuf>,
    pub version: Option<String>,
    /// Config folders and files this agent reads, whether or not they exist yet.
    pub roots: Vec<PathBuf>,
    pub assets: Vec<Asset>,
    /// Projects this agent knows about from its own config.
    pub known_projects: Vec<PathBuf>,
    /// Files that exist but could not be parsed.
    pub warnings: Vec<String>,
}

/// One file an agent loads into context when it starts in a project, in load order.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedFile {
    pub path: PathBuf,
    pub kind: AssetKind,
    pub scope: Scope,
    pub bytes: u64,
    /// Why it is in the stack, e.g. "user instructions" or "parent folder".
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadStack {
    pub agent_id: String,
    pub files: Vec<LoadedFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub agent_id: String,
    /// Index into that agent's `assets`.
    pub asset: usize,
    /// 1-based line of the first match in the file, or `None` when only the name matched.
    pub line: Option<usize>,
    pub snippet: String,
}
