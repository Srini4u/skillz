use crate::model::{Asset, AssetKind, Scope};
use std::fs;
use std::path::{Path, PathBuf};

/// Pulls `name` and `description` out of a Markdown file's YAML frontmatter.
/// Handles plain, quoted and folded (`>` / `|`) values; anything fancier is ignored.
pub fn frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let block: Vec<&str> = lines.take_while(|l| l.trim() != "---").collect();
    (fm_value(&block, "name"), fm_value(&block, "description"))
}

fn fm_value(block: &[&str], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let i = block.iter().position(|l| l.starts_with(&prefix))?;
    let rest = block[i][prefix.len()..].trim();
    let value = if rest.is_empty() || rest == ">" || rest == "|" || rest == ">-" || rest == "|-" {
        block[i + 1..]
            .iter()
            .take_while(|l| l.starts_with(' ') || l.starts_with('\t'))
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        rest.trim_matches(|c| c == '"' || c == '\'').to_string()
    };
    (!value.is_empty()).then_some(value)
}

/// Reads every `<dir>/<skill>/SKILL.md` one level down.
pub fn skills_in(dir: &Path, scope: &Scope) -> Vec<Asset> {
    let mut out = Vec::new();
    for entry in sorted_entries(dir) {
        let file = entry.join("SKILL.md");
        let Ok(text) = fs::read_to_string(&file) else { continue };
        let (name, description) = frontmatter(&text);
        let folder = entry.file_name().unwrap_or_default().to_string_lossy().to_string();
        let mut asset = Asset::new(AssetKind::Skill, name.unwrap_or(folder), scope.clone(), file);
        asset.description = description;
        out.push(asset);
    }
    out
}

/// Reads every `*.md` file under `dir` (up to `depth` folders deep) as one asset each.
/// Nested files get a `folder:name` name, matching how slash commands are invoked.
pub fn markdown_in(dir: &Path, kind: AssetKind, scope: &Scope, depth: usize) -> Vec<Asset> {
    let mut out = Vec::new();
    collect_md(dir, dir, kind, scope, depth, &mut out);
    out
}

fn collect_md(base: &Path, dir: &Path, kind: AssetKind, scope: &Scope, depth: usize, out: &mut Vec<Asset>) {
    for path in sorted_entries(dir) {
        if path.is_dir() {
            if depth > 0 {
                collect_md(base, &path, kind, scope, depth - 1, out);
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            let rel = path.strip_prefix(base).unwrap_or(&path).with_extension("");
            let fallback = rel.to_string_lossy().replace(std::path::MAIN_SEPARATOR, ":");
            let text = fs::read_to_string(&path).unwrap_or_default();
            let (name, description) = frontmatter(&text);
            let mut asset = Asset::new(kind, name.unwrap_or(fallback), scope.clone(), path);
            asset.description = description;
            out.push(asset);
        }
    }
}

/// A single instruction or memory file, if it exists.
pub fn file_asset(path: PathBuf, kind: AssetKind, scope: &Scope) -> Option<Asset> {
    if !path.is_file() {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().to_string();
    Some(Asset::new(kind, name, scope.clone(), path))
}

pub fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    v.sort();
    v
}

/// Finds an executable on `PATH`.
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(bin)).find(|p| p.is_file())
}

/// Runs `<bin> --version` and returns the first line of output.
pub fn version_of(bin: &Path) -> Option<String> {
    let out = std::process::Command::new(bin).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string()).filter(|l| !l.is_empty())
}

const SECRET_HINTS: [&str; 6] = ["key", "token", "secret", "password", "auth", "credential"];

/// Replaces every env value, and any header or arg that looks like a secret, with a mask.
pub fn mask_secrets(server: &mut serde_json::Value) {
    use serde_json::Value;
    let Some(obj) = server.as_object_mut() else { return };
    for field in ["env", "headers", "http_headers", "env_http_headers"] {
        if let Some(Value::Object(map)) = obj.get_mut(field) {
            for (k, v) in map.iter_mut() {
                if field == "env" || SECRET_HINTS.iter().any(|h| k.to_lowercase().contains(h)) {
                    *v = Value::String("••••••".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter() {
        let text = "---\nname: pdf-review\ndescription: \"Review PDFs\"\n---\nbody";
        assert_eq!(frontmatter(text), (Some("pdf-review".into()), Some("Review PDFs".into())));
    }

    #[test]
    fn parses_folded_description() {
        let text = "---\nname: x\ndescription: >\n  first line\n  second line\nother: 1\n---\n";
        assert_eq!(frontmatter(text).1.as_deref(), Some("first line second line"));
    }

    #[test]
    fn no_frontmatter() {
        assert_eq!(frontmatter("# Title"), (None, None));
    }

    #[test]
    fn masks_env() {
        let mut v = serde_json::json!({"command": "npx", "env": {"GITHUB_TOKEN": "abc"}, "headers": {"X-Api-Key": "k", "Accept": "json"}});
        mask_secrets(&mut v);
        assert_eq!(v["env"]["GITHUB_TOKEN"], "••••••");
        assert_eq!(v["headers"]["X-Api-Key"], "••••••");
        assert_eq!(v["headers"]["Accept"], "json");
    }
}
