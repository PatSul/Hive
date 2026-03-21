//! File-based persistence for specs and changes.
//!
//! Writes specs alongside code in `specs/{domain}/` directories and changes
//! in `changes/active/{name}/` directories, following the OpenSpec convention.

use std::path::{Path, PathBuf};

use crate::changes::Change;
use crate::specs::Spec;

/// Build the file path for a spec: `{base}/specs/{domain}/{name}.json`
/// If domain is empty, the spec goes directly in `{base}/specs/{name}.json`.
pub fn spec_file_path(base: &Path, domain: &str, name: &str) -> PathBuf {
    if domain.is_empty() {
        base.join("specs").join(format!("{name}.json"))
    } else {
        base.join("specs").join(domain).join(format!("{name}.json"))
    }
}

/// Build the directory path for an active change.
pub fn change_dir_path(base: &Path, change_name: &str) -> PathBuf {
    base.join("changes").join("active").join(change_name)
}

/// Build the directory path for an archived change.
pub fn archive_dir_path(base: &Path, date_prefix: &str, change_name: &str) -> PathBuf {
    base.join("changes").join("archive").join(format!("{date_prefix}-{change_name}"))
}

/// Save a spec to disk as JSON.
pub fn save_spec(base: &Path, spec: &Spec) -> Result<(), String> {
    let domain = spec.domain.as_deref().unwrap_or("");
    let path = spec_file_path(base, domain, &spec.title);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create spec directory: {e}"))?;
    }

    let json = serde_json::to_string_pretty(spec)
        .map_err(|e| format!("Failed to serialize spec: {e}"))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write spec file: {e}"))?;

    Ok(())
}

/// Load a spec from a JSON file.
pub fn load_spec(path: &Path) -> Result<Spec, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read spec file: {e}"))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse spec: {e}"))
}

/// Save a change to disk (creates `{base}/changes/active/{name}/change.json`).
pub fn save_change(base: &Path, change: &Change) -> Result<(), String> {
    let dir = change_dir_path(base, &change.name);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create change directory: {e}"))?;

    let path = dir.join("change.json");
    let json = serde_json::to_string_pretty(change)
        .map_err(|e| format!("Failed to serialize change: {e}"))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write change file: {e}"))?;

    Ok(())
}

/// Load a change from a JSON file.
pub fn load_change(path: &Path) -> Result<Change, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read change file: {e}"))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse change: {e}"))
}

/// List all spec files under `{base}/specs/`, recursively.
pub fn list_specs(base: &Path) -> Result<Vec<PathBuf>, String> {
    let specs_dir = base.join("specs");
    if !specs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    collect_json_files(&specs_dir, &mut results)
        .map_err(|e| format!("Failed to list specs: {e}"))?;
    Ok(results)
}

/// Recursively collect `.json` files from a directory.
fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::{ChangeProposal, DeltaEntry, DeltaKind};

    #[test]
    fn spec_path_with_domain() {
        let path = spec_file_path(Path::new("/project"), "auth", "login");
        assert_eq!(path, PathBuf::from("/project/specs/auth/login.json"));
    }

    #[test]
    fn spec_path_without_domain() {
        let path = spec_file_path(Path::new("/project"), "", "core");
        assert_eq!(path, PathBuf::from("/project/specs/core.json"));
    }

    #[test]
    fn change_path() {
        let path = change_dir_path(Path::new("/project"), "add-oauth");
        assert_eq!(path, PathBuf::from("/project/changes/active/add-oauth"));
    }

    #[test]
    fn archive_path() {
        let path = archive_dir_path(Path::new("/project"), "2026-03-15", "add-oauth");
        assert_eq!(path, PathBuf::from("/project/changes/archive/2026-03-15-add-oauth"));
    }

    #[test]
    fn save_and_load_spec_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut spec = Spec::new("spec-1", "Auth Spec", "Authentication behaviors");
        spec.domain = Some("auth".into());

        save_spec(base, &spec).unwrap();
        let loaded = load_spec(&spec_file_path(base, "auth", "Auth Spec")).unwrap();
        assert_eq!(loaded.id, "spec-1");
        assert_eq!(loaded.title, "Auth Spec");
        assert_eq!(loaded.domain.as_deref(), Some("auth"));
    }

    #[test]
    fn save_and_load_change_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut change = Change::new("add-oauth", "spec-1");
        change.proposal = Some(ChangeProposal::new("Add OAuth", "No auth"));
        change.delta_entries.push(DeltaEntry::new(
            "d1", DeltaKind::Added, "Requirements", "OAuth", "MUST support",
        ));

        save_change(base, &change).unwrap();
        let change_dir = change_dir_path(base, "add-oauth");
        let loaded = load_change(&change_dir.join("change.json")).unwrap();
        assert_eq!(loaded.name, "add-oauth");
        assert_eq!(loaded.delta_entries.len(), 1);
    }

    #[test]
    fn list_specs_finds_all() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut s1 = Spec::new("s1", "Auth", "Auth spec");
        s1.domain = Some("auth".into());
        let mut s2 = Spec::new("s2", "Billing", "Billing spec");
        s2.domain = Some("billing".into());
        let s3 = Spec::new("s3", "Core", "No domain");

        save_spec(base, &s1).unwrap();
        save_spec(base, &s2).unwrap();
        save_spec(base, &s3).unwrap();

        let found = list_specs(base).unwrap();
        assert_eq!(found.len(), 3);
    }
}
