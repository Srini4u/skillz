//! Prints what Skillz finds on this machine, to check every path before the app relies on it.
//!
//!     skillz-scan [--json] [PROJECT_DIR...]
//!     skillz-scan [--json] --load-order PROJECT_DIR

use skillz_core::{load_order, scan, AssetKind, ScanContext, Scope};
use std::path::PathBuf;

fn main() {
    let mut json = false;
    let mut order = false;
    let mut projects = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "--load-order" => order = true,
            "-h" | "--help" => {
                println!("usage: skillz-scan [--json] [PROJECT_DIR...]\n       skillz-scan [--json] --load-order PROJECT_DIR");
                return;
            }
            p => projects.push(PathBuf::from(p).canonicalize().unwrap_or_else(|_| PathBuf::from(p))),
        }
    }
    if order {
        let Some(project) = projects.first() else {
            eprintln!("--load-order needs a project folder");
            std::process::exit(2);
        };
        let stacks = load_order(&ScanContext::from_env(Vec::new()), project);
        if json {
            println!("{}", serde_json::to_string_pretty(&stacks).expect("stacks serialize"));
        } else {
            for s in stacks {
                println!("{}", s.agent_id);
                for (i, f) in s.files.iter().enumerate() {
                    println!("  {}. {} ({} bytes, {})", i + 1, f.path.display(), f.bytes, f.reason);
                }
            }
        }
        return;
    }
    let agents = scan(&ScanContext::from_env(projects));
    if json {
        println!("{}", serde_json::to_string_pretty(&agents).expect("agents serialize"));
        return;
    }
    for agent in &agents {
        let status = if agent.installed { "installed" } else { "not found" };
        println!("{} ({status}{})", agent.name, agent.version.as_deref().map(|v| format!(", {v}")).unwrap_or_default());
        for root in &agent.roots {
            println!("  {} {}", if root.exists() { "✓" } else { "✗" }, root.display());
        }
        println!("  known projects: {}", agent.known_projects.len());
        for kind in [AssetKind::Instruction, AssetKind::Memory, AssetKind::Skill, AssetKind::McpServer, AssetKind::Subagent, AssetKind::Command, AssetKind::Settings] {
            let items: Vec<_> = agent.assets.iter().filter(|a| a.kind == kind).collect();
            if items.is_empty() {
                continue;
            }
            println!("  {kind:?} ({})", items.len());
            for a in items {
                let scope = match &a.scope {
                    Scope::User => "user".to_string(),
                    Scope::System => "system".to_string(),
                    Scope::Project { path } => path.display().to_string(),
                };
                let off = if a.enabled { "" } else { " [disabled]" };
                println!("    {}{off}  [{scope}]  {}", a.name, a.path.display());
            }
        }
        for w in &agent.warnings {
            println!("  warning: {w}");
        }
        println!();
    }
}
