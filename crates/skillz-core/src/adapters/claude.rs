//! Claude Code: `~/.claude/`, `~/.claude.json`, and `CLAUDE.md`, `.claude/`, `.mcp.json` in each repo.

use crate::model::{Agent, Asset, AssetKind, Scope};
use crate::util::{file_asset, markdown_in, mask_secrets, skills_in, version_of, which};
use crate::{project_dirs, Adapter, ScanContext};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ClaudeCode;

impl Adapter for ClaudeCode {
    fn scan(&self, ctx: &ScanContext) -> Agent {
        let dir = ctx.home.join(".claude");
        let state_file = ctx.home.join(".claude.json");
        let binary = ctx.probe_binaries.then(|| which("claude")).flatten();
        let mut agent = Agent {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            installed: binary.is_some() || dir.is_dir() || state_file.is_file(),
            version: binary.as_deref().and_then(version_of),
            binary,
            roots: vec![dir.clone(), state_file.clone()],
            assets: Vec::new(),
            known_projects: Vec::new(),
            warnings: Vec::new(),
        };

        let state = read_json(&state_file, &mut agent.warnings);
        if let Some(projects) = state.as_ref().and_then(|s| s.get("projects")).and_then(Value::as_object) {
            agent.known_projects = projects.keys().map(PathBuf::from).collect();
        }

        // User scope.
        let user = Scope::User;
        agent.assets.extend(file_asset(dir.join("CLAUDE.md"), AssetKind::Instruction, &user));
        agent.assets.extend(skills_in(&dir.join("skills"), &user));
        agent.assets.extend(markdown_in(&dir.join("agents"), AssetKind::Subagent, &user, 2));
        agent.assets.extend(markdown_in(&dir.join("commands"), AssetKind::Command, &user, 3));
        agent.assets.extend(file_asset(dir.join("settings.json"), AssetKind::Settings, &user));
        if let Some(servers) = state.as_ref().and_then(|s| s.get("mcpServers")) {
            agent.assets.extend(mcp_servers(servers, &state_file, &user));
        }

        // Project scope.
        for project in project_dirs(ctx, &agent.known_projects) {
            let scope = Scope::Project { path: project.clone() };
            let local = project.join(".claude");
            for file in [project.join("CLAUDE.md"), local.join("CLAUDE.md"), project.join("CLAUDE.local.md")] {
                agent.assets.extend(file_asset(file, AssetKind::Instruction, &scope));
            }
            agent.assets.extend(skills_in(&local.join("skills"), &scope));
            agent.assets.extend(markdown_in(&local.join("agents"), AssetKind::Subagent, &scope, 2));
            agent.assets.extend(markdown_in(&local.join("commands"), AssetKind::Command, &scope, 3));
            for file in [local.join("settings.json"), local.join("settings.local.json")] {
                agent.assets.extend(file_asset(file, AssetKind::Settings, &scope));
            }
            let mcp_file = project.join(".mcp.json");
            if let Some(servers) = read_json(&mcp_file, &mut agent.warnings).and_then(|v| v.get("mcpServers").cloned()) {
                agent.assets.extend(mcp_servers(&servers, &mcp_file, &scope));
            }
            // Servers added with `claude mcp add` (local scope) live in ~/.claude.json under the project.
            let key = project.to_string_lossy();
            if let Some(servers) = state.as_ref().and_then(|s| s.pointer(&format!("/projects/{}/mcpServers", escape_pointer(&key)))) {
                agent.assets.extend(mcp_servers(servers, &state_file, &scope));
            }
            // Auto memory: ~/.claude/projects/<path with every non-alphanumeric char as '-'>/memory/*.md
            let encoded: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
            agent.assets.extend(markdown_in(&dir.join("projects").join(encoded).join("memory"), AssetKind::Memory, &scope, 1));
        }
        agent
    }
}

fn mcp_servers(servers: &Value, file: &Path, scope: &Scope) -> Vec<Asset> {
    let Some(map) = servers.as_object() else { return Vec::new() };
    map.iter()
        .map(|(name, cfg)| {
            let mut asset = Asset::new(AssetKind::McpServer, name, scope.clone(), file.to_path_buf());
            let mut details = cfg.clone();
            mask_secrets(&mut details);
            asset.description = details
                .get("command")
                .or_else(|| details.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string);
            asset.enabled = !cfg.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            asset.details = details;
            asset
        })
        .collect()
}

fn read_json(path: &Path, warnings: &mut Vec<String>) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            warnings.push(format!("{}: {e}", path.display()));
            None
        }
    }
}

fn escape_pointer(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
