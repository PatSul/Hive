//! Prompt templates — save and reuse prompt configurations.
//!
//! Each template is stored as a JSON file in `~/.hive/prompts/`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A saved prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instruction: String,
    /// Relative file paths to auto-check for context.
    pub context_files: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl PromptTemplate {
    /// Create a new template with a generated ID.
    pub fn new(name: String, description: String, instruction: String) -> Self {
        let id = format!(
            "{}-{}",
            slug(&name),
            &uuid_v4_simple()[..8]
        );
        Self {
            id,
            name,
            description,
            instruction,
            context_files: Vec::new(),
            tags: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

/// Directory where prompt templates are stored.
pub fn prompts_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hive")
        .join("prompts")
}

/// Validate that a template ID contains only safe characters.
///
/// Allows `[A-Za-z0-9_-]` which matches what `slug()` + `uuid_v4_simple()` produce.
/// Blocks path traversal attacks (e.g. `../../.ssh/id_rsa`).
fn validate_template_id(id: &str) -> Result<()> {
    if id.is_empty() {
        anyhow::bail!("Template ID must not be empty");
    }
    if id.len() > 128 {
        anyhow::bail!("Template ID exceeds maximum length");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!("Template ID contains invalid characters");
    }
    Ok(())
}

/// List all saved prompt templates.
pub fn list_templates() -> Result<Vec<PromptTemplate>> {
    let dir = prompts_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();
    for entry in std::fs::read_dir(&dir).context("reading prompts dir")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match load_template_from_path(&path) {
                Ok(t) => templates.push(t),
                Err(e) => {
                    tracing::warn!("Skipping invalid prompt template {:?}: {e}", path);
                }
            }
        }
    }
    templates.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(templates)
}

/// Save a prompt template to disk.
pub fn save_template(template: &PromptTemplate) -> Result<PathBuf> {
    validate_template_id(&template.id)?;
    let dir = prompts_dir();
    std::fs::create_dir_all(&dir).context("creating prompts dir")?;

    let path = dir.join(format!("{}.json", template.id));
    let json = serde_json::to_string_pretty(template).context("serializing template")?;
    std::fs::write(&path, json).context("writing template file")?;
    Ok(path)
}

/// Load a template by ID.
pub fn load_template(id: &str) -> Result<PromptTemplate> {
    validate_template_id(id)?;
    let path = prompts_dir().join(format!("{id}.json"));
    load_template_from_path(&path)
}

/// Delete a template by ID.
pub fn delete_template(id: &str) -> Result<()> {
    validate_template_id(id)?;
    let path = prompts_dir().join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path).context("deleting template")?;
    }
    Ok(())
}

fn load_template_from_path(path: &Path) -> Result<PromptTemplate> {
    let data = std::fs::read_to_string(path).context("reading template")?;
    serde_json::from_str(&data).context("parsing template JSON")
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn uuid_v4_simple() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_template_has_id() {
        let t = PromptTemplate::new(
            "Review Code".into(),
            "Code review prompt".into(),
            "Review the following files for bugs".into(),
        );
        assert!(!t.id.is_empty());
        assert!(t.id.starts_with("review-code-"));
    }

    #[test]
    fn test_slug() {
        assert_eq!(slug("Hello World"), "hello-world");
        assert_eq!(slug("  test  "), "test");
    }

    #[test]
    fn test_roundtrip_serialize() {
        let t = PromptTemplate::new(
            "Test".into(),
            "desc".into(),
            "instruction".into(),
        );
        let json = serde_json::to_string(&t).unwrap();
        let t2: PromptTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(t.id, t2.id);
        assert_eq!(t.name, t2.name);
    }

    #[test]
    fn test_validate_template_id_accepts_valid() {
        assert!(validate_template_id("review-code-abc123").is_ok());
        assert!(validate_template_id("my_template-42").is_ok());
        assert!(validate_template_id("a").is_ok());
    }

    #[test]
    fn test_validate_template_id_rejects_traversal() {
        assert!(validate_template_id("../../etc/passwd").is_err());
        assert!(validate_template_id("..\\windows\\system32").is_err());
        assert!(validate_template_id("test/path").is_err());
        assert!(validate_template_id("test.json").is_err());
    }

    #[test]
    fn test_validate_template_id_rejects_empty_and_long() {
        assert!(validate_template_id("").is_err());
        let long_id = "a".repeat(129);
        assert!(validate_template_id(&long_id).is_err());
    }

    #[test]
    fn test_uuid_v4_simple_uniqueness() {
        let a = uuid_v4_simple();
        let b = uuid_v4_simple();
        assert_ne!(a, b, "Two consecutive UUIDs must differ");
        assert_eq!(a.len(), 32, "UUID v4 without hyphens should be 32 chars");
    }
}
