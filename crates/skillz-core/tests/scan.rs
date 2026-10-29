use skillz_core::{scan, Agent, AssetKind, ScanContext, Scope};
use std::fs;
use std::path::Path;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn names(agent: &Agent, kind: AssetKind) -> Vec<String> {
    let mut v: Vec<String> = agent.assets.iter().filter(|a| a.kind == kind).map(|a| a.name.clone()).collect();
    v.sort();
    v
}

#[test]
fn finds_claude_and_codex_assets() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    let sys = tmp.path().join("sys");

    // Claude Code
    write(&home.join(".claude/CLAUDE.md"), "# global rules");
    write(&home.join(".claude/skills/pdf-review/SKILL.md"), "---\nname: pdf-review\ndescription: Review PDFs\n---\n");
    write(&home.join(".claude/agents/reviewer.md"), "---\nname: reviewer\n---\n");
    write(&home.join(".claude/commands/git/ship.md"), "ship it");
    write(&home.join(".claude/settings.json"), "{}");
    let encoded: String = repo.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    write(&home.join(".claude/projects").join(encoded).join("memory/MEMORY.md"), "- fact");
    let state = serde_json::json!({
        "mcpServers": {"github": {"command": "npx", "env": {"GITHUB_TOKEN": "secret"}}},
        "projects": {repo.to_string_lossy(): {"mcpServers": {"db": {"command": "pg-mcp"}}}}
    });
    write(&home.join(".claude.json"), &state.to_string());
    write(&repo.join("CLAUDE.md"), "# repo rules");
    write(&repo.join(".claude/skills/deploy/SKILL.md"), "---\nname: deploy\n---\n");
    write(&repo.join(".mcp.json"), r#"{"mcpServers": {"docs": {"type": "http", "url": "https://example.com/mcp"}}}"#);

    // Codex
    write(&home.join(".codex/AGENTS.md"), "# codex rules");
    write(&home.join(".agents/skills/pdf-review/SKILL.md"), "---\nname: pdf-review\n---\n");
    write(&home.join(".codex/skills/legacy/SKILL.md"), "---\nname: legacy\n---\n");
    write(&home.join(".codex/prompts/fix.md"), "fix");
    write(&sys.join("etc/codex/skills/plan/SKILL.md"), "---\nname: plan\n---\n");
    let legacy = home.join(".codex/skills/legacy");
    write(
        &home.join(".codex/config.toml"),
        &format!(
            "model = \"gpt-5\"\n[mcp_servers.github]\ncommand = \"npx\"\nenv = {{ GITHUB_TOKEN = \"secret\" }}\n[mcp_servers.off]\ncommand = \"x\"\nenabled = false\n[[skills.config]]\npath = \"{}\"\nenabled = false\n[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            legacy.display(),
            repo.display()
        ),
    );
    write(&repo.join("AGENTS.md"), "# repo agents");
    write(&repo.join(".agents/skills/deploy/SKILL.md"), "---\nname: deploy\n---\n");

    let ctx = ScanContext { home: home.clone(), codex_home: None, system_root: sys, projects: vec![], probe_binaries: false };
    let agents = scan(&ctx);
    let claude = agents.iter().find(|a| a.id == "claude-code").unwrap();
    let codex = agents.iter().find(|a| a.id == "codex").unwrap();

    assert!(claude.installed && codex.installed);
    assert_eq!(claude.known_projects, vec![repo.clone()]);
    assert_eq!(codex.known_projects, vec![repo.clone()]);

    assert_eq!(names(claude, AssetKind::Instruction), ["CLAUDE.md", "CLAUDE.md"]);
    assert_eq!(names(claude, AssetKind::Skill), ["deploy", "pdf-review"]);
    assert_eq!(names(claude, AssetKind::Subagent), ["reviewer"]);
    assert_eq!(names(claude, AssetKind::Command), ["git:ship"]);
    assert_eq!(names(claude, AssetKind::McpServer), ["db", "docs", "github"]);
    assert_eq!(names(claude, AssetKind::Memory), ["MEMORY"]);
    let gh = claude.assets.iter().find(|a| a.name == "github").unwrap();
    assert_eq!(gh.details["env"]["GITHUB_TOKEN"], "••••••");
    let db = claude.assets.iter().find(|a| a.name == "db").unwrap();
    assert_eq!(db.scope, Scope::Project { path: repo.clone() });

    assert_eq!(names(codex, AssetKind::Instruction), ["AGENTS.md", "AGENTS.md"]);
    assert_eq!(names(codex, AssetKind::Skill), ["deploy", "legacy", "pdf-review", "plan"]);
    assert_eq!(names(codex, AssetKind::McpServer), ["github", "off"]);
    assert_eq!(names(codex, AssetKind::Command), ["fix"]);
    let off = codex.assets.iter().find(|a| a.name == "off").unwrap();
    assert!(!off.enabled);
    let legacy_skill = codex.assets.iter().find(|a| a.name == "legacy").unwrap();
    assert!(!legacy_skill.enabled);
    let plan = codex.assets.iter().find(|a| a.name == "plan").unwrap();
    assert_eq!(plan.scope, Scope::System);
    let cgh = codex.assets.iter().find(|a| a.name == "github").unwrap();
    assert_eq!(cgh.details["env"]["GITHUB_TOKEN"], "••••••");
}

#[test]
fn empty_home_finds_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ScanContext { home: tmp.path().into(), codex_home: None, system_root: tmp.path().into(), projects: vec![], probe_binaries: false };
    for agent in scan(&ctx) {
        assert!(!agent.installed);
        assert!(agent.assets.is_empty());
    }
}

#[test]
fn broken_config_becomes_a_warning() {
    let tmp = tempfile::tempdir().unwrap();
    write(&tmp.path().join(".codex/config.toml"), "not = [valid");
    write(&tmp.path().join(".claude.json"), "{oops");
    let ctx = ScanContext { home: tmp.path().into(), codex_home: None, system_root: tmp.path().into(), projects: vec![], probe_binaries: false };
    for agent in scan(&ctx) {
        assert_eq!(agent.warnings.len(), 1, "{}", agent.name);
    }
}

#[test]
fn load_order_follows_each_agents_rules() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let repo = tmp.path().join("work/repo");
    let sub = repo.join("services/api");
    fs::create_dir_all(repo.join(".git")).unwrap();
    write(&home.join(".claude/CLAUDE.md"), "user");
    write(&tmp.path().join("work/CLAUDE.md"), "parent");
    write(&repo.join("CLAUDE.md"), "repo");
    write(&sub.join("CLAUDE.local.md"), "local");
    let encoded: String = sub.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    write(&home.join(".claude/projects").join(encoded).join("memory/MEMORY.md"), "mem");
    write(&home.join(".codex/AGENTS.md"), "user");
    write(&home.join(".codex/AGENTS.override.md"), "override wins");
    write(&tmp.path().join("work/AGENTS.md"), "above git root, ignored");
    write(&repo.join("AGENTS.md"), "repo");
    write(&sub.join("AGENTS.md"), "sub");

    let ctx = ScanContext { home: home.clone(), codex_home: None, system_root: tmp.path().into(), projects: vec![], probe_binaries: false };
    let stacks = skillz_core::load_order(&ctx, &sub);
    let rel = |id: &str| -> Vec<String> {
        stacks.iter().find(|s| s.agent_id == id).unwrap().files.iter()
            .map(|f| f.path.strip_prefix(tmp.path()).unwrap().to_string_lossy().to_string())
            .collect()
    };
    let claude = rel("claude-code");
    assert_eq!(claude[..4], ["home/.claude/CLAUDE.md", "work/CLAUDE.md", "work/repo/CLAUDE.md", "work/repo/services/api/CLAUDE.local.md"]);
    assert!(claude[4].ends_with("memory/MEMORY.md"));
    assert_eq!(rel("codex"), ["home/.codex/AGENTS.override.md", "work/repo/AGENTS.md", "work/repo/services/api/AGENTS.md"]);
}

#[test]
fn search_matches_content_and_names() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_text = "---\nname: deploy\n---\nRun the Terraform plan first.";
    write(&tmp.path().join(".claude/skills/deploy/SKILL.md"), skill_text);
    write(&tmp.path().join(".claude.json"), r#"{"mcpServers": {"postgres": {"command": "pg-mcp"}}}"#);
    let ctx = ScanContext { home: tmp.path().into(), codex_home: None, system_root: tmp.path().into(), projects: vec![], probe_binaries: false };
    let agents = scan(&ctx);
    let hits = skillz_core::search(&agents, "terraform");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, Some(4));
    assert_eq!(hits[0].snippet, "Run the Terraform plan first.");
    assert_eq!(skillz_core::search(&agents, "pg-mcp").len(), 1);
    assert!(skillz_core::search(&agents, "  ").is_empty());
    let skill = &agents[0].assets[hits[0].asset];
    assert_eq!(skill.bytes, Some(skill_text.len() as u64));
}
