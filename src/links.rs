//! Manual chain-link storage — strictly decoupled from the customer repo.
//!
//! Manual links describe hops invisible to static workflow.json analysis
//! (EventGrid subscriptions, dynamic queue routing). They are tool metadata,
//! so they live in the tool home `~/.ais/chains/<project-key>.txt`, never in
//! the customer's repository.
//!
//! Resolution order on load:
//!   1. `~/.ais/chains/<project-key>.txt` — the canonical tool-side file.
//!   2. Legacy/opt-in `<logic_apps_dir>/.ais-chain` in the repo — read-only.
//!      When found and no tool-side file exists yet, it is migrated (copied)
//!      to the tool-side path so subsequent loads use the tool home.
//!   3. Nothing — empty link set.
//!
//! Tools MUST write via [`save`], which only ever touches the tool home.

use std::fs;
use std::path::{Path, PathBuf};

/// Where a loaded link set came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkSource {
    ToolHome,
    RepoFile,
    None,
}

#[derive(Debug)]
pub struct LoadedLinks {
    pub links: Vec<String>,
    pub source: LinkSource,
    /// Human-readable notices: migration performed, validation problems, …
    pub warnings: Vec<String>,
}

/// Stable, human-debuggable key for a project directory:
/// `<last-path-component>-<8-hex-hash-of-full-path>`.
pub fn project_key(logic_apps_dir: &Path) -> String {
    let canon = logic_apps_dir
        .canonicalize()
        .unwrap_or_else(|_| logic_apps_dir.to_path_buf());
    let full = canon.to_string_lossy();

    // FNV-1a — no external dependency needed for a filename-grade hash.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in full.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    let slug: String = canon
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();

    format!("{slug}-{:08x}", (hash >> 32) as u32)
}

/// Tool-home path for a project's links file: `~/.ais/chains/<key>.txt`.
pub fn links_path(logic_apps_dir: &Path) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ais")
        .join("chains")
        .join(format!("{}.txt", project_key(logic_apps_dir)))
}

fn parse_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Load manual links for a project. See module docs for resolution order.
pub fn load(logic_apps_dir: &Path) -> LoadedLinks {
    let tool_path = links_path(logic_apps_dir);
    let mut warnings = Vec::new();

    if let Ok(content) = fs::read_to_string(&tool_path) {
        return LoadedLinks { links: parse_lines(&content), source: LinkSource::ToolHome, warnings };
    }

    // Legacy / opt-in repo file. Read-only; migrate a copy to the tool home.
    let repo_path = logic_apps_dir.join(".ais-chain");
    if let Ok(content) = fs::read_to_string(&repo_path) {
        match save(logic_apps_dir, &content) {
            Ok(()) => warnings.push(format!(
                "Migrated repo {} to tool home {} — the repo copy can be deleted.",
                repo_path.display(), tool_path.display()
            )),
            Err(e) => warnings.push(format!(
                "Found repo {} but could not migrate to tool home: {e}",
                repo_path.display()
            )),
        }
        return LoadedLinks { links: parse_lines(&content), source: LinkSource::RepoFile, warnings };
    }

    LoadedLinks { links: Vec::new(), source: LinkSource::None, warnings }
}

/// Write the project's links file in the tool home. Never touches the repo.
pub fn save(logic_apps_dir: &Path, content: &str) -> std::io::Result<()> {
    let path = links_path(logic_apps_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)
}

/// Validate links against the set of known workflow names.
/// Returns one warning per link that references an unknown workflow —
/// typically a renamed or deleted workflow, or a typo.
pub fn validate(links: &[String], workflow_names: &[&str]) -> Vec<String> {
    let known: std::collections::HashSet<&str> = workflow_names.iter().copied().collect();
    let mut warnings = Vec::new();
    for link in links {
        let Some((from, rest)) = link.split_once("->") else {
            warnings.push(format!("Malformed link (expected Source->Target:label): {link}"));
            continue;
        };
        let to = rest.split(':').next().unwrap_or(rest).trim();
        let from = from.trim();
        if !known.contains(from) {
            warnings.push(format!("Link references unknown workflow '{from}': {link}"));
        }
        if !known.contains(to) {
            warnings.push(format!("Link references unknown workflow '{to}': {link}"));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_and_sluggy() {
        let k1 = project_key(Path::new("/tmp/some/logic_apps"));
        let k2 = project_key(Path::new("/tmp/some/logic_apps"));
        assert_eq!(k1, k2);
        assert!(k1.starts_with("logic_apps-"));
    }

    #[test]
    fn parse_skips_comments_and_blanks() {
        let lines = parse_lines("# c\n\nA->B:x\n  C->D:y  \n");
        assert_eq!(lines, vec!["A->B:x", "C->D:y"]);
    }

    #[test]
    fn validate_flags_unknown_names() {
        let links = vec!["A->B:EventGrid".to_string(), "A->Missing:q".to_string(), "junk".to_string()];
        let w = validate(&links, &["A", "B"]);
        assert_eq!(w.len(), 2);
        assert!(w[0].contains("Missing"));
        assert!(w[1].contains("Malformed"));
    }
}
