//! A checklist per agent of what makes it "well rounded", with a fix for each gap where one
//! can be applied automatically.

use crate::model::{Agent, AssetKind, LoadedFile, Scope};
use crate::sync::CopyRequest;
use crate::util::which;
use crate::{load_order, ScanContext};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Instruction files over this size eat a noticeable share of context on every session.
const BIG_INSTRUCTIONS: u64 = 40 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fix {
    /// Create an empty instruction file from the template.
    Create { agent_id: String, kind: AssetKind, project: Option<PathBuf> },
    /// Copy what another agent already has.
    Copy { request: CopyRequest },
    /// Open a file to fix it by hand.
    Open { path: PathBuf },
    /// Look at the coverage heatmap.
    Coverage,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    pub ok: bool,
    /// "warn" for things that break or mislead the agent, "info" for things that would help.
    pub severity: &'static str,
    pub title: String,
    pub detail: String,
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Health {
    pub agent_id: String,
    /// Share of checks passed, 0 to 100.
    pub score: u8,
    pub checks: Vec<Check>,
}

pub fn health(agents: &[Agent], ctx: &ScanContext) -> Vec<Health> {
    let projects: BTreeSet<PathBuf> = agents.iter().flat_map(|a| a.known_projects.iter().cloned()).chain(ctx.projects.iter().cloned()).filter(|p| p.is_dir()).collect();
    agents.iter().filter(|a| a.installed).map(|a| check_agent(a, agents, ctx, &projects)).collect()
}

fn check_agent(agent: &Agent, all: &[Agent], ctx: &ScanContext, projects: &BTreeSet<PathBuf>) -> Health {
    let others: Vec<&Agent> = all.iter().filter(|o| o.id != agent.id && o.installed).collect();
    let of = |k: AssetKind| agent.assets.iter().filter(move |x| x.kind == k);
    let mut checks = Vec::new();
    let mut push = |id: &str, ok: bool, severity, title: &str, detail: String, fix: Option<Fix>| {
        checks.push(Check { id: id.into(), ok, severity, title: title.into(), detail, fix: if ok { None } else { fix } });
    };

    // User-level instructions.
    let has_user = of(AssetKind::Instruction).any(|x| x.scope == Scope::User);
    let donor = others.iter().find_map(|o| o.assets.iter().find(|x| x.kind == AssetKind::Instruction && x.scope == Scope::User).map(|x| (o, x)));
    let fix = match donor {
        Some((o, x)) => Fix::Copy { request: CopyRequest { from_agent: o.id.clone(), kind: AssetKind::Instruction, name: x.name.clone(), path: x.path.clone(), project: None, to_agent: agent.id.clone(), to_project: None } },
        None => Fix::Create { agent_id: agent.id.clone(), kind: AssetKind::Instruction, project: None },
    };
    push("user-instructions", has_user, "info", "Has instructions for every project", if has_user { "Found".into() } else { "No user-level instruction file".into() }, Some(fix));

    // Instructions in each project.
    for project in projects {
        let stacks = load_order(ctx, project);
        // Project instructions, including ones inherited from a parent folder of the repo.
        let project_file = |f: &&LoadedFile| matches!(&f.scope, Scope::Project { .. }) && f.kind == AssetKind::Instruction;
        let has = |id: &str| stacks.iter().find(|s| s.agent_id == id).is_some_and(|s| s.files.iter().any(|f| project_file(&f)));
        let ok = has(&agent.id);
        let name = project.file_name().unwrap_or_default().to_string_lossy();
        let donor = others.iter().find(|o| has(&o.id)).and_then(|o| {
            let file = stacks.iter().find(|s| s.agent_id == o.id)?.files.iter().rev().find(project_file)?.clone();
            Some((o, file))
        });
        let fix = match donor {
            Some((o, f)) => Fix::Copy {
                request: CopyRequest {
                    from_agent: o.id.clone(),
                    kind: AssetKind::Instruction,
                    name: f.path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                    path: f.path.clone(),
                    project: Some(project.clone()),
                    to_agent: agent.id.clone(),
                    to_project: Some(project.clone()),
                },
            },
            None => Fix::Create { agent_id: agent.id.clone(), kind: AssetKind::Instruction, project: Some(project.clone()) },
        };
        push(&format!("project-instructions:{}", project.display()), ok, "warn", &format!("Knows the rules of {name}"), if ok { "Loads a project instruction file".into() } else { format!("Loads no instructions in {}", project.display()) }, Some(fix));
    }

    let skills = of(AssetKind::Skill).count();
    push("has-skills", skills > 0, "info", "Has skills", format!("{skills} skills"), Some(Fix::Coverage));
    let servers = of(AssetKind::McpServer).count();
    push("has-mcp", servers > 0, "info", "Has MCP servers", format!("{servers} MCP servers"), Some(Fix::Coverage));

    // Skills without a description are rarely picked up automatically.
    let undescribed: Vec<_> = of(AssetKind::Skill).filter(|x| x.description.as_deref().unwrap_or("").trim().is_empty()).collect();
    push(
        "skills-described",
        undescribed.is_empty(),
        "warn",
        "Every skill says when to use it",
        if undescribed.is_empty() { "All skills have a description".into() } else { format!("No description: {}", names(&undescribed)) },
        undescribed.first().map(|x| Fix::Open { path: x.path.clone() }),
    );

    let mut seen = BTreeSet::new();
    let dupes: BTreeSet<&str> = of(AssetKind::Skill).filter(|x| !seen.insert(x.name.as_str())).map(|x| x.name.as_str()).collect();
    push(
        "no-duplicate-skills",
        dupes.is_empty(),
        "warn",
        "No two skills share a name",
        if dupes.is_empty() { "Names are unique".into() } else { format!("Defined more than once: {}", dupes.into_iter().collect::<Vec<_>>().join(", ")) },
        None,
    );

    // A stdio server whose command isn't installed fails at startup.
    let broken: Vec<_> = of(AssetKind::McpServer)
        .filter(|x| x.enabled)
        .filter(|x| {
            let Some(cmd) = x.details.get("command").and_then(|c| c.as_str()) else { return false };
            let p = PathBuf::from(cmd);
            if p.is_absolute() {
                !p.exists()
            } else {
                which(cmd).is_none()
            }
        })
        .collect();
    push(
        "mcp-commands",
        broken.is_empty(),
        "warn",
        "Every MCP server can start",
        if broken.is_empty() { "All commands found".into() } else { format!("Command not found on PATH: {}", names(&broken)) },
        broken.first().map(|x| Fix::Open { path: x.path.clone() }),
    );

    let big: Vec<_> = of(AssetKind::Instruction).filter(|x| x.bytes.unwrap_or(0) > BIG_INSTRUCTIONS).collect();
    push(
        "instructions-size",
        big.is_empty(),
        "info",
        "Instruction files stay small",
        if big.is_empty() { "All under 40 KB".into() } else { format!("Over 40 KB, loaded every session: {}", big.iter().map(|x| x.path.display().to_string()).collect::<Vec<_>>().join(", ")) },
        big.first().map(|x| Fix::Open { path: x.path.clone() }),
    );

    // What the other agents can do that this one can't.
    let have = |k: AssetKind, n: &str| agent.assets.iter().any(|x| x.kind == k && x.name == n);
    let missing: BTreeSet<String> = others
        .iter()
        .flat_map(|o| o.assets.iter())
        .filter(|x| matches!(x.kind, AssetKind::Skill | AssetKind::McpServer) && !have(x.kind, &x.name))
        .map(|x| x.name.clone())
        .collect();
    push(
        "parity",
        missing.is_empty(),
        "info",
        "Has the skills and MCP servers your other agents have",
        if missing.is_empty() { "Nothing missing".into() } else { format!("Missing {}: {}", missing.len(), missing.into_iter().collect::<Vec<_>>().join(", ")) },
        Some(Fix::Coverage),
    );

    push("config-readable", agent.warnings.is_empty(), "warn", "All config files parse", if agent.warnings.is_empty() { "No errors".into() } else { agent.warnings.join("; ") }, None);

    let ok = checks.iter().filter(|c| c.ok).count();
    let score = if checks.is_empty() { 100 } else { (100 * ok / checks.len()) as u8 };
    Health { agent_id: agent.id.clone(), score, checks }
}

fn names(xs: &[&crate::model::Asset]) -> String {
    xs.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;
    use std::fs;

    #[test]
    fn finds_gaps_and_offers_fixes() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let repo = home.join("repo");
        let w = |p: PathBuf, t: &str| {
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, t).unwrap();
        };
        w(home.join(".claude/CLAUDE.md"), "# rules");
        w(home.join(".claude/skills/a/SKILL.md"), "---\nname: a\n---\n");
        w(repo.join("CLAUDE.md"), "# repo");
        let state = serde_json::json!({"mcpServers": {"gone": {"command": "/nope/missing-binary"}}, "projects": {repo.to_string_lossy(): {}}});
        w(home.join(".claude.json"), &state.to_string());
        fs::create_dir_all(home.join(".codex")).unwrap();

        let ctx = ScanContext { home: home.clone(), codex_home: None, system_root: home.clone(), projects: vec![], probe_binaries: false };
        let report = health(&scan(&ctx), &ctx);
        let get = |agent: &str, id: &str| report.iter().find(|h| h.agent_id == agent).unwrap().checks.iter().find(|c| c.id.starts_with(id)).unwrap().clone();

        assert!(get("claude-code", "user-instructions").ok);
        assert!(!get("claude-code", "skills-described").ok);
        assert!(!get("claude-code", "mcp-commands").ok);

        let codex_user = get("codex", "user-instructions");
        assert!(!codex_user.ok);
        assert!(matches!(&codex_user.fix, Some(Fix::Copy { request }) if request.path == home.join(".claude/CLAUDE.md")));
        let codex_repo = get("codex", "project-instructions");
        assert!(matches!(&codex_repo.fix, Some(Fix::Copy { request }) if request.path == repo.join("CLAUDE.md") && request.to_project.as_deref() == Some(repo.as_path())));
        assert!(get("codex", "parity").detail.contains("a"));
        let codex = report.iter().find(|h| h.agent_id == "codex").unwrap();
        assert_eq!(codex.score, 50, "5 of 10 checks fail");
    }

    #[test]
    fn inherited_instructions_are_offered_as_the_donor() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let app = home.join("code/app");
        fs::create_dir_all(&app).unwrap();
        fs::write(home.join("code/CLAUDE.md"), "# shared rules").unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        let ctx = ScanContext { home: home.clone(), codex_home: None, system_root: home.clone(), projects: vec![app.clone()], probe_binaries: false };
        let report = health(&scan(&ctx), &ctx);
        let check = |agent: &str| report.iter().find(|h| h.agent_id == agent).unwrap().checks.iter().find(|c| c.id.starts_with("project-instructions")).unwrap().clone();
        assert!(check("claude-code").ok, "a parent folder's CLAUDE.md counts");
        let codex = check("codex");
        assert!(!codex.ok);
        assert!(matches!(&codex.fix, Some(Fix::Copy { request }) if request.path == home.join("code/CLAUDE.md") && request.to_project.as_deref() == Some(app.as_path())));
    }
}
