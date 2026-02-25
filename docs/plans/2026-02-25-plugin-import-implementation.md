# Plugin Import System — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable importing skills from GitHub repos, URLs, and local files into Hive's Skills panel.

**Architecture:** New `PluginManager` service in `hive_agents` handles fetching/parsing external plugins. `SkillMarketplace` extended with plugin storage and persistence. Skills panel UI gains an Import button with dropdown, preview screen, and grouped installed view.

**Tech Stack:** Rust, GPUI, reqwest 0.12, serde_json, sha2, chrono, uuid — all already workspace dependencies.

---

## Task 1: Plugin Types Module

**Files:**
- Create: `hive/crates/hive_agents/src/plugin_types.rs`
- Modify: `hive/crates/hive_agents/src/lib.rs:1-71`

**Step 1: Create `plugin_types.rs` with all data types**

```rust
//! Plugin types — data model for imported plugin packages.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::skill_marketplace::SecurityIssue;

// ---------------------------------------------------------------------------
// Manifest types (parsed from plugin.json)
// ---------------------------------------------------------------------------

/// A plugin manifest parsed from `plugin.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: PluginAuthor,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Relative path to skills directory (e.g. "./skills/").
    #[serde(rename = "skills")]
    pub skills_path: Option<String>,
    /// Relative path to commands directory (e.g. "./commands/").
    #[serde(rename = "commands")]
    pub commands_path: Option<String>,
    /// Relative path to agents directory (e.g. "./agents/").
    #[serde(rename = "agents")]
    pub agents_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    pub email: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsed skill/command (pre-install)
// ---------------------------------------------------------------------------

/// A skill parsed from a SKILL.md file (YAML frontmatter + markdown body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSkill {
    pub name: String,
    pub description: String,
    /// Full markdown body (everything after frontmatter).
    pub instructions: String,
    /// Relative path within plugin (e.g. "skills/brainstorming/SKILL.md").
    pub source_file: String,
}

/// A command parsed from a commands/*.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCommand {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

// ---------------------------------------------------------------------------
// Plugin source
// ---------------------------------------------------------------------------

/// Where a plugin was imported from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginSource {
    GitHub {
        owner: String,
        repo: String,
        branch: Option<String>,
    },
    Url(String),
    Local {
        path: String,
    },
}

// ---------------------------------------------------------------------------
// Installed plugin (persisted)
// ---------------------------------------------------------------------------

/// A command installed as part of a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledCommand {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

/// A skill within an installed plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
    pub enabled: bool,
    pub integrity_hash: String,
}

/// An installed plugin group (persisted to ~/.hive/plugins.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: PluginAuthor,
    pub description: String,
    pub source: PluginSource,
    pub installed_at: DateTime<Utc>,
    pub skills: Vec<PluginSkill>,
    pub commands: Vec<InstalledCommand>,
}

// ---------------------------------------------------------------------------
// Plugin preview (pre-install)
// ---------------------------------------------------------------------------

/// Result of fetching a plugin before installation.
pub struct PluginPreview {
    pub manifest: PluginManifest,
    pub skills: Vec<ParsedSkill>,
    pub commands: Vec<ParsedCommand>,
    pub security_warnings: Vec<SecurityIssue>,
}

// ---------------------------------------------------------------------------
// Version check
// ---------------------------------------------------------------------------

/// Result of checking a plugin for updates.
#[derive(Debug, Clone)]
pub struct UpdateAvailable {
    pub plugin_id: String,
    pub plugin_name: String,
    pub current_version: String,
    pub latest_version: String,
    pub source: PluginSource,
}

// ---------------------------------------------------------------------------
// Plugin store (persisted JSON)
// ---------------------------------------------------------------------------

/// Top-level structure for ~/.hive/plugins.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginStore {
    pub plugins: Vec<InstalledPlugin>,
}

/// Version check cache (~/.hive/plugin_cache.json).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCache {
    pub last_checked: Option<DateTime<Utc>>,
    pub versions: std::collections::HashMap<String, CachedVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedVersion {
    pub latest_version: String,
    pub checked_at: DateTime<Utc>,
}
```

**Step 2: Add module to `lib.rs`**

In `hive/crates/hive_agents/src/lib.rs`, add after line 19 (`pub mod skill_marketplace;`):

```rust
pub mod plugin_types;
```

And add re-exports after the `skill_marketplace` re-exports (after line 58):

```rust
pub use plugin_types::{
    InstalledCommand, InstalledPlugin, ParsedCommand, ParsedSkill, PluginAuthor,
    PluginCache, PluginManifest, PluginPreview, PluginSkill, PluginSource,
    PluginStore, UpdateAvailable,
};
```

**Step 3: Run `cargo check -p hive_agents`**

Expected: compiles with no errors.

**Step 4: Commit**

```bash
git add hive/crates/hive_agents/src/plugin_types.rs hive/crates/hive_agents/src/lib.rs
git commit -m "feat(agents): add plugin_types module with data model for plugin imports"
```

---

## Task 2: PluginManager — Markdown Parsing

**Files:**
- Create: `hive/crates/hive_agents/src/plugin_manager.rs`
- Modify: `hive/crates/hive_agents/src/lib.rs`

**Step 1: Create `plugin_manager.rs` with frontmatter parser and manifest parser**

```rust
//! Plugin Manager — fetch, parse, and version-check external plugin packages.

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::plugin_types::*;
use crate::skill_marketplace::SkillMarketplace;

/// Manages fetching, parsing, and version-checking of external plugins.
pub struct PluginManager {
    client: reqwest::Client,
}

impl PluginManager {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    // -- Markdown frontmatter parsing ----------------------------------------

    /// Parse YAML frontmatter from a markdown file.
    ///
    /// Expects the format:
    /// ```
    /// ---
    /// name: foo
    /// description: "bar"
    /// ---
    /// # Body content here
    /// ```
    ///
    /// Returns (frontmatter_yaml, body_markdown).
    pub fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
        let content = content.trim_start();
        if !content.starts_with("---") {
            return None;
        }
        let after_first = &content[3..];
        let end = after_first.find("\n---")?;
        let frontmatter = after_first[..end].trim();
        let body_start = 3 + end + 4; // "---" + "\n---"
        let body = if body_start < content.len() {
            content[body_start..].trim()
        } else {
            ""
        };
        Some((frontmatter, body))
    }

    /// Parse a SKILL.md file into a ParsedSkill.
    pub fn parse_skill_md(content: &str, source_file: &str) -> Result<ParsedSkill> {
        let (frontmatter, body) = Self::split_frontmatter(content)
            .ok_or_else(|| anyhow::anyhow!("No YAML frontmatter found in {source_file}"))?;

        // Extract name and description from frontmatter lines.
        let mut name = String::new();
        let mut description = String::new();
        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("name:") {
                name = val.trim().trim_matches('"').to_string();
            } else if let Some(val) = line.strip_prefix("description:") {
                description = val.trim().trim_matches('"').to_string();
            }
        }

        if name.is_empty() {
            // Fall back to filename.
            name = std::path::Path::new(source_file)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
        }

        Ok(ParsedSkill {
            name,
            description,
            instructions: body.to_string(),
            source_file: source_file.to_string(),
        })
    }

    /// Parse a command .md file into a ParsedCommand.
    pub fn parse_command_md(content: &str, source_file: &str) -> Result<ParsedCommand> {
        let (frontmatter, body) = Self::split_frontmatter(content)
            .ok_or_else(|| anyhow::anyhow!("No YAML frontmatter found in {source_file}"))?;

        let mut description = String::new();
        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("description:") {
                description = val.trim().trim_matches('"').to_string();
            }
        }

        let name = std::path::Path::new(source_file)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ParsedCommand {
            name,
            description,
            instructions: body.to_string(),
            source_file: source_file.to_string(),
        })
    }

    /// Parse a plugin.json manifest.
    pub fn parse_manifest(json: &str) -> Result<PluginManifest> {
        serde_json::from_str(json).context("Failed to parse plugin.json manifest")
    }

    /// Compute SHA-256 integrity hash for skill instructions.
    pub fn integrity_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Run security scan on all skills and commands, collecting warnings.
    pub fn scan_plugin(skills: &[ParsedSkill], commands: &[ParsedCommand]) -> Vec<crate::skill_marketplace::SecurityIssue> {
        let mut all_warnings = Vec::new();
        for skill in skills {
            let issues = SkillMarketplace::scan_for_injection(&skill.instructions);
            all_warnings.extend(issues);
        }
        for cmd in commands {
            let issues = SkillMarketplace::scan_for_injection(&cmd.instructions);
            all_warnings.extend(issues);
        }
        all_warnings
    }
}
```

**Step 2: Add module to `lib.rs`**

Add `pub mod plugin_manager;` after the `plugin_types` line.

Add re-export:

```rust
pub use plugin_manager::PluginManager;
```

**Step 3: Run `cargo check -p hive_agents`**

Expected: compiles with no errors.

**Step 4: Write tests for frontmatter parser**

Add to the bottom of `plugin_manager.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_valid() {
        let content = "---\nname: foo\ndescription: \"bar\"\n---\n# Body\nContent here.";
        let (fm, body) = PluginManager::split_frontmatter(content).unwrap();
        assert!(fm.contains("name: foo"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn split_frontmatter_no_frontmatter() {
        let content = "# Just a regular markdown file";
        assert!(PluginManager::split_frontmatter(content).is_none());
    }

    #[test]
    fn parse_skill_md_basic() {
        let content = "---\nname: brainstorming\ndescription: \"Design ideas\"\n---\n# Brainstorming\n\nExplore ideas.";
        let skill = PluginManager::parse_skill_md(content, "skills/brainstorming/SKILL.md").unwrap();
        assert_eq!(skill.name, "brainstorming");
        assert_eq!(skill.description, "Design ideas");
        assert!(skill.instructions.contains("Explore ideas."));
    }

    #[test]
    fn parse_skill_md_name_from_path() {
        let content = "---\ndescription: \"No name field\"\n---\nBody.";
        let skill = PluginManager::parse_skill_md(content, "skills/debugging/SKILL.md").unwrap();
        assert_eq!(skill.name, "debugging");
    }

    #[test]
    fn parse_command_md_basic() {
        let content = "---\ndescription: \"Run brainstorm\"\n---\nInvoke the brainstorming skill.";
        let cmd = PluginManager::parse_command_md(content, "commands/brainstorm.md").unwrap();
        assert_eq!(cmd.name, "brainstorm");
        assert!(cmd.instructions.contains("Invoke"));
    }

    #[test]
    fn parse_manifest_valid() {
        let json = r#"{"name":"superpowers","description":"Core skills","version":"4.3.1","author":{"name":"Jesse"}}"#;
        let manifest = PluginManager::parse_manifest(json).unwrap();
        assert_eq!(manifest.name, "superpowers");
        assert_eq!(manifest.version, "4.3.1");
    }

    #[test]
    fn integrity_hash_deterministic() {
        let h1 = PluginManager::integrity_hash("hello");
        let h2 = PluginManager::integrity_hash("hello");
        assert_eq!(h1, h2);
        assert_ne!(h1, PluginManager::integrity_hash("world"));
    }

    #[test]
    fn scan_plugin_clean() {
        let skills = vec![ParsedSkill {
            name: "test".into(),
            description: "test".into(),
            instructions: "Generate unit tests for the function.".into(),
            source_file: "skills/test/SKILL.md".into(),
        }];
        let warnings = PluginManager::scan_plugin(&skills, &[]);
        assert!(warnings.is_empty());
    }
}
```

**Step 5: Run `cargo test -p hive_agents -- plugin`**

Expected: all tests pass.

**Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/plugin_manager.rs hive/crates/hive_agents/src/lib.rs
git commit -m "feat(agents): add PluginManager with markdown/manifest parsing and tests"
```

---

## Task 3: PluginManager — GitHub Fetching

**Files:**
- Modify: `hive/crates/hive_agents/src/plugin_manager.rs`

**Step 1: Add GitHub fetching methods to PluginManager**

Add these methods to the `impl PluginManager` block:

```rust
    // -- GitHub fetching -----------------------------------------------------

    /// Fetch a plugin from a GitHub repository.
    ///
    /// Detects plugin.json or .claude-plugin/plugin.json for full plugins,
    /// or wraps a single .md file as a synthetic plugin.
    pub async fn fetch_from_github(&self, owner: &str, repo: &str) -> Result<PluginPreview> {
        debug!(owner, repo, "Fetching plugin from GitHub");

        // 1. Get default branch.
        let repo_info: serde_json::Value = self.client
            .get(format!("https://api.github.com/repos/{owner}/{repo}"))
            .header("User-Agent", "Hive")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to reach GitHub API")?
            .json()
            .await
            .context("Failed to parse GitHub repo info")?;

        let branch = repo_info["default_branch"]
            .as_str()
            .unwrap_or("main")
            .to_string();

        // 2. Get file tree.
        let tree: serde_json::Value = self.client
            .get(format!(
                "https://api.github.com/repos/{owner}/{repo}/git/trees/{branch}?recursive=1"
            ))
            .header("User-Agent", "Hive")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to fetch repo tree")?
            .json()
            .await
            .context("Failed to parse repo tree")?;

        let files: Vec<String> = tree["tree"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|entry| entry["path"].as_str().map(|s| s.to_string()))
            .collect();

        // 3. Find plugin.json (try .claude-plugin/plugin.json first, then root).
        let manifest_path = if files.iter().any(|f| f == ".claude-plugin/plugin.json") {
            Some(".claude-plugin/plugin.json".to_string())
        } else if files.iter().any(|f| f == "plugin.json") {
            Some("plugin.json".to_string())
        } else {
            None
        };

        if let Some(manifest_path) = manifest_path {
            self.fetch_full_plugin_from_github(owner, repo, &branch, &manifest_path, &files)
                .await
        } else {
            // Look for a single SKILL.md at root.
            let skill_files: Vec<&String> = files.iter().filter(|f| f.ends_with("SKILL.md")).collect();
            if skill_files.is_empty() {
                bail!("No plugin.json or SKILL.md found in {owner}/{repo}");
            }
            self.fetch_single_skill_from_github(owner, repo, &branch, skill_files[0]).await
        }
    }

    /// Fetch a full plugin from GitHub (has plugin.json manifest).
    async fn fetch_full_plugin_from_github(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        manifest_path: &str,
        files: &[String],
    ) -> Result<PluginPreview> {
        // Fetch and parse manifest.
        let manifest_content = self.fetch_github_file(owner, repo, branch, manifest_path).await?;
        let manifest = Self::parse_manifest(&manifest_content)?;

        // Determine skill and command paths.
        let skills_dir = manifest.skills_path.as_deref().unwrap_or("./skills/");
        let commands_dir = manifest.commands_path.as_deref().unwrap_or("./commands/");

        // Normalize paths (remove leading ./).
        let skills_prefix = skills_dir.trim_start_matches("./");
        let commands_prefix = commands_dir.trim_start_matches("./");

        // Find SKILL.md files.
        let skill_files: Vec<&String> = files
            .iter()
            .filter(|f| f.starts_with(skills_prefix) && f.ends_with("SKILL.md"))
            .collect();

        // Find command .md files.
        let command_files: Vec<&String> = files
            .iter()
            .filter(|f| {
                f.starts_with(commands_prefix)
                    && f.ends_with(".md")
                    && !f.contains("SKILL.md")
            })
            .collect();

        // Fetch and parse all skills.
        let mut skills = Vec::new();
        for file in &skill_files {
            match self.fetch_github_file(owner, repo, branch, file).await {
                Ok(content) => match Self::parse_skill_md(&content, file) {
                    Ok(skill) => skills.push(skill),
                    Err(e) => warn!(file, "Failed to parse skill: {e}"),
                },
                Err(e) => warn!(file, "Failed to fetch skill file: {e}"),
            }
        }

        // Fetch and parse all commands.
        let mut commands = Vec::new();
        for file in &command_files {
            match self.fetch_github_file(owner, repo, branch, file).await {
                Ok(content) => match Self::parse_command_md(&content, file) {
                    Ok(cmd) => commands.push(cmd),
                    Err(e) => warn!(file, "Failed to parse command: {e}"),
                },
                Err(e) => warn!(file, "Failed to fetch command file: {e}"),
            }
        }

        let security_warnings = Self::scan_plugin(&skills, &commands);

        Ok(PluginPreview {
            manifest,
            skills,
            commands,
            security_warnings,
        })
    }

    /// Wrap a single SKILL.md as a synthetic single-skill plugin.
    async fn fetch_single_skill_from_github(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        skill_path: &str,
    ) -> Result<PluginPreview> {
        let content = self.fetch_github_file(owner, repo, branch, skill_path).await?;
        let skill = Self::parse_skill_md(&content, skill_path)?;

        let manifest = PluginManifest {
            name: skill.name.clone(),
            description: skill.description.clone(),
            version: "1.0.0".into(),
            author: PluginAuthor::default(),
            homepage: Some(format!("https://github.com/{owner}/{repo}")),
            repository: Some(format!("https://github.com/{owner}/{repo}")),
            license: None,
            keywords: vec![],
            skills_path: None,
            commands_path: None,
            agents_path: None,
        };

        let security_warnings = Self::scan_plugin(&[skill.clone()], &[]);

        Ok(PluginPreview {
            manifest,
            skills: vec![skill],
            commands: vec![],
            security_warnings,
        })
    }

    /// Fetch a single file from GitHub (base64-decoded).
    async fn fetch_github_file(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
    ) -> Result<String> {
        let resp: serde_json::Value = self.client
            .get(format!(
                "https://api.github.com/repos/{owner}/{repo}/contents/{path}?ref={branch}"
            ))
            .header("User-Agent", "Hive")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context(format!("Failed to fetch {path}"))?
            .json()
            .await
            .context(format!("Failed to parse response for {path}"))?;

        let encoded = resp["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No content field in response for {path}"))?;

        // GitHub returns base64 with newlines.
        let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = base64_decode(&cleaned)
            .context(format!("Failed to decode base64 for {path}"))?;
        String::from_utf8(bytes).context(format!("File {path} is not valid UTF-8"))
    }
}

/// Decode base64 (standard alphabet with padding).
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    use std::collections::HashMap;

    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let lookup: HashMap<u8, u8> = alphabet.iter().enumerate().map(|(i, &c)| (c, i as u8)).collect();

    let input = input.trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in input.as_bytes() {
        let val = *lookup.get(&byte).ok_or_else(|| anyhow::anyhow!("Invalid base64 char"))?;
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}
```

Note: We implement a simple base64 decoder to avoid adding a new dependency. If `base64` crate is already available in workspace, use that instead. Check `Cargo.lock` first.

**Step 2: Run `cargo check -p hive_agents`**

Expected: compiles.

**Step 3: Commit**

```bash
git add hive/crates/hive_agents/src/plugin_manager.rs
git commit -m "feat(agents): add GitHub fetching to PluginManager"
```

---

## Task 4: PluginManager — URL and Local Fetching

**Files:**
- Modify: `hive/crates/hive_agents/src/plugin_manager.rs`

**Step 1: Add URL fetching**

Add to `impl PluginManager`:

```rust
    // -- URL fetching --------------------------------------------------------

    /// Fetch a plugin from a URL.
    ///
    /// If URL points to a .md file, treat as single skill.
    /// If URL points to a plugin.json, fetch the manifest and discover skills
    /// relative to the URL base.
    pub async fn fetch_from_url(&self, url: &str) -> Result<PluginPreview> {
        debug!(url, "Fetching plugin from URL");

        let content = self.client
            .get(url)
            .header("User-Agent", "Hive")
            .send()
            .await
            .context("Failed to fetch URL")?
            .text()
            .await
            .context("Failed to read response body")?;

        if url.ends_with(".md") {
            // Single skill file.
            let filename = url.rsplit('/').next().unwrap_or("skill.md");
            let skill = Self::parse_skill_md(&content, filename)?;
            let manifest = PluginManifest {
                name: skill.name.clone(),
                description: skill.description.clone(),
                version: "1.0.0".into(),
                author: PluginAuthor::default(),
                homepage: None,
                repository: None,
                license: None,
                keywords: vec![],
                skills_path: None,
                commands_path: None,
                agents_path: None,
            };
            let security_warnings = Self::scan_plugin(&[skill.clone()], &[]);
            Ok(PluginPreview {
                manifest,
                skills: vec![skill],
                commands: vec![],
                security_warnings,
            })
        } else {
            // Assume plugin.json manifest.
            let manifest = Self::parse_manifest(&content)?;
            // For URL-based plugins, we can only install what the manifest describes.
            // Skills need to be fetched from relative URLs.
            let security_warnings = vec![];
            Ok(PluginPreview {
                manifest,
                skills: vec![],
                commands: vec![],
                security_warnings,
            })
        }
    }
```

**Step 2: Add local file/directory loading**

```rust
    // -- Local loading -------------------------------------------------------

    /// Load a plugin from a local path.
    ///
    /// If path is a .md file, treat as single skill.
    /// If path is a directory, look for plugin.json and enumerate skills/.
    pub fn load_from_local(path: &std::path::Path) -> Result<PluginPreview> {
        debug!(?path, "Loading plugin from local path");

        if path.is_file() {
            let content = std::fs::read_to_string(path)
                .context(format!("Failed to read {}", path.display()))?;
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("skill.md");

            if filename.ends_with(".md") {
                let skill = Self::parse_skill_md(&content, filename)?;
                let manifest = PluginManifest {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: "1.0.0".into(),
                    author: PluginAuthor::default(),
                    homepage: None,
                    repository: None,
                    license: None,
                    keywords: vec![],
                    skills_path: None,
                    commands_path: None,
                    agents_path: None,
                };
                let security_warnings = Self::scan_plugin(&[skill.clone()], &[]);
                return Ok(PluginPreview {
                    manifest,
                    skills: vec![skill],
                    commands: vec![],
                    security_warnings,
                });
            } else if filename == "plugin.json" {
                // Treat parent directory as plugin root.
                let parent = path.parent().ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
                return Self::load_plugin_directory(parent, &content);
            }
            bail!("Unsupported file type: {filename}");
        }

        if path.is_dir() {
            // Look for plugin.json or .claude-plugin/plugin.json.
            let manifest_path = if path.join(".claude-plugin/plugin.json").exists() {
                path.join(".claude-plugin/plugin.json")
            } else if path.join("plugin.json").exists() {
                path.join("plugin.json")
            } else {
                bail!("No plugin.json found in {}", path.display());
            };

            let content = std::fs::read_to_string(&manifest_path)
                .context("Failed to read plugin.json")?;
            return Self::load_plugin_directory(path, &content);
        }

        bail!("Path does not exist: {}", path.display());
    }

    /// Load a plugin from a directory with a known plugin.json content.
    fn load_plugin_directory(root: &std::path::Path, manifest_json: &str) -> Result<PluginPreview> {
        let manifest = Self::parse_manifest(manifest_json)?;

        let skills_dir = manifest.skills_path.as_deref().unwrap_or("./skills/");
        let commands_dir = manifest.commands_path.as_deref().unwrap_or("./commands/");

        let skills_path = root.join(skills_dir.trim_start_matches("./"));
        let commands_path = root.join(commands_dir.trim_start_matches("./"));

        let mut skills = Vec::new();
        if skills_path.is_dir() {
            for entry in std::fs::read_dir(&skills_path).into_iter().flatten() {
                if let Ok(entry) = entry {
                    let skill_md = entry.path().join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(content) = std::fs::read_to_string(&skill_md) {
                            let rel = format!(
                                "{}/{}",
                                skills_dir.trim_start_matches("./").trim_end_matches('/'),
                                entry.file_name().to_string_lossy()
                            );
                            match Self::parse_skill_md(&content, &format!("{rel}/SKILL.md")) {
                                Ok(skill) => skills.push(skill),
                                Err(e) => warn!("Failed to parse {}: {e}", skill_md.display()),
                            }
                        }
                    }
                }
            }
        }

        let mut commands = Vec::new();
        if commands_path.is_dir() {
            for entry in std::fs::read_dir(&commands_path).into_iter().flatten() {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let rel = format!(
                                "{}/{}",
                                commands_dir.trim_start_matches("./").trim_end_matches('/'),
                                entry.file_name().to_string_lossy()
                            );
                            match Self::parse_command_md(&content, &rel) {
                                Ok(cmd) => commands.push(cmd),
                                Err(e) => warn!("Failed to parse {}: {e}", path.display()),
                            }
                        }
                    }
                }
            }
        }

        let security_warnings = Self::scan_plugin(&skills, &commands);

        Ok(PluginPreview {
            manifest,
            skills,
            commands,
            security_warnings,
        })
    }
```

**Step 2: Run `cargo check -p hive_agents`**

Expected: compiles.

**Step 3: Commit**

```bash
git add hive/crates/hive_agents/src/plugin_manager.rs
git commit -m "feat(agents): add URL and local file fetching to PluginManager"
```

---

## Task 5: PluginManager — Version Checking

**Files:**
- Modify: `hive/crates/hive_agents/src/plugin_manager.rs`

**Step 1: Add version checking**

```rust
    // -- Version checking ----------------------------------------------------

    /// Check installed plugins for available updates.
    ///
    /// Only checks GitHub-sourced plugins. Respects cache throttling.
    pub async fn check_for_updates(
        &self,
        plugins: &[InstalledPlugin],
        cache: &mut PluginCache,
    ) -> Vec<UpdateAvailable> {
        let now = chrono::Utc::now();

        // Throttle: at most once per hour.
        if let Some(last) = cache.last_checked {
            let elapsed = now.signed_duration_since(last);
            if elapsed.num_minutes() < 60 {
                debug!("Skipping version check (last checked {}m ago)", elapsed.num_minutes());
                // Return cached results.
                return plugins
                    .iter()
                    .filter_map(|p| {
                        let cached = cache.versions.get(&p.id)?;
                        if cached.latest_version != p.version {
                            Some(UpdateAvailable {
                                plugin_id: p.id.clone(),
                                plugin_name: p.name.clone(),
                                current_version: p.version.clone(),
                                latest_version: cached.latest_version.clone(),
                                source: p.source.clone(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }

        let mut updates = Vec::new();
        cache.last_checked = Some(now);

        for plugin in plugins {
            match &plugin.source {
                PluginSource::GitHub { owner, repo, .. } => {
                    // Fetch just the plugin.json to check version.
                    let manifest_url = format!(
                        "https://api.github.com/repos/{owner}/{repo}/contents/.claude-plugin/plugin.json"
                    );
                    match self.client
                        .get(&manifest_url)
                        .header("User-Agent", "Hive")
                        .header("Accept", "application/vnd.github.v3+json")
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                if let Some(encoded) = json["content"].as_str() {
                                    let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
                                    if let Ok(bytes) = base64_decode(&cleaned) {
                                        if let Ok(content) = String::from_utf8(bytes) {
                                            if let Ok(manifest) = Self::parse_manifest(&content) {
                                                cache.versions.insert(plugin.id.clone(), CachedVersion {
                                                    latest_version: manifest.version.clone(),
                                                    checked_at: now,
                                                });
                                                if manifest.version != plugin.version {
                                                    updates.push(UpdateAvailable {
                                                        plugin_id: plugin.id.clone(),
                                                        plugin_name: plugin.name.clone(),
                                                        current_version: plugin.version.clone(),
                                                        latest_version: manifest.version,
                                                        source: plugin.source.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => warn!(plugin.name, "Version check failed: {e}"),
                    }
                }
                _ => {} // URL and Local plugins don't support auto-check yet.
            }
        }

        updates
    }
```

**Step 2: Run `cargo check -p hive_agents`**

Expected: compiles.

**Step 3: Commit**

```bash
git add hive/crates/hive_agents/src/plugin_manager.rs
git commit -m "feat(agents): add version checking to PluginManager"
```

---

## Task 6: SkillMarketplace — Plugin Persistence

**Files:**
- Modify: `hive/crates/hive_agents/src/skill_marketplace.rs:195-209`

**Step 1: Add `installed_plugins` field and persistence methods**

Add field to `SkillMarketplace` struct (line 195-199):

```rust
pub struct SkillMarketplace {
    installed_skills: Vec<InstalledSkill>,
    skill_sources: Vec<SkillSource>,
    trusted_domains: Vec<String>,
    installed_plugins: Vec<crate::plugin_types::InstalledPlugin>,
}
```

Update `new()` (line 203-208):

```rust
    pub fn new() -> Self {
        Self {
            installed_skills: Vec::new(),
            skill_sources: Vec::new(),
            trusted_domains: Vec::new(),
            installed_plugins: Vec::new(),
        }
    }
```

Add plugin management methods before the `// -- directory` section (before line 445):

```rust
    // -- plugin management --------------------------------------------------

    /// Return all installed plugins.
    pub fn installed_plugins(&self) -> &[crate::plugin_types::InstalledPlugin] {
        &self.installed_plugins
    }

    /// Install a plugin from a preview (user has confirmed).
    pub fn install_plugin(
        &mut self,
        preview: &crate::plugin_types::PluginPreview,
        source: crate::plugin_types::PluginSource,
        selected_skills: &[usize],
        selected_commands: &[usize],
    ) -> crate::plugin_types::InstalledPlugin {
        use crate::plugin_types::*;

        let plugin = InstalledPlugin {
            id: Uuid::new_v4().to_string(),
            name: preview.manifest.name.clone(),
            version: preview.manifest.version.clone(),
            author: preview.manifest.author.clone(),
            description: preview.manifest.description.clone(),
            source,
            installed_at: Utc::now(),
            skills: selected_skills
                .iter()
                .filter_map(|&i| preview.skills.get(i))
                .map(|s| PluginSkill {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    instructions: s.instructions.clone(),
                    source_file: s.source_file.clone(),
                    enabled: true,
                    integrity_hash: Self::compute_integrity_hash(&s.instructions),
                })
                .collect(),
            commands: selected_commands
                .iter()
                .filter_map(|&i| preview.commands.get(i))
                .map(|c| InstalledCommand {
                    name: c.name.clone(),
                    description: c.description.clone(),
                    instructions: c.instructions.clone(),
                    source_file: c.source_file.clone(),
                })
                .collect(),
        };

        debug!(name = plugin.name, "Installed plugin with {} skills", plugin.skills.len());
        self.installed_plugins.push(plugin.clone());
        plugin
    }

    /// Remove an installed plugin by ID.
    pub fn remove_plugin(&mut self, plugin_id: &str) -> Result<()> {
        let before = self.installed_plugins.len();
        self.installed_plugins.retain(|p| p.id != plugin_id);
        if self.installed_plugins.len() == before {
            bail!("Plugin with id '{}' not found", plugin_id);
        }
        debug!(plugin_id, "Removed plugin");
        Ok(())
    }

    /// Toggle a skill within an installed plugin.
    pub fn toggle_plugin_skill(&mut self, plugin_id: &str, skill_name: &str) -> Result<bool> {
        let plugin = self.installed_plugins
            .iter_mut()
            .find(|p| p.id == plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;

        let skill = plugin.skills
            .iter_mut()
            .find(|s| s.name == skill_name)
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' not found in plugin", skill_name))?;

        skill.enabled = !skill.enabled;
        Ok(skill.enabled)
    }

    /// Load plugins from a JSON file.
    pub fn load_plugins_from_file(&mut self, path: &std::path::Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(path)
            .context("Failed to read plugins.json")?;
        let store: crate::plugin_types::PluginStore = serde_json::from_str(&content)
            .context("Failed to parse plugins.json")?;
        self.installed_plugins = store.plugins;
        debug!(count = self.installed_plugins.len(), "Loaded plugins from file");
        Ok(())
    }

    /// Save plugins to a JSON file.
    pub fn save_plugins_to_file(&self, path: &std::path::Path) -> Result<()> {
        let store = crate::plugin_types::PluginStore {
            plugins: self.installed_plugins.clone(),
        };
        let json = serde_json::to_string_pretty(&store)
            .context("Failed to serialize plugins")?;
        std::fs::write(path, json)
            .context("Failed to write plugins.json")?;
        debug!("Saved {} plugins to file", self.installed_plugins.len());
        Ok(())
    }
```

**Step 2: Run `cargo check -p hive_agents`**

Expected: compiles.

**Step 3: Write tests**

Add to the `mod tests` section in `skill_marketplace.rs`:

```rust
    // -- plugin management --------------------------------------------------

    #[test]
    fn install_and_list_plugin() {
        use crate::plugin_types::*;

        let mut mp = SkillMarketplace::new();
        let preview = PluginPreview {
            manifest: PluginManifest {
                name: "test-plugin".into(),
                description: "A test plugin".into(),
                version: "1.0.0".into(),
                author: PluginAuthor { name: "Test".into(), email: None },
                homepage: None,
                repository: None,
                license: None,
                keywords: vec![],
                skills_path: None,
                commands_path: None,
                agents_path: None,
            },
            skills: vec![
                ParsedSkill {
                    name: "skill-a".into(),
                    description: "Skill A".into(),
                    instructions: "Do thing A.".into(),
                    source_file: "skills/a/SKILL.md".into(),
                },
                ParsedSkill {
                    name: "skill-b".into(),
                    description: "Skill B".into(),
                    instructions: "Do thing B.".into(),
                    source_file: "skills/b/SKILL.md".into(),
                },
            ],
            commands: vec![],
            security_warnings: vec![],
        };

        let source = PluginSource::GitHub {
            owner: "test".into(),
            repo: "plugin".into(),
            branch: None,
        };

        let plugin = mp.install_plugin(&preview, source, &[0, 1], &[]);
        assert_eq!(plugin.name, "test-plugin");
        assert_eq!(plugin.skills.len(), 2);
        assert_eq!(mp.installed_plugins().len(), 1);
    }

    #[test]
    fn remove_plugin() {
        use crate::plugin_types::*;

        let mut mp = SkillMarketplace::new();
        let preview = PluginPreview {
            manifest: PluginManifest {
                name: "to-remove".into(),
                description: "".into(),
                version: "1.0.0".into(),
                author: PluginAuthor::default(),
                homepage: None, repository: None, license: None,
                keywords: vec![], skills_path: None, commands_path: None, agents_path: None,
            },
            skills: vec![],
            commands: vec![],
            security_warnings: vec![],
        };
        let source = PluginSource::Local { path: "/tmp".into() };
        let plugin = mp.install_plugin(&preview, source, &[], &[]);
        assert!(mp.remove_plugin(&plugin.id).is_ok());
        assert!(mp.installed_plugins().is_empty());
    }
```

**Step 4: Run `cargo test -p hive_agents`**

Expected: all tests pass.

**Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/skill_marketplace.rs
git commit -m "feat(agents): add plugin management and persistence to SkillMarketplace"
```

---

## Task 7: UI Actions for Plugin Import

**Files:**
- Modify: `hive/crates/hive_ui_core/src/actions.rs:74-101` (zero-sized actions) and `292-371` (data-carrying actions)
- Modify: `hive/crates/hive_ui_core/src/globals.rs`

**Step 1: Add zero-sized plugin actions to actions! macro**

In `actions.rs`, add after the `SkillsClearSearch` line (line 76):

```rust
        // Plugin import actions
        PluginImportOpen,
        PluginImportCancel,
        PluginImportConfirm,
```

**Step 2: Add data-carrying plugin actions**

After the Skills/ClawdHub section (after line 371):

```rust
// ---------------------------------------------------------------------------
// Plugin import actions
// ---------------------------------------------------------------------------

/// Import a plugin from a GitHub repository (owner/repo format).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportFromGitHub {
    pub owner_repo: String,
}

/// Import a plugin from a URL.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportFromUrl {
    pub url: String,
}

/// Import a plugin from a local path.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportFromLocal {
    pub path: String,
}

/// Toggle a skill checkbox in the import preview.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginImportToggleSkill {
    pub index: usize,
}

/// Remove an installed plugin by ID.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginRemove {
    pub plugin_id: String,
}

/// Update an installed plugin to its latest version.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginUpdate {
    pub plugin_id: String,
}

/// Toggle expand/collapse of an installed plugin group.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginToggleExpand {
    pub plugin_id: String,
}

/// Toggle a skill within an installed plugin.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = hive_workspace, no_json)]
pub struct PluginToggleSkill {
    pub plugin_id: String,
    pub skill_name: String,
}
```

**Step 3: Add AppPluginManager global**

In `globals.rs`, add import at top:

```rust
use hive_agents::plugin_manager::PluginManager;
```

And add after `AppMarketplace` (after line 91):

```rust
/// Global wrapper for the plugin manager (fetch/parse/version-check external plugins).
pub struct AppPluginManager(pub PluginManager);
impl Global for AppPluginManager {}
```

**Step 4: Run `cargo check -p hive_ui_core`**

Expected: compiles.

**Step 5: Commit**

```bash
git add hive/crates/hive_ui_core/src/actions.rs hive/crates/hive_ui_core/src/globals.rs
git commit -m "feat(ui): add plugin import actions and AppPluginManager global"
```

---

## Task 8: Skills Panel UI — Data Types and Import State

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/skills.rs:1-134`

**Step 1: Add new UI types for plugins**

After the existing `SkillSource` struct (after line 122), add:

```rust
/// Import flow state machine.
#[derive(Debug, Clone)]
pub enum ImportState {
    Closed,
    SelectMethod,
    InputGitHub(String),
    InputUrl(String),
    InputLocal(Option<String>),
    Fetching,
    Preview(ImportPreview),
    Installing,
    Done(String, bool), // message, is_success
}

/// Import preview data (UI-friendly version of PluginPreview).
#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub skills: Vec<ImportSkillEntry>,
    pub commands: Vec<ImportCommandEntry>,
    pub security_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ImportSkillEntry {
    pub name: String,
    pub description: String,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct ImportCommandEntry {
    pub name: String,
    pub description: String,
    pub selected: bool,
}

/// An installed plugin displayed in the Installed tab.
#[derive(Debug, Clone)]
pub struct UiInstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub skills: Vec<UiPluginSkill>,
    pub expanded: bool,
    pub update_available: Option<String>, // latest version if different
}

#[derive(Debug, Clone)]
pub struct UiPluginSkill {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}
```

**Step 2: Add new fields to `SkillsData`**

Update the `SkillsData` struct (line 126-134):

```rust
pub struct SkillsData {
    pub installed: Vec<InstalledSkill>,
    pub directory: Vec<DirectorySkill>,
    pub active_tab: SkillsTab,
    pub search_query: String,
    pub selected_category: Option<SkillCategory>,
    pub sources: Vec<SkillSource>,
    pub create_draft: CreateSkillDraft,
    // Plugin import
    pub installed_plugins: Vec<UiInstalledPlugin>,
    pub import_state: ImportState,
}
```

Update `SkillsData::empty()` (line 138-148):

```rust
    pub fn empty() -> Self {
        Self {
            installed: Vec::new(),
            directory: Vec::new(),
            active_tab: SkillsTab::Installed,
            search_query: String::new(),
            selected_category: None,
            sources: Vec::new(),
            create_draft: CreateSkillDraft::empty(),
            installed_plugins: Vec::new(),
            import_state: ImportState::Closed,
        }
    }
```

Also update `sample()` to include empty plugin fields (add at end of sample data).

**Step 3: Run `cargo check -p hive_ui_panels`**

Expected: compiles.

**Step 4: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/skills.rs
git commit -m "feat(ui): add plugin import data types to SkillsData"
```

---

## Task 9: Skills Panel UI — Import Button and Dropdown

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/skills.rs` (render method — the header section)

**Step 1: Add import action imports**

Update the imports at top of file (line 6-10) to include the new actions:

```rust
use hive_ui_core::{
    PluginImportOpen, PluginImportCancel, PluginImportFromGitHub,
    PluginImportFromUrl, PluginImportFromLocal, PluginImportConfirm,
    PluginImportToggleSkill, PluginRemove, PluginUpdate, PluginToggleExpand,
    PluginToggleSkill,
    SkillsAddSource, SkillsClearSearch, SkillsCreate, SkillsInstall, SkillsRefresh,
    SkillsRemove, SkillsRemoveSource, SkillsSetCategory, SkillsSetTab,
    SkillsToggle,
};
```

**Step 2: Add "Import +" button to the header**

Find the header rendering section in `SkillsPanel::render()` where the refresh button is rendered. Add an "Import +" button before the refresh button. The exact rendering code will follow GPUI patterns already used in the file (using `div()`, `Button::new()`, `.on_click()`, etc.).

This is the rendering logic — follow the existing pattern of how the refresh button dispatches `SkillsRefresh` action, and add an "Import" button that dispatches `PluginImportOpen`.

**Step 3: Add dropdown rendering**

When `import_state` is `SelectMethod`, render a dropdown with three options:
- "From GitHub..." → dispatches `PluginImportFromGitHub` with empty string to trigger input
- "From URL..." → dispatches `PluginImportFromUrl`
- "From Local File..." → dispatches `PluginImportFromLocal`

**Step 4: Run `cargo check -p hive_ui_panels`**

Expected: compiles.

**Step 5: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/skills.rs
git commit -m "feat(ui): add Import button and dropdown to Skills panel header"
```

---

## Task 10: Skills Panel UI — Import Preview Screen

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/skills.rs` (render method)

**Step 1: Add import preview rendering**

When `import_state` is `Preview(preview)`, render the preview screen replacing the tab content area. Show:
- Plugin name, version, author, description
- Security warnings (if any) in a collapsible warning section
- Checklist of skills with checkboxes
- Checklist of commands with checkboxes
- Cancel and Install Selected buttons

Follow the existing panel rendering patterns (div containers, theme colors, button styles).

**Step 2: Add input screens**

When `import_state` is `InputGitHub(text)`, render a text input for `owner/repo` with a Fetch button.
When `import_state` is `InputUrl(text)`, render a text input for URL with a Fetch button.
When `import_state` is `Fetching`, render a loading indicator.
When `import_state` is `Done(msg, success)`, render a success/error message with an OK button.

**Step 3: Run `cargo check -p hive_ui_panels`**

Expected: compiles.

**Step 4: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/skills.rs
git commit -m "feat(ui): add import preview and input screens to Skills panel"
```

---

## Task 11: Skills Panel UI — Grouped Installed View

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/skills.rs` (render_installed_tab)

**Step 1: Update the Installed tab rendering**

Replace the current flat list rendering with:

1. First section: installed plugins, each as a collapsible group
   - Header: expand/collapse toggle, plugin name + version, Update badge (if available), Remove button
   - Expanded: list of skills with on/off toggles
2. Second section: "Individual Skills" divider, then the existing flat list of non-plugin skills

Follow the existing card/row rendering pattern used for `InstalledSkill` items.

**Step 2: Run `cargo check -p hive_ui_panels`**

Expected: compiles.

**Step 3: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/skills.rs
git commit -m "feat(ui): add grouped plugin view to Installed tab"
```

---

## Task 12: Workspace — Plugin Action Handlers

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs`

**Step 1: Add plugin action imports**

Add to the imports section (around line 52-54):

```rust
    PluginImportOpen, PluginImportCancel, PluginImportFromGitHub,
    PluginImportFromUrl, PluginImportFromLocal, PluginImportConfirm,
    PluginImportToggleSkill, PluginRemove, PluginUpdate, PluginToggleExpand,
    PluginToggleSkill,
```

**Step 2: Add handler methods**

Add after the existing skills handlers (after line ~5199):

```rust
    fn handle_plugin_import_open(
        &mut self, _action: &PluginImportOpen, _window: &mut Window, cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        self.skills_data.import_state = ImportState::SelectMethod;
        cx.notify();
    }

    fn handle_plugin_import_cancel(
        &mut self, _action: &PluginImportCancel, _window: &mut Window, cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;
        self.skills_data.import_state = ImportState::Closed;
        cx.notify();
    }

    fn handle_plugin_import_from_github(
        &mut self, action: &PluginImportFromGitHub, _window: &mut Window, cx: &mut Context<Self>,
    ) {
        use hive_ui_panels::panels::skills::ImportState;

        if action.owner_repo.is_empty() {
            self.skills_data.import_state = ImportState::InputGitHub(String::new());
            cx.notify();
            return;
        }

        // Parse owner/repo.
        let parts: Vec<&str> = action.owner_repo.splitn(2, '/').collect();
        if parts.len() != 2 {
            self.skills_data.import_state = ImportState::Done(
                "Invalid format. Use owner/repo".into(), false,
            );
            cx.notify();
            return;
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        self.skills_data.import_state = ImportState::Fetching;
        cx.notify();

        // Spawn async fetch on background thread.
        if cx.has_global::<hive_ui_core::AppPluginManager>() {
            let client = cx.global::<hive_ui_core::AppPluginManager>().0.client.clone();
            // ... spawn background task, update import_state on completion
        }
    }

    // Similar handlers for:
    // handle_plugin_import_from_url
    // handle_plugin_import_from_local
    // handle_plugin_import_confirm  (calls marketplace.install_plugin + save)
    // handle_plugin_import_toggle_skill
    // handle_plugin_remove
    // handle_plugin_update
    // handle_plugin_toggle_expand
    // handle_plugin_toggle_skill
```

**Step 3: Register action handlers**

In the `render()` method action chain (around line 6762-6772), add:

```rust
            .on_action(cx.listener(Self::handle_plugin_import_open))
            .on_action(cx.listener(Self::handle_plugin_import_cancel))
            .on_action(cx.listener(Self::handle_plugin_import_from_github))
            .on_action(cx.listener(Self::handle_plugin_import_from_url))
            .on_action(cx.listener(Self::handle_plugin_import_from_local))
            .on_action(cx.listener(Self::handle_plugin_import_confirm))
            .on_action(cx.listener(Self::handle_plugin_import_toggle_skill))
            .on_action(cx.listener(Self::handle_plugin_remove))
            .on_action(cx.listener(Self::handle_plugin_update))
            .on_action(cx.listener(Self::handle_plugin_toggle_expand))
            .on_action(cx.listener(Self::handle_plugin_toggle_skill))
```

**Step 4: Update `refresh_skills_data` to include plugins**

In `refresh_skills_data()` (line 997-1087), add after the marketplace section:

```rust
        // Populate installed plugins.
        if cx.has_global::<AppMarketplace>() {
            let mp = &cx.global::<AppMarketplace>().0;
            self.skills_data.installed_plugins = mp.installed_plugins()
                .iter()
                .map(|p| hive_ui_panels::panels::skills::UiInstalledPlugin {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    version: p.version.clone(),
                    author: p.author.name.clone(),
                    description: p.description.clone(),
                    skills: p.skills.iter().map(|s| hive_ui_panels::panels::skills::UiPluginSkill {
                        name: s.name.clone(),
                        description: s.description.clone(),
                        enabled: s.enabled,
                    }).collect(),
                    expanded: false,
                    update_available: None,
                })
                .collect();
        }
```

**Step 5: Run `cargo check -p hive_ui`**

Expected: compiles.

**Step 6: Commit**

```bash
git add hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(ui): add plugin action handlers to workspace"
```

---

## Task 13: App Bootstrap — Init PluginManager and Load Plugins

**Files:**
- Modify: `hive/crates/hive_app/src/main.rs:277-285`

**Step 1: Add PluginManager initialization**

After the `SkillMarketplace` init (line 284), add:

```rust
    // Plugin manager — fetch/parse external plugin packages.
    cx.set_global(AppPluginManager(hive_agents::PluginManager::new(
        reqwest::Client::new(),
    )));
    info!("PluginManager initialized");

    // Load installed plugins from disk.
    {
        let plugins_path = hive_dir.join("plugins.json");
        let mp = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = mp.load_plugins_from_file(&plugins_path) {
            warn!("Failed to load plugins: {e}");
        }
    }
```

**Step 2: Add `AppPluginManager` to imports**

In the imports at top of `main.rs` (line 22-28), add `AppPluginManager`:

```rust
    AppKnowledge, AppKubernetes, AppLearning, AppMarketplace, AppMcpServer, AppMessaging,
    AppNetwork, AppNotifications, AppPersonas, AppPluginManager,
```

**Step 3: Run `cargo check -p hive_app`**

Expected: compiles.

**Step 4: Run `cargo build`**

Expected: full workspace builds.

**Step 5: Commit**

```bash
git add hive/crates/hive_app/src/main.rs
git commit -m "feat(app): init PluginManager and load plugins on startup"
```

---

## Task 14: Integration Testing

**Step 1: Run full test suite**

```bash
cargo test --workspace
```

Expected: all existing tests pass, plus new plugin tests.

**Step 2: Manual smoke test**

```bash
cargo run -p hive_app
```

1. Open Skills panel
2. Verify "Import +" button appears
3. Click Import, verify dropdown shows three options
4. Click "From GitHub..."
5. Enter `obra/superpowers`
6. Verify preview screen shows skills and commands
7. Click "Install Selected"
8. Verify plugins appear in Installed tab as collapsible group
9. Toggle skills on/off within plugin
10. Remove plugin
11. Verify `~/.hive/plugins.json` persists correctly

**Step 3: Commit any fixes**

```bash
git commit -m "fix: integration fixes from smoke testing"
```

---

## Task 15: Update Obsidian Vault

**Files:**
- Modify: `H:/WORK/AG/Obsidian/Hive/HiveCode/Features/UI Panels.md`
- Modify: `H:/WORK/AG/Obsidian/Hive/HiveCode/Features/Slash Commands.md`

**Step 1: Update UI Panels doc**

Update the Skills panel row in the panel table to reflect the new import capability.

**Step 2: Update Slash Commands doc**

Add a note about plugin-imported skills being available as slash commands.

**Step 3: Commit all changes**

```bash
git add -A
git commit -m "docs: update Obsidian vault with plugin import feature"
```
