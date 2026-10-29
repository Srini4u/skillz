//! OpenAI Codex CLI: `~/.codex/config.toml`, `AGENTS.md`, and skills in `.agents/skills`.

use crate::model::{Agent, Asset, AssetKind, Scope};
use crate::util::{file_asset, markdown_in, mask_secrets, skills_in, version_of, which};
use crate::model::LoadedFile;
use crate::{ancestors_top_down, loaded, project_dirs, Adapter, ScanContext};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Codex;

impl Adapter for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    /// The global AGENTS.md, then one file per folder from the git root down to the project.
    /// In each place AGENTS.override.md wins over AGENTS.md.
    fn load_order(&self, ctx: &ScanContext, project: &Path) -> Vec<LoadedFile> {
        let mut out: Vec<LoadedFile> = Vec::new();
        out.extend(pick(&codex_dir(ctx), Scope::User, "user instructions"));
        let root = project.ancestors().find(|p| p.join(".git").exists()).unwrap_or(project);
        for folder in ancestors_top_down(project).into_iter().filter(|f| f.starts_with(root)) {
            let reason = if folder == project { "project" } else { "parent folder" };
            out.extend(pick(&folder, Scope::Project { path: project.to_path_buf() }, reason));
        }
        out
    }

    fn scan(&self, ctx: &ScanContext) -> Agent {
        let dir = codex_dir(ctx);
        let config_file = dir.join("config.toml");
        let user_skills = ctx.home.join(".agents").join("skills");
        let system_skills = ctx.system_root.join("etc").join("codex").join("skills");
        let binary = ctx.probe_binaries.then(|| which("codex")).flatten();
        let mut agent = Agent {
            id: "codex".into(),
            name: "Codex CLI".into(),
            installed: binary.is_some() || dir.is_dir(),
            version: binary.as_deref().and_then(version_of),
            binary,
            roots: vec![dir.clone(), user_skills.clone()],
            assets: Vec::new(),
            known_projects: Vec::new(),
            warnings: Vec::new(),
        };

        let config = read_toml(&config_file, &mut agent.warnings);
        if let Some(projects) = config.as_ref().and_then(|c| c.get("projects")).and_then(|p| p.as_table()) {
            agent.known_projects = projects.keys().map(PathBuf::from).collect();
        }
        let disabled = disabled_skills(config.as_ref());

        // User and system scope.
        let user = Scope::User;
        for file in [dir.join("AGENTS.md"), dir.join("AGENTS.override.md")] {
            agent.assets.extend(file_asset(file, AssetKind::Instruction, &user));
        }
        // ~/.agents/skills is current; ~/.codex/skills is where older releases looked.
        agent.assets.extend(skills_in(&user_skills, &user));
        agent.assets.extend(skills_in(&dir.join("skills"), &user));
        agent.assets.extend(skills_in(&system_skills, &Scope::System));
        agent.assets.extend(markdown_in(&dir.join("prompts"), AssetKind::Command, &user, 0));
        agent.assets.extend(file_asset(config_file.clone(), AssetKind::Settings, &user));
        if let Some(servers) = config.as_ref().and_then(|c| c.get("mcp_servers")).and_then(|s| s.as_table()) {
            for (name, cfg) in servers {
                let mut asset = Asset::new(AssetKind::McpServer, name, user.clone(), config_file.clone());
                let mut details = serde_json::to_value(cfg).unwrap_or_default();
                mask_secrets(&mut details);
                asset.description = cfg.get("command").or_else(|| cfg.get("url")).and_then(|v| v.as_str()).map(str::to_string);
                asset.enabled = cfg.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                asset.details = details;
                agent.assets.push(asset);
            }
        }

        // Project scope.
        for project in project_dirs(ctx, &agent.known_projects) {
            let scope = Scope::Project { path: project.clone() };
            for file in [project.join("AGENTS.md"), project.join("AGENTS.override.md")] {
                agent.assets.extend(file_asset(file, AssetKind::Instruction, &scope));
            }
            agent.assets.extend(skills_in(&project.join(".agents").join("skills"), &scope));
            agent.assets.extend(skills_in(&project.join(".codex").join("skills"), &scope));
        }

        for asset in agent.assets.iter_mut().filter(|a| a.kind == AssetKind::Skill) {
            let folder = asset.path.parent().map(Path::to_path_buf);
            if disabled.iter().any(|d| *d == asset.path || Some(d) == folder.as_ref()) {
                asset.enabled = false;
            }
        }
        agent
    }
}

fn codex_dir(ctx: &ScanContext) -> PathBuf {
    ctx.codex_home.clone().unwrap_or_else(|| ctx.home.join(".codex"))
}

fn pick(folder: &Path, scope: Scope, reason: &str) -> Option<LoadedFile> {
    loaded(folder.join("AGENTS.override.md"), AssetKind::Instruction, scope.clone(), reason)
        .or_else(|| loaded(folder.join("AGENTS.md"), AssetKind::Instruction, scope, reason))
}

/// Paths listed as `[[skills.config]] path = "…" enabled = false`.
fn disabled_skills(config: Option<&toml::Table>) -> Vec<PathBuf> {
    let Some(entries) = config.and_then(|c| c.get("skills")).and_then(|s| s.get("config")).and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|e| e.get("enabled").and_then(|v| v.as_bool()) == Some(false))
        .filter_map(|e| e.get("path").and_then(|p| p.as_str()).map(PathBuf::from))
        .collect()
}

fn read_toml(path: &Path, warnings: &mut Vec<String>) -> Option<toml::Table> {
    let text = fs::read_to_string(path).ok()?;
    match text.parse::<toml::Table>() {
        Ok(t) => Some(t),
        Err(e) => {
            warnings.push(format!("{}: {e}", path.display()));
            None
        }
    }
}
