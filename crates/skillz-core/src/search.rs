use crate::model::{Agent, AssetKind, SearchHit};
use std::fs;

const MAX_FILE: u64 = 1 << 20;

/// Case-insensitive search over every asset's name, description and file contents.
/// Shared config files (MCP servers) are matched on name and details only.
pub fn search(agents: &[Agent], query: &str) -> Vec<SearchHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for agent in agents {
        for (i, asset) in agent.assets.iter().enumerate() {
            let hit = |line, snippet: &str| SearchHit { agent_id: agent.id.clone(), asset: i, line, snippet: snippet.chars().take(160).collect() };
            let meta = format!("{} {}", asset.name, asset.description.as_deref().unwrap_or_default()).to_lowercase();
            if asset.kind == AssetKind::McpServer {
                if meta.contains(&q) || asset.details.to_string().to_lowercase().contains(&q) {
                    hits.push(hit(None, asset.description.as_deref().unwrap_or_default()));
                }
                continue;
            }
            let text = fs::metadata(&asset.path)
                .ok()
                .filter(|m| m.len() <= MAX_FILE)
                .and_then(|_| fs::read_to_string(&asset.path).ok())
                .unwrap_or_default();
            if let Some((n, line)) = text.lines().enumerate().find(|(_, l)| l.to_lowercase().contains(&q)) {
                hits.push(hit(Some(n + 1), line.trim()));
            } else if meta.contains(&q) {
                hits.push(hit(None, asset.description.as_deref().unwrap_or_default()));
            }
        }
    }
    hits
}
