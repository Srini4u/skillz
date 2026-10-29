//! Agent-aware changes built on [`edit::Store`]: new assets from templates, MCP servers in
//! each agent's own config format, enabling and disabling, and renaming skills.

use crate::edit::{read_state, EditError, JournalEntry, Result, Store};
use crate::model::AssetKind;
use crate::util::MASK;
use crate::ScanContext;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Where a new asset of `kind` named `name` goes for `agent_id`, at user level or in `project`.
pub fn new_asset_path(ctx: &ScanContext, agent_id: &str, kind: AssetKind, project: Option<&Path>, name: &str) -> Result<PathBuf> {
    let slug = slug(name)?;
    let claude = project.map(|p| p.join(".claude")).unwrap_or_else(|| ctx.home.join(".claude"));
    let codex = ctx.codex_home.clone().unwrap_or_else(|| ctx.home.join(".codex"));
    let path = match (agent_id, kind) {
        ("claude-code", AssetKind::Skill) => claude.join("skills").join(&slug).join("SKILL.md"),
        ("claude-code", AssetKind::Subagent) => claude.join("agents").join(format!("{slug}.md")),
        ("claude-code", AssetKind::Command) => claude.join("commands").join(format!("{slug}.md")),
        ("claude-code", AssetKind::Instruction) => match project {
            Some(p) => p.join("CLAUDE.md"),
            None => claude.join("CLAUDE.md"),
        },
        ("claude-code", AssetKind::Memory) => {
            let p = project.ok_or_else(|| unsupported("Claude Code memory belongs to a project"))?;
            let encoded: String = p.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
            ctx.home.join(".claude/projects").join(encoded).join("memory").join(format!("{slug}.md"))
        }
        ("codex", AssetKind::Skill) => match project {
            Some(p) => p.join(".agents/skills").join(&slug).join("SKILL.md"),
            None => ctx.home.join(".agents/skills").join(&slug).join("SKILL.md"),
        },
        ("codex", AssetKind::Command) if project.is_none() => codex.join("prompts").join(format!("{slug}.md")),
        ("codex", AssetKind::Instruction) => match project {
            Some(p) => p.join("AGENTS.md"),
            None => codex.join("AGENTS.md"),
        },
        _ => return Err(unsupported(&format!("{agent_id} has no {kind:?} at this scope"))),
    };
    Ok(path)
}

/// Starter content for a new asset.
pub fn template(kind: AssetKind, name: &str, description: &str) -> String {
    let desc = if description.trim().is_empty() { "Say when this should be used." } else { description.trim() };
    match kind {
        AssetKind::Skill | AssetKind::Subagent => format!("---\nname: {name}\ndescription: {desc}\n---\n\n# {name}\n\n"),
        AssetKind::Command => format!("---\ndescription: {desc}\n---\n\n"),
        AssetKind::Instruction => format!("# {name}\n\n{desc}\n"),
        AssetKind::Memory => format!("- {desc}\n"),
        _ => String::new(),
    }
}

pub fn create_asset(store: &Store, ctx: &ScanContext, agent_id: &str, kind: AssetKind, project: Option<&Path>, name: &str, description: &str) -> Result<JournalEntry> {
    let path = new_asset_path(ctx, agent_id, kind, project, name)?;
    if path.exists() {
        return Err(EditError::Io(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} already exists", path.display()))));
    }
    let display = if kind == AssetKind::Instruction { file_name(&path) } else { slug(name)? };
    store.write(&path, &template(kind, &display, description), None, &format!("Create {kind:?} {display}").to_lowercase())
}

/// Adds or replaces MCP server `name` in `file`, keeping every other byte of meaning.
/// `project` is set when the server lives under `projects.<path>` in `~/.claude.json`.
/// Masked secret values in `config` are swapped back for the values already on disk.
/// When `rename_from` is given, that server is removed in the same write.
pub fn save_mcp_server(store: &Store, file: &Path, project: Option<&Path>, name: &str, config: Value, rename_from: Option<&str>) -> Result<JournalEntry> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EditError::Invalid("an MCP server needs a name".into()));
    }
    if !config.is_object() {
        return Err(EditError::Invalid("an MCP server's config must be an object".into()));
    }
    let taken = || EditError::Invalid(format!("there is already an MCP server named {name}"));
    let renaming = rename_from.is_some_and(|from| from != name);
    let state = read_state(file)?;
    let content = if is_toml(file) {
        let mut doc: toml_edit::DocumentMut = state.content.parse().map_err(|e: toml_edit::TomlError| EditError::Invalid(e.to_string()))?;
        let servers = doc.entry("mcp_servers").or_insert(toml_edit::table()).as_table_mut().ok_or_else(|| EditError::Invalid("mcp_servers is not a table".into()))?;
        servers.set_implicit(true);
        if renaming && servers.contains_key(name) {
            return Err(taken());
        }
        let old = servers.get(rename_from.unwrap_or(name)).map(|i| toml_to_json(i)).unwrap_or(Value::Null);
        let merged = unmask(config, &old);
        if let Some(from) = rename_from {
            servers.remove(from);
        }
        servers.insert(name, toml_edit::Item::Table(json_to_toml_table(&merged)?));
        doc.to_string()
    } else {
        let mut root: Value = if state.exists { serde_json::from_str(&state.content).map_err(|e| EditError::Invalid(e.to_string()))? } else { Value::Object(Default::default()) };
        let servers = servers_obj(&mut root, project)?;
        if renaming && servers.contains_key(name) {
            return Err(taken());
        }
        let old = servers.get(rename_from.unwrap_or(name)).cloned().unwrap_or(Value::Null);
        let merged = unmask(config, &old);
        if let Some(from) = rename_from {
            servers.shift_remove(from);
        }
        servers.insert(name.to_string(), merged);
        pretty_json(&root)
    };
    store.write(file, &content, Some(&state.hash), &format!("Save MCP server {name}"))
}

pub fn delete_mcp_server(store: &Store, file: &Path, project: Option<&Path>, name: &str) -> Result<JournalEntry> {
    let state = read_state(file)?;
    let content = if is_toml(file) {
        let mut doc: toml_edit::DocumentMut = state.content.parse().map_err(|e: toml_edit::TomlError| EditError::Invalid(e.to_string()))?;
        let removed = doc.get_mut("mcp_servers").and_then(|s| s.as_table_mut()).and_then(|t| t.remove(name));
        removed.ok_or_else(|| EditError::NotFound(PathBuf::from(name)))?;
        doc.to_string()
    } else {
        let mut root: Value = serde_json::from_str(&state.content).map_err(|e| EditError::Invalid(e.to_string()))?;
        servers_obj(&mut root, project)?.shift_remove(name).ok_or_else(|| EditError::NotFound(PathBuf::from(name)))?;
        pretty_json(&root)
    };
    store.write(file, &content, Some(&state.hash), &format!("Delete MCP server {name}"))
}

/// Codex only: `enabled = false` on an MCP server table.
pub fn set_codex_mcp_enabled(store: &Store, config_file: &Path, name: &str, enabled: bool) -> Result<JournalEntry> {
    let state = read_state(config_file)?;
    let mut doc: toml_edit::DocumentMut = state.content.parse().map_err(|e: toml_edit::TomlError| EditError::Invalid(e.to_string()))?;
    let server = doc.get_mut("mcp_servers").and_then(|s| s.get_mut(name)).and_then(|s| s.as_table_like_mut()).ok_or_else(|| EditError::NotFound(PathBuf::from(name)))?;
    if enabled {
        server.remove("enabled");
    } else {
        server.insert("enabled", toml_edit::value(false));
    }
    let verb = if enabled { "Enable" } else { "Disable" };
    store.write(config_file, &doc.to_string(), Some(&state.hash), &format!("{verb} MCP server {name}"))
}

/// Codex only: adds or removes a `[[skills.config]] path = … enabled = false` entry.
pub fn set_codex_skill_enabled(store: &Store, config_file: &Path, skill_file: &Path, enabled: bool) -> Result<JournalEntry> {
    let state = read_state(config_file)?;
    let mut doc: toml_edit::DocumentMut = state.content.parse().map_err(|e: toml_edit::TomlError| EditError::Invalid(e.to_string()))?;
    let folder = skill_file.parent().unwrap_or(skill_file);
    let matches = |t: &toml_edit::Table| t.get("path").and_then(|p| p.as_str()).is_some_and(|p| Path::new(p) == skill_file || Path::new(p) == folder);
    let skills = doc.entry("skills").or_insert(toml_edit::table()).as_table_mut().ok_or_else(|| EditError::Invalid("skills is not a table".into()))?;
    skills.set_implicit(true);
    let list = skills.entry("config").or_insert(toml_edit::Item::ArrayOfTables(Default::default())).as_array_of_tables_mut().ok_or_else(|| EditError::Invalid("skills.config is not an array of tables".into()))?;
    list.retain(|t| !matches(t));
    if !enabled {
        let mut t = toml_edit::Table::new();
        t.insert("path", toml_edit::value(folder.to_string_lossy().to_string()));
        t.insert("enabled", toml_edit::value(false));
        list.push(t);
    }
    if list.is_empty() {
        skills.remove("config");
    }
    if doc.get("skills").and_then(|s| s.as_table()).is_some_and(|t| t.is_empty()) {
        doc.remove("skills");
    }
    let verb = if enabled { "Enable" } else { "Disable" };
    store.write(config_file, &doc.to_string(), Some(&state.hash), &format!("{verb} skill {}", file_name(folder)))
}

/// Renames a skill's folder and the `name:` in its frontmatter (two journal entries).
pub fn rename_skill(store: &Store, skill_file: &Path, new_name: &str) -> Result<Vec<JournalEntry>> {
    let slug = slug(new_name)?;
    let folder = skill_file.parent().ok_or_else(|| EditError::NotFound(skill_file.to_path_buf()))?;
    let to = folder.with_file_name(&slug);
    let moved = store.rename(folder, &to, &format!("Rename skill {} to {slug}", file_name(folder)))?;
    let new_file = to.join("SKILL.md");
    let state = read_state(&new_file)?;
    let mut lines: Vec<String> = state.content.lines().map(str::to_string).collect();
    let mut entries = vec![moved];
    if lines.first().map(|l| l.trim()) == Some("---") {
        if let Some(l) = lines.iter_mut().skip(1).take_while(|l| l.trim() != "---").find(|l| l.starts_with("name:")) {
            *l = format!("name: {slug}");
            let mut text = lines.join("\n");
            if state.content.ends_with('\n') {
                text.push('\n');
            }
            entries.push(store.write(&new_file, &text, Some(&state.hash), &format!("Set skill name to {slug}"))?);
        }
    }
    Ok(entries)
}

fn servers_obj<'a>(root: &'a mut Value, project: Option<&Path>) -> Result<&'a mut serde_json::Map<String, Value>> {
    let bad = || EditError::Invalid("unexpected shape in the JSON file".into());
    let mut node = root.as_object_mut().ok_or_else(bad)?;
    if let Some(p) = project {
        node = node.entry("projects").or_insert_with(|| Value::Object(Default::default())).as_object_mut().ok_or_else(bad)?;
        node = node.entry(p.to_string_lossy().to_string()).or_insert_with(|| Value::Object(Default::default())).as_object_mut().ok_or_else(bad)?;
    }
    node.entry("mcpServers").or_insert_with(|| Value::Object(Default::default())).as_object_mut().ok_or_else(bad)
}

/// Replaces every masked value in `edited` with the matching value from `original`.
fn unmask(mut edited: Value, original: &Value) -> Value {
    if let Some(obj) = edited.as_object_mut() {
        for (field, value) in obj.iter_mut() {
            if let Some(map) = value.as_object_mut() {
                for (k, v) in map.iter_mut() {
                    if v.as_str() == Some(MASK) {
                        if let Some(orig) = original.get(field).and_then(|m| m.get(k)) {
                            *v = orig.clone();
                        }
                    }
                }
            }
        }
    }
    edited
}

fn json_to_toml_table(v: &Value) -> Result<toml_edit::Table> {
    let text = toml::to_string(v).map_err(|e| EditError::Invalid(format!("cannot express this config in TOML: {e}")))?;
    let doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| EditError::Invalid(e.to_string()))?;
    Ok(doc.as_table().clone())
}

fn toml_to_json(item: &toml_edit::Item) -> Value {
    let text = match item.as_table_like() {
        Some(_) => {
            let mut doc = toml_edit::DocumentMut::new();
            doc.insert("x", item.clone());
            doc.to_string()
        }
        None => return Value::Null,
    };
    text.parse::<toml::Table>().ok().and_then(|t| t.get("x").cloned()).and_then(|t| serde_json::to_value(t).ok()).unwrap_or(Value::Null)
}

fn pretty_json(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).expect("json serializes");
    s.push('\n');
    s
}

fn is_toml(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "toml")
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().to_string()
}

fn unsupported(msg: &str) -> EditError {
    EditError::Invalid(msg.to_string())
}

/// Lower-case, dash-separated, safe as a folder or file name.
pub fn slug(name: &str) -> Result<String> {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() {
        Err(EditError::Invalid("the name needs at least one letter or digit".into()))
    } else {
        Ok(s)
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

    #[test]
    fn creates_skills_where_each_agent_looks() {
        let (_t, store, ctx) = setup();
        create_asset(&store, &ctx, "claude-code", AssetKind::Skill, None, "PDF Review", "Review PDFs").unwrap();
        let text = fs::read_to_string(ctx.home.join(".claude/skills/pdf-review/SKILL.md")).unwrap();
        assert!(text.starts_with("---\nname: pdf-review\ndescription: Review PDFs\n---"));
        create_asset(&store, &ctx, "codex", AssetKind::Skill, None, "pdf-review", "").unwrap();
        assert!(ctx.home.join(".agents/skills/pdf-review/SKILL.md").exists());
        assert!(create_asset(&store, &ctx, "codex", AssetKind::Skill, None, "pdf-review", "").is_err());
        assert!(create_asset(&store, &ctx, "codex", AssetKind::Subagent, None, "x", "").is_err());
    }

    #[test]
    fn claude_mcp_roundtrip_keeps_other_keys_and_secrets() {
        let (tmp, store, _) = setup();
        let file = tmp.path().join(".claude.json");
        fs::write(&file, r#"{"numStartups": 5, "mcpServers": {"github": {"command": "npx", "env": {"GITHUB_TOKEN": "real"}}}, "zLast": true}"#).unwrap();
        let edited = serde_json::json!({"command": "npx", "args": ["-y", "gh"], "env": {"GITHUB_TOKEN": MASK}});
        save_mcp_server(&store, &file, None, "github", edited, None).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["github"]["env"]["GITHUB_TOKEN"], "real");
        assert_eq!(v["mcpServers"]["github"]["args"][1], "gh");
        assert_eq!(v["numStartups"], 5);
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["numStartups", "mcpServers", "zLast"]);

        let project = Path::new("/work/repo");
        save_mcp_server(&store, &file, Some(project), "db", serde_json::json!({"command": "pg"}), None).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["projects"]["/work/repo"]["mcpServers"]["db"]["command"], "pg");

        let before = fs::read_to_string(&file).unwrap();
        save_mcp_server(&store, &file, None, "linear", serde_json::json!({"command": "x"}), None).unwrap();
        assert!(save_mcp_server(&store, &file, None, "linear", serde_json::json!({"command": "y"}), Some("github")).is_err(), "rename onto an existing server");
        assert_ne!(before, fs::read_to_string(&file).unwrap());
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["linear"]["command"], "x");
        assert!(v["mcpServers"].get("github").is_some());

        delete_mcp_server(&store, &file, None, "github").unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(v["mcpServers"].get("github").is_none());
    }

    #[test]
    fn codex_mcp_keeps_comments_and_toggles() {
        let (tmp, store, _) = setup();
        let file = tmp.path().join("config.toml");
        fs::write(&file, "# my settings\nmodel = \"gpt-5\"\n\n[mcp_servers.github]\ncommand = \"npx\"\nenv = { GITHUB_TOKEN = \"real\" }\n").unwrap();
        save_mcp_server(&store, &file, None, "gh", serde_json::json!({"command": "npx", "args": ["gh"], "env": {"GITHUB_TOKEN": MASK}}), Some("github")).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# my settings\nmodel = \"gpt-5\""));
        let t: toml::Table = text.parse().unwrap();
        assert!(t["mcp_servers"].get("github").is_none());
        assert_eq!(t["mcp_servers"]["gh"]["env"]["GITHUB_TOKEN"].as_str(), Some("real"));

        set_codex_mcp_enabled(&store, &file, "gh", false).unwrap();
        let t: toml::Table = fs::read_to_string(&file).unwrap().parse().unwrap();
        assert_eq!(t["mcp_servers"]["gh"]["enabled"].as_bool(), Some(false));
        set_codex_mcp_enabled(&store, &file, "gh", true).unwrap();
        let t: toml::Table = fs::read_to_string(&file).unwrap().parse().unwrap();
        assert!(t["mcp_servers"]["gh"].get("enabled").is_none());
    }

    #[test]
    fn codex_skill_toggle() {
        let (tmp, store, _) = setup();
        let file = tmp.path().join("config.toml");
        fs::write(&file, "model = \"gpt-5\"\n").unwrap();
        let skill = tmp.path().join("skills/pdf/SKILL.md");
        set_codex_skill_enabled(&store, &file, &skill, false).unwrap();
        let t: toml::Table = fs::read_to_string(&file).unwrap().parse().unwrap();
        let entry = &t["skills"]["config"][0];
        assert_eq!(entry["enabled"].as_bool(), Some(false));
        assert_eq!(entry["path"].as_str().map(PathBuf::from), Some(skill.parent().unwrap().to_path_buf()));
        set_codex_skill_enabled(&store, &file, &skill, true).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap().trim(), "model = \"gpt-5\"");
    }

    #[test]
    fn renames_skill_folder_and_frontmatter() {
        let (tmp, store, _) = setup();
        let file = tmp.path().join("skills/old/SKILL.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "---\nname: old\ndescription: d\n---\nbody\n").unwrap();
        let entries = rename_skill(&store, &file, "New Name").unwrap();
        assert_eq!(entries.len(), 2);
        let text = fs::read_to_string(tmp.path().join("skills/new-name/SKILL.md")).unwrap();
        assert_eq!(text, "---\nname: new-name\ndescription: d\n---\nbody\n");
        assert!(!tmp.path().join("skills/old").exists());
    }
}
