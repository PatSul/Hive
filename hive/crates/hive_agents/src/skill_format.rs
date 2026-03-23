//! Skill file format — TOML-based, model-agnostic skill definitions.
//!
//! Each skill lives as a `.toml` file in `~/.hive/skills/`. The format
//! carries capability requirements so the runtime can adapt execution
//! per-model without the skill author touching provider-specific code.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::skill_marketplace::SkillCategory;
use hive_ai::types::{ModelCapability, ModelTier};

// ---------------------------------------------------------------------------
// TOML schema types
// ---------------------------------------------------------------------------

/// A skill loaded from a `.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub skill: SkillMeta,
    #[serde(default)]
    pub requirements: SkillRequirements,
    pub prompt: SkillPrompt,
    #[serde(default)]
    pub tools: SkillTools,
    #[serde(default)]
    pub metadata: SkillMetadata,
}

/// Core identity of the skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_category")]
    pub category: SkillCategory,
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default = "default_source")]
    pub source: SkillFileSource,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Where the skill came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillFileSource {
    Builtin,
    Community,
    Custom,
}

/// Model capabilities required and preferred for this skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirements {
    /// The model MUST have all of these to run the skill.
    #[serde(default)]
    pub capabilities: Vec<ModelCapability>,
    /// Nice-to-have — the runtime will enhance the prompt if present.
    #[serde(default)]
    pub preferred: Vec<ModelCapability>,
    /// Minimum model tier (Budget, Mid, Premium). Defaults to Budget.
    #[serde(default = "default_tier")]
    pub min_tier: ModelTier,
}

impl Default for SkillRequirements {
    fn default() -> Self {
        Self {
            capabilities: Vec::new(),
            preferred: Vec::new(),
            min_tier: ModelTier::Budget,
        }
    }
}

/// The prompt template and hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPrompt {
    pub template: String,
    /// Hint appended when model supports tool use.
    #[serde(default)]
    pub tool_use_hint: Option<String>,
    /// Hint appended when model supports structured output.
    #[serde(default)]
    pub structured_output_hint: Option<String>,
}

/// Tools the skill needs the model to call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillTools {
    /// The model must have access to these tools.
    #[serde(default)]
    pub required: Vec<String>,
    /// Nice-to-have tools.
    #[serde(default)]
    pub optional: Vec<String>,
}

/// File-level metadata (auto-computed or set by author).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// SHA-256 hex digest of the prompt template. Auto-computed on save.
    #[serde(default)]
    pub integrity_hash: String,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_version() -> String {
    "1.0.0".into()
}
fn default_category() -> SkillCategory {
    SkillCategory::Custom
}
fn default_author() -> String {
    "unknown".into()
}
fn default_source() -> SkillFileSource {
    SkillFileSource::Custom
}
fn default_true() -> bool {
    true
}
fn default_tier() -> ModelTier {
    ModelTier::Budget
}

// ---------------------------------------------------------------------------
// Built-in skills (embedded at compile time)
// ---------------------------------------------------------------------------

/// All 15 built-in skill definitions, compiled from TOML files.
pub fn builtin_skills() -> Vec<SkillFile> {
    let sources: &[&str] = &[
        include_str!("builtin_skills/help.toml"),
        include_str!("builtin_skills/web-search.toml"),
        include_str!("builtin_skills/code-review.toml"),
        include_str!("builtin_skills/git-commit.toml"),
        include_str!("builtin_skills/generate-docs.toml"),
        include_str!("builtin_skills/test-gen.toml"),
        include_str!("builtin_skills/slack.toml"),
        include_str!("builtin_skills/jira.toml"),
        include_str!("builtin_skills/notion.toml"),
        include_str!("builtin_skills/db.toml"),
        include_str!("builtin_skills/docker.toml"),
        include_str!("builtin_skills/k8s.toml"),
        include_str!("builtin_skills/deploy.toml"),
        include_str!("builtin_skills/browse.toml"),
        include_str!("builtin_skills/index-docs.toml"),
    ];

    sources
        .iter()
        .filter_map(|src| {
            SkillFile::from_toml(src)
                .map(|s| s.with_computed_hash())
                .ok()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// SkillFile helpers
// ---------------------------------------------------------------------------

impl SkillFile {
    /// Compute the integrity hash for the prompt template.
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.prompt.template.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Verify the stored integrity hash matches the template.
    pub fn verify_integrity(&self) -> bool {
        if self.metadata.integrity_hash.is_empty() {
            return true; // no hash stored yet — first load
        }
        self.compute_hash() == self.metadata.integrity_hash
    }

    /// Parse a `.toml` string into a `SkillFile`.
    pub fn from_toml(content: &str) -> Result<Self> {
        let skill: Self = toml::from_str(content).context("Failed to parse skill TOML")?;
        Ok(skill)
    }

    /// Serialize to a TOML string.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize skill to TOML")
    }

    /// Create a `SkillFile` with integrity hash auto-computed.
    pub fn with_computed_hash(mut self) -> Self {
        self.metadata.integrity_hash = self.compute_hash();
        self
    }
}

// ---------------------------------------------------------------------------
// SkillLoader — scans ~/.hive/skills/ for .toml files
// ---------------------------------------------------------------------------

/// Loads and persists skill files from a directory.
pub struct SkillLoader {
    skills_dir: PathBuf,
}

impl SkillLoader {
    /// Create a loader for the given directory, creating it if absent.
    pub fn new(skills_dir: PathBuf) -> Self {
        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir).ok();
        }
        Self { skills_dir }
    }

    /// The directory being managed.
    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }

    /// Load all `.toml` skill files from the directory.
    pub fn load_all(&self) -> Result<Vec<SkillFile>> {
        let mut skills = Vec::new();
        if !self.skills_dir.exists() {
            return Ok(skills);
        }
        for entry in std::fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                match self.load_file(&path) {
                    Ok(skill) => skills.push(skill),
                    Err(e) => {
                        tracing::warn!("Skipping invalid skill file {:?}: {}", path, e);
                    }
                }
            }
        }
        skills.sort_by(|a, b| a.skill.name.cmp(&b.skill.name));
        Ok(skills)
    }

    /// Load a single skill file by path.
    pub fn load_file(&self, path: &Path) -> Result<SkillFile> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {:?}", path))?;
        let skill = SkillFile::from_toml(&content)?;
        if !skill.verify_integrity() {
            bail!(
                "Skill '{}' integrity check failed — file may have been tampered",
                skill.skill.name
            );
        }
        Ok(skill)
    }

    /// Load a skill by name (looks for `{name}.toml`).
    pub fn load_by_name(&self, name: &str) -> Result<Option<SkillFile>> {
        let path = self.skill_path(name);
        if !path.exists() {
            return Ok(None);
        }
        self.load_file(&path).map(Some)
    }

    /// Save a skill to disk. Auto-computes integrity hash.
    pub fn save(&self, skill: &SkillFile) -> Result<()> {
        let mut skill = skill.clone();
        skill.metadata.integrity_hash = skill.compute_hash();
        let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
        if skill.metadata.created.is_none() {
            skill.metadata.created = Some(now.clone());
        }
        skill.metadata.updated = Some(now);

        let content = skill.to_toml()?;
        let path = self.skill_path(&skill.skill.name);
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write skill file: {:?}", path))?;
        Ok(())
    }

    /// Delete a skill file by name.
    pub fn delete(&self, name: &str) -> Result<bool> {
        let path = self.skill_path(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Toggle a skill's enabled state on disk.
    pub fn toggle(&self, name: &str) -> Result<Option<bool>> {
        let path = self.skill_path(name);
        if !path.exists() {
            return Ok(None);
        }
        let mut skill = self.load_file(&path)?;
        skill.skill.enabled = !skill.skill.enabled;
        self.save(&skill)?;
        Ok(Some(skill.skill.enabled))
    }

    /// Ensure built-in skills exist on disk. Writes any missing ones.
    pub fn ensure_builtins(&self, builtins: &[SkillFile]) -> Result<()> {
        for builtin in builtins {
            let path = self.skill_path(&builtin.skill.name);
            if !path.exists() {
                self.save(builtin)?;
            }
        }
        Ok(())
    }

    fn skill_path(&self, name: &str) -> PathBuf {
        self.skills_dir.join(format!("{}.toml", name))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_skill() -> SkillFile {
        SkillFile {
            skill: SkillMeta {
                name: "test-skill".into(),
                description: "A test skill".into(),
                version: "1.0.0".into(),
                category: SkillCategory::Testing,
                author: "test".into(),
                source: SkillFileSource::Custom,
                enabled: true,
            },
            requirements: SkillRequirements {
                capabilities: vec![ModelCapability::ToolUse],
                preferred: vec![ModelCapability::ExtendedThinking],
                min_tier: ModelTier::Budget,
            },
            prompt: SkillPrompt {
                template: "Do something useful with the code.".into(),
                tool_use_hint: Some("Use read_file to examine code.".into()),
                structured_output_hint: None,
            },
            tools: SkillTools {
                required: vec!["read_file".into()],
                optional: vec![],
            },
            metadata: SkillMetadata::default(),
        }
    }

    #[test]
    fn parse_toml_roundtrip() {
        let skill = sample_skill().with_computed_hash();
        let toml_str = skill.to_toml().unwrap();
        let parsed = SkillFile::from_toml(&toml_str).unwrap();
        assert_eq!(parsed.skill.name, "test-skill");
        assert_eq!(
            parsed.requirements.capabilities,
            vec![ModelCapability::ToolUse]
        );
        assert!(parsed.verify_integrity());
    }

    #[test]
    fn integrity_hash_detects_tampering() {
        let skill = sample_skill().with_computed_hash();
        let toml_str = skill.to_toml().unwrap();
        // Tamper with the template
        let tampered = toml_str.replace("Do something useful", "EVIL INJECTION");
        let parsed = SkillFile::from_toml(&tampered).unwrap();
        assert!(!parsed.verify_integrity());
    }

    #[test]
    fn loader_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let loader = SkillLoader::new(tmp.path().to_path_buf());

        let skill = sample_skill();
        loader.save(&skill).unwrap();

        let loaded = loader.load_by_name("test-skill").unwrap().unwrap();
        assert_eq!(loaded.skill.name, "test-skill");
        assert_eq!(loaded.skill.description, "A test skill");
        assert!(loaded.verify_integrity());
    }

    #[test]
    fn loader_load_all() {
        let tmp = TempDir::new().unwrap();
        let loader = SkillLoader::new(tmp.path().to_path_buf());

        let mut s1 = sample_skill();
        s1.skill.name = "alpha".into();
        loader.save(&s1).unwrap();

        let mut s2 = sample_skill();
        s2.skill.name = "beta".into();
        loader.save(&s2).unwrap();

        let all = loader.load_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].skill.name, "alpha");
        assert_eq!(all[1].skill.name, "beta");
    }

    #[test]
    fn loader_delete() {
        let tmp = TempDir::new().unwrap();
        let loader = SkillLoader::new(tmp.path().to_path_buf());

        loader.save(&sample_skill()).unwrap();
        assert!(loader.delete("test-skill").unwrap());
        assert!(!loader.delete("test-skill").unwrap());
        assert!(loader.load_by_name("test-skill").unwrap().is_none());
    }

    #[test]
    fn loader_toggle() {
        let tmp = TempDir::new().unwrap();
        let loader = SkillLoader::new(tmp.path().to_path_buf());

        loader.save(&sample_skill()).unwrap();
        assert_eq!(loader.toggle("test-skill").unwrap(), Some(false));
        assert_eq!(loader.toggle("test-skill").unwrap(), Some(true));
        assert_eq!(loader.toggle("nonexistent").unwrap(), None);
    }

    #[test]
    fn ensure_builtins_only_writes_missing() {
        let tmp = TempDir::new().unwrap();
        let loader = SkillLoader::new(tmp.path().to_path_buf());

        let builtin = sample_skill();
        loader.ensure_builtins(&[builtin.clone()]).unwrap();

        // Modify the on-disk copy
        let mut modified = loader.load_by_name("test-skill").unwrap().unwrap();
        modified.skill.description = "Modified by user".into();
        loader.save(&modified).unwrap();

        // ensure_builtins should NOT overwrite
        loader.ensure_builtins(&[builtin]).unwrap();
        let loaded = loader.load_by_name("test-skill").unwrap().unwrap();
        assert_eq!(loaded.skill.description, "Modified by user");
    }

    #[test]
    fn empty_hash_passes_integrity() {
        let skill = sample_skill(); // no hash computed
        assert!(skill.verify_integrity());
    }

    #[test]
    fn builtin_skills_load_all_15() {
        let builtins = super::builtin_skills();
        assert_eq!(builtins.len(), 15, "Should load all 15 built-in skills");

        let names: Vec<&str> = builtins.iter().map(|s| s.skill.name.as_str()).collect();
        assert!(names.contains(&"help"));
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"slack"));
        assert!(names.contains(&"k8s"));
        assert!(names.contains(&"index-docs"));
    }

    #[test]
    fn builtin_skills_have_valid_hashes() {
        for skill in super::builtin_skills() {
            assert!(
                skill.verify_integrity(),
                "Built-in skill '{}' should have valid integrity hash",
                skill.skill.name
            );
        }
    }

    #[test]
    fn builtin_skills_roundtrip_through_loader() {
        let tmp = TempDir::new().unwrap();
        let loader = SkillLoader::new(tmp.path().to_path_buf());
        let builtins = super::builtin_skills();

        loader.ensure_builtins(&builtins).unwrap();
        let loaded = loader.load_all().unwrap();
        assert_eq!(loaded.len(), 15);
    }
}
