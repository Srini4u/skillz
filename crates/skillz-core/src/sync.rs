//! Copies a skill, command, subagent, instruction file or MCP server from one agent to
//! another, translating it into the target agent's layout and format.

use crate::edit::{read_state, EditError, JournalEntry, Result, Store};
use crate::model::AssetKind;
use crate::ops::{self, new_asset_path};
use crate::ScanContext;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyRequest {
    pub from_agent: String,
    pub kind: AssetKind,
    pub name: String,
    /// The source asset's file (for an MCP server, the config file holding it).
    pub path: PathBuf,
    /// The source's project, when it lives in one.
    pub project: Option<PathBuf>,
    pub to_agent: String,
    /// Where to put the copy: a project, or `None` for user level.
    pub to_project: Option<PathBuf>,
}

/// Where a copy should go by default: the same project if the target agent keeps that kind
/// per project, otherwise user level.
pub fn default_target(ctx: &ScanContext, to_agent: &str, kind: AssetKind, project: Option<&Path>, name: &str) -> Option<PathBuf> {
    let project = project?;
    let in_project = match kind {
        AssetKind::McpServer => to_agent == "claude-code",
        _ => new_asset_path(ctx, to_agent, kind, Some(project), name).is_ok(),
    };
    in_project.then(|| project.to_path_buf())
}

/// The file or folder a copy writes: a skill's folder, the new file, or the MCP config file.
pub fn destination(ctx: &ScanContext, req: &CopyRequest) -> Result<PathBuf> {
    let target = |name: &str| new_asset_path(ctx, &req.to_agent, req.kind, req.to_project.as_deref(), name);
    match req.kind {
        AssetKind::Skill => target(&req.name).map(|p| p.parent().map(Path::to_path_buf).unwrap_or(p)),
        // A slash command maps to a Codex prompt and back; nested names (git:ship) flatten.
        AssetKind::Command | AssetKind::Subagent => target(&req.name.replace(':', "-")),
        AssetKind::Instruction | AssetKind::Memory => target(&req.name),
        AssetKind::McpServer => mcp_file(ctx, &req.to_agent, req.to_project.as_deref()),
        AssetKind::Settings => Err(EditError::Invalid("settings differ too much between agents to copy".into())),
    }
}

pub fn copy_asset(store: &Store, ctx: &ScanContext, req: &CopyRequest) -> Result<JournalEntry> {
    if req.from_agent == req.to_agent && req.project == req.to_project {
        return Err(EditError::Invalid("source and target are the same".into()));
    }
    let dest = destination(ctx, req)?;
    let summary = format!("Copy {} {} to {}", label(req.kind), req.name, agent_name(&req.to_agent));
    match req.kind {
        AssetKind::Skill => {
            let src = req.path.parent().ok_or_else(|| EditError::NotFound(req.path.clone()))?;
            store.copy_in(src, &dest, &summary)
        }
        AssetKind::Command | AssetKind::Subagent => store.copy_in(&req.path, &dest, &summary),
        AssetKind::Instruction | AssetKind::Memory => {
            if dest.exists() {
                return Err(exists(&dest));
            }
            let content = read_state(&req.path)?.content;
            store.write(&dest, &content, None, &summary)
        }
        AssetKind::McpServer => {
            let config = read_server(&req.path, req.project.as_deref(), &req.name)?;
            let translated = translate_server(&config, &req.from_agent, &req.to_agent)?;
            let file = dest;
            if read_server(&file, json_project(&file, req.to_project.as_deref()), &req.name).is_ok() {
                return Err(EditError::Invalid(format!("{} already has an MCP server named {}", agent_name(&req.to_agent), req.name)));
            }
            let mut e = ops::save_mcp_server(store, &file, json_project(&file, req.to_project.as_deref()), &req.name, translated, None, None)?;
            e.summary = summary;
            Ok(e)
        }
        AssetKind::Settings => unreachable!("destination refuses settings"),
    }
}

/// The config file that holds `to_agent`'s MCP servers at that scope.
fn mcp_file(ctx: &ScanContext, to_agent: &str, project: Option<&Path>) -> Result<PathBuf> {
    match (to_agent, project) {
        ("claude-code", Some(p)) => Ok(p.join(".mcp.json")),
        ("claude-code", None) => Ok(ctx.home.join(".claude.json")),
        ("codex", None) => Ok(ctx.codex_home.clone().unwrap_or_else(|| ctx.home.join(".codex")).join("config.toml")),
        ("codex", Some(_)) => Err(EditError::Invalid("Codex keeps MCP servers at user level only".into())),
        _ => Err(EditError::Invalid(format!("unknown agent {to_agent}"))),
    }
}

/// Servers inside ~/.claude.json are keyed by project; everywhere else the file is the scope.
fn json_project<'a>(file: &Path, project: Option<&'a Path>) -> Option<&'a Path> {
    file.file_name().is_some_and(|n| n == ".claude.json").then_some(project).flatten()
}

/// The real (unmasked) config of server `name` in `file`.
pub fn read_server(file: &Path, project: Option<&Path>, name: &str) -> Result<Value> {
    let text = read_state(file)?.content;
    let missing = || EditError::NotFound(PathBuf::from(name));
    if ops::is_toml(file) {
        let doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| EditError::Invalid(e.to_string()))?;
        let item = doc.get("mcp_servers").and_then(|s| s.get(name)).ok_or_else(missing)?;
        Ok(ops::toml_to_json(item))
    } else {
        let root: Value = serde_json::from_str(&text).map_err(|e| EditError::Invalid(e.to_string()))?;
        let project = json_project(file, project);
        let servers = match project {
            Some(p) => root.get("projects").and_then(|ps| ps.get(p.to_string_lossy().as_ref())).and_then(|p| p.get("mcpServers")),
            None => root.get("mcpServers"),
        };
        servers.and_then(|s| s.get(name)).cloned().ok_or_else(missing)
    }
}

/// Claude Code and Codex describe servers with mostly the same fields. Only the fields both
/// understand are carried over, so the target never sees a key it would reject. Header
/// secrets taken from environment variables stay in environment variables, and a server
/// that is switched off stays off.
pub fn translate_server(config: &Value, from: &str, to: &str) -> Result<Value> {
    let get = |k: &str| config.get(k).cloned();
    let mut out = Map::new();
    if let Some(url) = get("url") {
        let sse = from != "codex" && get("type").is_some_and(|t| t == "sse");
        if to == "claude-code" {
            out.insert("type".into(), Value::String(if sse { "sse" } else { "http" }.into()));
        } else if sse {
            return Err(EditError::Invalid("Codex only connects to MCP servers over streamable HTTP, not SSE".into()));
        }
        out.insert("url".into(), url);
        let headers = if from == "codex" { codex_headers(config) } else { get("headers").and_then(|h| h.as_object().cloned()).unwrap_or_default() };
        if to == "codex" {
            codex_from_headers(headers, &mut out);
        } else if !headers.is_empty() {
            out.insert("headers".into(), Value::Object(headers));
        }
    } else {
        for k in ["command", "args", "env", "cwd"] {
            if let Some(v) = get(k) {
                out.insert(k.into(), v);
            }
        }
    }
    let off = if from == "codex" { get("enabled") == Some(Value::Bool(false)) } else { get("disabled") == Some(Value::Bool(true)) };
    if off {
        let (k, v) = if to == "codex" { ("enabled", false) } else { ("disabled", true) };
        out.insert(k.into(), Value::Bool(v));
    }
    Ok(Value::Object(out))
}

/// A Codex server's headers in Claude Code's form, where `${VAR}` is read from the environment.
fn codex_headers(config: &Value) -> Map<String, Value> {
    let mut headers = config.get("http_headers").and_then(|h| h.as_object().cloned()).unwrap_or_default();
    if let Some(env) = config.get("env_http_headers").and_then(Value::as_object) {
        for (k, var) in env {
            if let Some(var) = var.as_str() {
                headers.insert(k.clone(), Value::String(format!("${{{var}}}")));
            }
        }
    }
    if let Some(var) = config.get("bearer_token_env_var").and_then(Value::as_str) {
        headers.insert("Authorization".into(), Value::String(format!("Bearer ${{{var}}}")));
    }
    headers
}

/// The reverse: headers that only name an environment variable go to Codex's env fields.
fn codex_from_headers(headers: Map<String, Value>, out: &mut Map<String, Value>) {
    let (mut plain, mut env) = (Map::new(), Map::new());
    for (k, v) in headers {
        let text = v.as_str().unwrap_or_default();
        if let Some(var) = text.strip_prefix("Bearer ").and_then(env_var).filter(|_| k.eq_ignore_ascii_case("authorization")) {
            out.insert("bearer_token_env_var".into(), Value::String(var.into()));
        } else if let Some(var) = env_var(text) {
            env.insert(k, Value::String(var.into()));
        } else {
            plain.insert(k, v);
        }
    }
    if !plain.is_empty() {
        out.insert("http_headers".into(), Value::Object(plain));
    }
    if !env.is_empty() {
        out.insert("env_http_headers".into(), Value::Object(env));
    }
}

/// `VAR` when `s` is exactly `${VAR}`.
fn env_var(s: &str) -> Option<&str> {
    s.strip_prefix("${")?.strip_suffix('}').filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn exists(p: &Path) -> EditError {
    EditError::Io(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} already exists", p.display())))
}

fn label(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Skill => "skill",
        AssetKind::Instruction => "instructions",
        AssetKind::Memory => "memory",
        AssetKind::McpServer => "MCP server",
        AssetKind::Settings => "settings",
        AssetKind::Subagent => "subagent",
        AssetKind::Command => "command",
    }
}

fn agent_name(id: &str) -> &str {
    match id {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup() -> (tempfile::TempDir, Store, ScanContext) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path().join("store"));
        let ctx = ScanContext { home: tmp.path().join("home"), codex_home: None, system_root: tmp.path().into(), projects: vec![], probe_binaries: false };
        (tmp, store, ctx)
    }

    fn req(from: &str, kind: AssetKind, name: &str, path: PathBuf, project: Option<PathBuf>, to: &str, to_project: Option<PathBuf>) -> CopyRequest {
        CopyRequest { from_agent: from.into(), kind, name: name.into(), path, project, to_agent: to.into(), to_project }
    }

    #[test]
    fn copies_skill_folder_claude_to_codex() {
        let (_t, store, ctx) = setup();
        let src = ctx.home.join(".claude/skills/pdf/SKILL.md");
        fs::create_dir_all(src.parent().unwrap().join("scripts")).unwrap();
        fs::write(&src, "---\nname: pdf\n---\n").unwrap();
        fs::write(src.parent().unwrap().join("scripts/run.sh"), "echo").unwrap();
        copy_asset(&store, &ctx, &req("claude-code", AssetKind::Skill, "pdf", src.clone(), None, "codex", None)).unwrap();
        assert!(ctx.home.join(".agents/skills/pdf/scripts/run.sh").exists());
        assert!(copy_asset(&store, &ctx, &req("claude-code", AssetKind::Skill, "pdf", src, None, "codex", None)).is_err());
    }

    #[test]
    fn copies_mcp_server_with_real_secret_across_formats() {
        let (_t, store, ctx) = setup();
        fs::create_dir_all(&ctx.home).unwrap();
        let claude = ctx.home.join(".claude.json");
        fs::write(&claude, r#"{"mcpServers": {"github": {"type": "stdio", "command": "npx", "args": ["gh"], "env": {"TOKEN": "real"}}, "docs": {"type": "http", "url": "https://x/mcp", "headers": {"Authorization": "Bearer t"}}}}"#).unwrap();
        copy_asset(&store, &ctx, &req("claude-code", AssetKind::McpServer, "github", claude.clone(), None, "codex", None)).unwrap();
        copy_asset(&store, &ctx, &req("claude-code", AssetKind::McpServer, "docs", claude.clone(), None, "codex", None)).unwrap();
        let t: toml::Table = fs::read_to_string(ctx.home.join(".codex/config.toml")).unwrap().parse().unwrap();
        assert_eq!(t["mcp_servers"]["github"]["env"]["TOKEN"].as_str(), Some("real"));
        assert!(t["mcp_servers"]["github"].get("type").is_none());
        assert_eq!(t["mcp_servers"]["docs"]["http_headers"]["Authorization"].as_str(), Some("Bearer t"));

        // And back into a project's .mcp.json.
        let repo = ctx.home.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let codex = ctx.home.join(".codex/config.toml");
        copy_asset(&store, &ctx, &req("codex", AssetKind::McpServer, "docs", codex, None, "claude-code", Some(repo.clone()))).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(repo.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["docs"], serde_json::json!({"type": "http", "url": "https://x/mcp", "headers": {"Authorization": "Bearer t"}}));
        assert!(copy_asset(&store, &ctx, &req("claude-code", AssetKind::McpServer, "github", claude, None, "codex", None)).is_err(), "already there");
    }

    #[test]
    fn keeps_env_secrets_and_off_switch_and_refuses_sse() {
        let codex = serde_json::json!({"url": "https://x/mcp", "bearer_token_env_var": "TOKEN", "env_http_headers": {"X-Org": "ORG"}, "http_headers": {"X-A": "1"}, "enabled": false});
        let claude = translate_server(&codex, "codex", "claude-code").unwrap();
        assert_eq!(claude, serde_json::json!({"type": "http", "url": "https://x/mcp", "headers": {"X-A": "1", "X-Org": "${ORG}", "Authorization": "Bearer ${TOKEN}"}, "disabled": true}));
        assert_eq!(translate_server(&claude, "claude-code", "codex").unwrap(), codex);
        let sse = serde_json::json!({"type": "sse", "url": "https://x/sse"});
        assert!(translate_server(&sse, "claude-code", "codex").is_err());
        assert_eq!(translate_server(&sse, "claude-code", "claude-code").unwrap()["type"], "sse");
    }

    #[test]
    fn scaffolds_agents_md_from_claude_md() {
        let (_t, store, ctx) = setup();
        let repo = ctx.home.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("CLAUDE.md"), "# Repo rules\n").unwrap();
        let r = req("claude-code", AssetKind::Instruction, "CLAUDE.md", repo.join("CLAUDE.md"), Some(repo.clone()), "codex", Some(repo.clone()));
        copy_asset(&store, &ctx, &r).unwrap();
        assert_eq!(fs::read_to_string(repo.join("AGENTS.md")).unwrap(), "# Repo rules\n");
        assert!(copy_asset(&store, &ctx, &r).is_err(), "never overwrites");
    }

    #[test]
    fn command_to_codex_prompt_and_default_targets() {
        let (_t, store, ctx) = setup();
        let src = ctx.home.join(".claude/commands/git/ship.md");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        fs::write(&src, "ship").unwrap();
        copy_asset(&store, &ctx, &req("claude-code", AssetKind::Command, "git:ship", src, None, "codex", None)).unwrap();
        assert!(ctx.home.join(".codex/prompts/git-ship.md").exists());
        let repo = Path::new("/r");
        assert_eq!(default_target(&ctx, "codex", AssetKind::Skill, Some(repo), "x"), Some(repo.to_path_buf()));
        assert_eq!(default_target(&ctx, "codex", AssetKind::Command, Some(repo), "x"), None);
        assert_eq!(default_target(&ctx, "codex", AssetKind::McpServer, Some(repo), "x"), None);
        assert_eq!(default_target(&ctx, "claude-code", AssetKind::McpServer, Some(repo), "x"), Some(repo.to_path_buf()));
    }
}
