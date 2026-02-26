//! Plugin Manager — fetch, parse, and version-check external plugin packages.

use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::plugin_types::{
    CachedVersion, InstalledPlugin, ParsedCommand, ParsedSkill, PluginCache, PluginManifest,
    PluginPreview, PluginSource, UpdateAvailable,
};
use crate::skill_marketplace::{SecurityIssue, SkillMarketplace};

// ---------------------------------------------------------------------------
// PluginManager
// ---------------------------------------------------------------------------

/// Manages fetching, parsing, and version-checking of external plugin packages.
#[derive(Clone)]
pub struct PluginManager {
    client: reqwest::Client,
}

impl PluginManager {
    /// Create a new `PluginManager` backed by the given HTTP client.
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    // -----------------------------------------------------------------------
    // Parsing helpers (Task 2)
    // -----------------------------------------------------------------------

    /// Split YAML frontmatter from markdown content.
    ///
    /// Expects the content to start with `---\n` and the frontmatter to end
    /// at the next `\n---` marker. Returns `(frontmatter, body)` on success.
    pub fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
        let trimmed = content.strip_prefix("---")?;
        // The opening `---` must be followed by a newline (or be at EOF for an
        // edge case, but we require body).
        let trimmed = trimmed.strip_prefix('\n').or_else(|| trimmed.strip_prefix("\r\n"))?;

        // Find the closing `---` on its own line.
        let end = trimmed.find("\n---")?;
        let frontmatter = &trimmed[..end];
        let rest = &trimmed[end + 4..]; // skip `\n---`

        // The body starts after the optional newline following the closing `---`.
        let body = rest
            .strip_prefix('\n')
            .or_else(|| rest.strip_prefix("\r\n"))
            .unwrap_or(rest);

        Some((frontmatter, body))
    }

    /// Parse a SKILL.md file into a [`ParsedSkill`].
    ///
    /// The file is expected to have optional YAML frontmatter containing
    /// `name:` and `description:` fields. If `name` is absent it is derived
    /// from `source_file` (its parent directory name).
    pub fn parse_skill_md(content: &str, source_file: &str) -> Result<ParsedSkill> {
        let (name, description, instructions) =
            if let Some((fm, body)) = Self::split_frontmatter(content) {
                let name = Self::extract_fm_field(fm, "name");
                let desc = Self::extract_fm_field(fm, "description").unwrap_or_default();
                (name, desc, body.to_owned())
            } else {
                (None, String::new(), content.to_owned())
            };

        let name = name.unwrap_or_else(|| Self::name_from_skill_path(source_file));

        Ok(ParsedSkill {
            name,
            description,
            instructions,
            source_file: source_file.to_owned(),
        })
    }

    /// Parse a command markdown file into a [`ParsedCommand`].
    ///
    /// The command name is derived from the filename stem if not present in
    /// frontmatter.
    pub fn parse_command_md(content: &str, source_file: &str) -> Result<ParsedCommand> {
        let (name, description, instructions) =
            if let Some((fm, body)) = Self::split_frontmatter(content) {
                let name = Self::extract_fm_field(fm, "name");
                let desc = Self::extract_fm_field(fm, "description").unwrap_or_default();
                (name, desc, body.to_owned())
            } else {
                (None, String::new(), content.to_owned())
            };

        let name = name.unwrap_or_else(|| Self::name_from_filename_stem(source_file));

        Ok(ParsedCommand {
            name,
            description,
            instructions,
            source_file: source_file.to_owned(),
        })
    }

    /// Deserialize a `plugin.json` string into a [`PluginManifest`].
    pub fn parse_manifest(json: &str) -> Result<PluginManifest> {
        serde_json::from_str(json).context("failed to parse plugin.json manifest")
    }

    /// Compute the SHA-256 hex digest of `content`.
    pub fn integrity_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Run the injection scanner over all skill and command instructions,
    /// collecting any security warnings.
    pub fn scan_plugin(skills: &[ParsedSkill], commands: &[ParsedCommand]) -> Vec<SecurityIssue> {
        let mut warnings = Vec::new();
        for skill in skills {
            warnings.extend(SkillMarketplace::scan_for_injection(&skill.instructions));
        }
        for cmd in commands {
            warnings.extend(SkillMarketplace::scan_for_injection(&cmd.instructions));
        }
        warnings
    }

    // -----------------------------------------------------------------------
    // GitHub fetching (Task 3)
    // -----------------------------------------------------------------------

    /// Fetch a plugin or standalone skill from a GitHub repository.
    ///
    /// Looks for `.claude-plugin/plugin.json` or `plugin.json` in the repo
    /// root. If found, fetches the full plugin. Otherwise, looks for
    /// `SKILL.md` files and wraps them as a synthetic plugin.
    pub async fn fetch_from_github(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<PluginPreview> {
        // 1. Determine default branch
        let repo_url = format!("https://api.github.com/repos/{owner}/{repo}");
        let repo_json: serde_json::Value = self
            .client
            .get(&repo_url)
            .header("User-Agent", "Hive")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("GitHub repo request failed")?
            .error_for_status()
            .context("GitHub repo request returned error status")?
            .json()
            .await
            .context("failed to parse GitHub repo JSON")?;

        let branch = repo_json["default_branch"]
            .as_str()
            .unwrap_or("main")
            .to_owned();

        debug!("GitHub {owner}/{repo}: default branch = {branch}");

        // 2. Fetch recursive file tree
        let tree_url = format!(
            "https://api.github.com/repos/{owner}/{repo}/git/trees/{branch}?recursive=1"
        );
        let tree_json: serde_json::Value = self
            .client
            .get(&tree_url)
            .header("User-Agent", "Hive")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("GitHub tree request failed")?
            .error_for_status()
            .context("GitHub tree request returned error status")?
            .json()
            .await
            .context("failed to parse GitHub tree JSON")?;

        let tree = tree_json["tree"]
            .as_array()
            .context("missing tree array in GitHub response")?;

        let paths: Vec<String> = tree
            .iter()
            .filter_map(|e| e["path"].as_str().map(|s| s.to_owned()))
            .collect();

        // 3. Look for manifest
        let manifest_path = if paths.iter().any(|p| p == ".claude-plugin/plugin.json") {
            Some(".claude-plugin/plugin.json".to_owned())
        } else if paths.iter().any(|p| p == "plugin.json") {
            Some("plugin.json".to_owned())
        } else {
            None
        };

        if let Some(manifest_path) = manifest_path {
            self.fetch_full_plugin_from_github(owner, repo, &branch, &manifest_path, &paths)
                .await
        } else {
            // Look for any SKILL.md file
            let skill_files: Vec<&String> = paths
                .iter()
                .filter(|p| p.ends_with("SKILL.md") || p.ends_with("skill.md"))
                .collect();

            if skill_files.is_empty() {
                bail!("No plugin.json or SKILL.md found in {owner}/{repo}");
            }

            self.fetch_single_skill_from_github(owner, repo, &branch, skill_files[0])
                .await
        }
    }

    /// Fetch a full plugin (manifest + skills + commands) from GitHub.
    async fn fetch_full_plugin_from_github(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        manifest_path: &str,
        paths: &[String],
    ) -> Result<PluginPreview> {
        let manifest_content = self
            .fetch_github_file(owner, repo, branch, manifest_path)
            .await
            .context("failed to fetch plugin.json")?;
        let manifest = Self::parse_manifest(&manifest_content)?;

        // Determine skills directory (default "skills")
        let skills_dir = manifest
            .skills_path
            .as_deref()
            .unwrap_or("skills");
        let commands_dir = manifest
            .commands_path
            .as_deref()
            .unwrap_or("commands");

        // Resolve paths relative to manifest location
        let prefix = if let Some(idx) = manifest_path.rfind('/') {
            &manifest_path[..=idx]
        } else {
            ""
        };

        let skills_prefix = if prefix.is_empty() {
            format!("{skills_dir}/")
        } else {
            format!("{prefix}{skills_dir}/")
        };
        let commands_prefix = if prefix.is_empty() {
            format!("{commands_dir}/")
        } else {
            format!("{prefix}{commands_dir}/")
        };

        // Collect skill files
        let skill_files: Vec<&String> = paths
            .iter()
            .filter(|p| {
                p.starts_with(&skills_prefix)
                    && (p.ends_with(".md") || p.ends_with(".MD"))
            })
            .collect();

        let command_files: Vec<&String> = paths
            .iter()
            .filter(|p| {
                p.starts_with(&commands_prefix)
                    && (p.ends_with(".md") || p.ends_with(".MD"))
            })
            .collect();

        let mut skills = Vec::new();
        for path in &skill_files {
            match self.fetch_github_file(owner, repo, branch, path).await {
                Ok(content) => match Self::parse_skill_md(&content, path) {
                    Ok(skill) => skills.push(skill),
                    Err(e) => warn!("Skipping skill {path}: {e}"),
                },
                Err(e) => warn!("Failed to fetch skill {path}: {e}"),
            }
        }

        let mut commands = Vec::new();
        for path in &command_files {
            match self.fetch_github_file(owner, repo, branch, path).await {
                Ok(content) => match Self::parse_command_md(&content, path) {
                    Ok(cmd) => commands.push(cmd),
                    Err(e) => warn!("Skipping command {path}: {e}"),
                },
                Err(e) => warn!("Failed to fetch command {path}: {e}"),
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

    /// Wrap a single SKILL.md file as a synthetic plugin preview.
    async fn fetch_single_skill_from_github(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        skill_path: &str,
    ) -> Result<PluginPreview> {
        let content = self
            .fetch_github_file(owner, repo, branch, skill_path)
            .await
            .context("failed to fetch SKILL.md")?;

        let skill = Self::parse_skill_md(&content, skill_path)?;
        let security_warnings = Self::scan_plugin(&[skill.clone()], &[]);

        let manifest = PluginManifest {
            name: skill.name.clone(),
            description: skill.description.clone(),
            version: "0.0.0".to_owned(),
            author: Default::default(),
            homepage: Some(format!("https://github.com/{owner}/{repo}")),
            repository: Some(format!("https://github.com/{owner}/{repo}")),
            license: None,
            keywords: vec![],
            skills_path: None,
            commands_path: None,
            agents_path: None,
        };

        Ok(PluginPreview {
            manifest,
            skills: vec![skill],
            commands: vec![],
            security_warnings,
        })
    }

    /// Fetch a single file from GitHub via the Contents API, decoding the
    /// base64-encoded content.
    async fn fetch_github_file(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        path: &str,
    ) -> Result<String> {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/contents/{path}?ref={branch}"
        );
        let json: serde_json::Value = self
            .client
            .get(&url)
            .header("User-Agent", "Hive")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("GitHub file request failed")?
            .error_for_status()
            .context("GitHub file request returned error status")?
            .json()
            .await
            .context("failed to parse GitHub file JSON")?;

        let encoded = json["content"]
            .as_str()
            .context("missing content field in GitHub response")?;

        // GitHub returns base64 with newlines; strip them then decode.
        let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = decode_base64(&cleaned).context("base64 decode of GitHub content failed")?;
        String::from_utf8(bytes).context("GitHub file content is not valid UTF-8")
    }

    // -----------------------------------------------------------------------
    // URL fetching (Task 4)
    // -----------------------------------------------------------------------

    /// Fetch a plugin or standalone skill from an arbitrary URL.
    ///
    /// If the URL ends in `.md`, the content is treated as a single SKILL.md.
    /// Otherwise it is parsed as a `plugin.json` manifest.
    pub async fn fetch_from_url(&self, url: &str) -> Result<PluginPreview> {
        let body = self
            .client
            .get(url)
            .header("User-Agent", "Hive")
            .send()
            .await
            .context("HTTP request to plugin URL failed")?
            .error_for_status()
            .context("plugin URL returned error status")?
            .text()
            .await
            .context("failed to read plugin URL body")?;

        if url.ends_with(".md") || url.ends_with(".MD") {
            let skill = Self::parse_skill_md(&body, url)?;
            let security_warnings = Self::scan_plugin(&[skill.clone()], &[]);
            let manifest = PluginManifest {
                name: skill.name.clone(),
                description: skill.description.clone(),
                version: "0.0.0".to_owned(),
                author: Default::default(),
                homepage: Some(url.to_owned()),
                repository: None,
                license: None,
                keywords: vec![],
                skills_path: None,
                commands_path: None,
                agents_path: None,
            };
            Ok(PluginPreview {
                manifest,
                skills: vec![skill],
                commands: vec![],
                security_warnings,
            })
        } else {
            // Assume plugin.json
            let manifest = Self::parse_manifest(&body)?;
            let security_warnings = Vec::new();
            Ok(PluginPreview {
                manifest,
                skills: vec![],
                commands: vec![],
                security_warnings,
            })
        }
    }

    /// Load a plugin from a local path (file or directory).
    ///
    /// - If `path` is a `.md` file, it is treated as a single skill.
    /// - If `path` is a `plugin.json` file, its parent directory is loaded.
    /// - If `path` is a directory, the directory is scanned for a manifest
    ///   at `plugin.json` or `.claude-plugin/plugin.json`.
    pub fn load_from_local(path: &Path) -> Result<PluginPreview> {
        if path.is_file() {
            let content =
                std::fs::read_to_string(path).context("failed to read local plugin file")?;
            let source_file = path.to_string_lossy().to_string();

            if source_file.ends_with(".md") || source_file.ends_with(".MD") {
                let skill = Self::parse_skill_md(&content, &source_file)?;
                let security_warnings = Self::scan_plugin(&[skill.clone()], &[]);
                let manifest = PluginManifest {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: "0.0.0".to_owned(),
                    author: Default::default(),
                    homepage: None,
                    repository: None,
                    license: None,
                    keywords: vec![],
                    skills_path: None,
                    commands_path: None,
                    agents_path: None,
                };
                return Ok(PluginPreview {
                    manifest,
                    skills: vec![skill],
                    commands: vec![],
                    security_warnings,
                });
            }

            // Assume plugin.json — load the parent directory
            let dir = path
                .parent()
                .context("plugin.json has no parent directory")?;
            Self::load_plugin_directory(dir, &content)
        } else if path.is_dir() {
            // Try standard manifest locations
            let manifest_path = if path.join("plugin.json").is_file() {
                path.join("plugin.json")
            } else if path.join(".claude-plugin/plugin.json").is_file() {
                path.join(".claude-plugin/plugin.json")
            } else {
                bail!(
                    "No plugin.json or .claude-plugin/plugin.json found in {}",
                    path.display()
                );
            };
            let content = std::fs::read_to_string(&manifest_path)
                .context("failed to read plugin.json")?;
            Self::load_plugin_directory(path, &content)
        } else {
            bail!("Path does not exist: {}", path.display());
        }
    }

    /// Load a plugin from a local directory given its manifest JSON.
    fn load_plugin_directory(root: &Path, manifest_json: &str) -> Result<PluginPreview> {
        let manifest = Self::parse_manifest(manifest_json)?;

        let skills_dir_name = manifest.skills_path.as_deref().unwrap_or("skills");
        let commands_dir_name = manifest.commands_path.as_deref().unwrap_or("commands");

        let skills_dir = root.join(skills_dir_name);
        let commands_dir = root.join(commands_dir_name);

        let mut skills = Vec::new();
        if skills_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            let source = p.to_string_lossy().to_string();
                            match Self::parse_skill_md(&content, &source) {
                                Ok(s) => skills.push(s),
                                Err(e) => warn!("Skipping skill {}: {e}", p.display()),
                            }
                        }
                    }
                }
            }
        }

        let mut commands = Vec::new();
        if commands_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&commands_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            let source = p.to_string_lossy().to_string();
                            match Self::parse_command_md(&content, &source) {
                                Ok(c) => commands.push(c),
                                Err(e) => warn!("Skipping command {}: {e}", p.display()),
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

    // -----------------------------------------------------------------------
    // Version checking (Task 5)
    // -----------------------------------------------------------------------

    /// Check for available updates on all GitHub-sourced plugins.
    ///
    /// Results are throttled to at most once per hour via the provided cache.
    pub async fn check_for_updates(
        &self,
        plugins: &[InstalledPlugin],
        cache: &mut PluginCache,
    ) -> Vec<UpdateAvailable> {
        // Throttle: skip if last check was less than 1 hour ago.
        if let Some(last) = cache.last_checked {
            let elapsed = Utc::now().signed_duration_since(last);
            if elapsed.num_seconds() < 3600 {
                debug!("Skipping plugin update check — last checked {}s ago", elapsed.num_seconds());
                return Vec::new();
            }
        }

        let mut updates = Vec::new();

        for plugin in plugins {
            let (owner, repo) = match &plugin.source {
                PluginSource::GitHub { owner, repo, .. } => (owner.clone(), repo.clone()),
                _ => continue,
            };

            // Try to fetch just the manifest
            let manifest_result = async {
                // Try .claude-plugin/plugin.json first
                let path = ".claude-plugin/plugin.json";
                match self.fetch_github_file(&owner, &repo, "HEAD", path).await {
                    Ok(content) => return Self::parse_manifest(&content),
                    Err(_) => {}
                }
                // Fall back to plugin.json
                let path = "plugin.json";
                let content = self
                    .fetch_github_file(&owner, &repo, "HEAD", path)
                    .await?;
                Self::parse_manifest(&content)
            }
            .await;

            match manifest_result {
                Ok(manifest) => {
                    cache.versions.insert(
                        plugin.id.clone(),
                        CachedVersion {
                            latest_version: manifest.version.clone(),
                            checked_at: Utc::now(),
                        },
                    );
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
                Err(e) => {
                    warn!("Failed to check updates for {}: {e}", plugin.name);
                }
            }
        }

        cache.last_checked = Some(Utc::now());
        updates
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Extract a simple `key: value` from YAML frontmatter (no full YAML parser).
    fn extract_fm_field(frontmatter: &str, key: &str) -> Option<String> {
        for line in frontmatter.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(key) {
                let rest = rest.trim_start();
                if let Some(value) = rest.strip_prefix(':') {
                    let value = value.trim();
                    // Strip surrounding quotes if present
                    let value = value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .or_else(|| {
                            value
                                .strip_prefix('\'')
                                .and_then(|v| v.strip_suffix('\''))
                        })
                        .unwrap_or(value);
                    if !value.is_empty() {
                        return Some(value.to_owned());
                    }
                }
            }
        }
        None
    }

    /// Derive a skill name from a SKILL.md source path by using the parent
    /// directory name.
    fn name_from_skill_path(source_file: &str) -> String {
        // e.g. "skills/code-review/SKILL.md" → "code-review"
        let path = Path::new(source_file);
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown-skill")
            .to_owned()
    }

    /// Derive a command name from the filename stem.
    fn name_from_filename_stem(source_file: &str) -> String {
        Path::new(source_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown-command")
            .to_owned()
    }
}

// ---------------------------------------------------------------------------
// Standalone base64 decoder (no external crate)
// ---------------------------------------------------------------------------

/// Decode a standard base64 string (RFC 4648) into bytes.
fn decode_base64(input: &str) -> Result<Vec<u8>> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn val(c: u8) -> Result<u8> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => bail!("invalid base64 character: {}", c as char),
        }
    }

    let _ = TABLE; // suppress unused warning for the const table

    let input = input.trim_end_matches('=');
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);

    let chunks = bytes.chunks(4);
    for chunk in chunks {
        let a = val(chunk[0])?;
        let b = if chunk.len() > 1 { val(chunk[1])? } else { 0 };
        let c = if chunk.len() > 2 { val(chunk[2])? } else { 0 };
        let d = if chunk.len() > 3 { val(chunk[3])? } else { 0 };

        let n = (u32::from(a) << 18) | (u32::from(b) << 12) | (u32::from(c) << 6) | u32::from(d);

        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8 & 0xFF) as u8);
        }
        if chunk.len() > 3 {
            out.push((n & 0xFF) as u8);
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter_valid() {
        let content = "---\nname: test\ndescription: a test\n---\nBody content here.";
        let (fm, body) = PluginManager::split_frontmatter(content).unwrap();
        assert_eq!(fm, "name: test\ndescription: a test");
        assert_eq!(body, "Body content here.");
    }

    #[test]
    fn test_split_frontmatter_missing() {
        let content = "No frontmatter here.";
        assert!(PluginManager::split_frontmatter(content).is_none());
    }

    #[test]
    fn test_split_frontmatter_no_closing() {
        let content = "---\nname: test\nNo closing marker";
        assert!(PluginManager::split_frontmatter(content).is_none());
    }

    #[test]
    fn test_split_frontmatter_empty_body() {
        let content = "---\nname: test\n---\n";
        let (fm, body) = PluginManager::split_frontmatter(content).unwrap();
        assert_eq!(fm, "name: test");
        assert_eq!(body, "");
    }

    #[test]
    fn test_parse_skill_md_basic() {
        let content = "---\nname: code-review\ndescription: Reviews code\n---\nDo the review.";
        let skill = PluginManager::parse_skill_md(content, "skills/code-review/SKILL.md").unwrap();
        assert_eq!(skill.name, "code-review");
        assert_eq!(skill.description, "Reviews code");
        assert_eq!(skill.instructions, "Do the review.");
        assert_eq!(skill.source_file, "skills/code-review/SKILL.md");
    }

    #[test]
    fn test_parse_skill_md_name_from_path() {
        let content = "---\ndescription: Reviews code\n---\nDo the review.";
        let skill = PluginManager::parse_skill_md(content, "skills/my-skill/SKILL.md").unwrap();
        assert_eq!(skill.name, "my-skill");
    }

    #[test]
    fn test_parse_skill_md_no_frontmatter() {
        let content = "Just some instructions.";
        let skill =
            PluginManager::parse_skill_md(content, "skills/fallback-name/SKILL.md").unwrap();
        assert_eq!(skill.name, "fallback-name");
        assert_eq!(skill.instructions, "Just some instructions.");
    }

    #[test]
    fn test_parse_command_md() {
        let content = "---\nname: deploy\ndescription: Deploy the app\n---\nRun deploy.";
        let cmd = PluginManager::parse_command_md(content, "commands/deploy.md").unwrap();
        assert_eq!(cmd.name, "deploy");
        assert_eq!(cmd.description, "Deploy the app");
        assert_eq!(cmd.instructions, "Run deploy.");
    }

    #[test]
    fn test_parse_command_md_name_from_stem() {
        let content = "---\ndescription: test\n---\nInstructions.";
        let cmd = PluginManager::parse_command_md(content, "commands/my-cmd.md").unwrap();
        assert_eq!(cmd.name, "my-cmd");
    }

    #[test]
    fn test_parse_manifest_valid() {
        let json = r#"{
            "name": "test-plugin",
            "description": "A test plugin",
            "version": "1.0.0"
        }"#;
        let manifest = PluginManager::parse_manifest(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_parse_manifest_invalid() {
        let json = r#"{ not valid json }"#;
        assert!(PluginManager::parse_manifest(json).is_err());
    }

    #[test]
    fn test_integrity_hash_deterministic() {
        let content = "Hello, world!";
        let h1 = PluginManager::integrity_hash(content);
        let h2 = PluginManager::integrity_hash(content);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_integrity_hash_known_value() {
        // SHA-256 of empty string
        let hash = PluginManager::integrity_hash("");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_scan_plugin_clean() {
        let skills = vec![ParsedSkill {
            name: "test".into(),
            description: "test".into(),
            instructions: "Write a hello world program.".into(),
            source_file: "test.md".into(),
        }];
        let warnings = PluginManager::scan_plugin(&skills, &[]);
        assert!(warnings.is_empty(), "Clean text should produce no warnings");
    }

    #[test]
    fn test_decode_base64_simple() {
        let encoded = "SGVsbG8=";
        let decoded = decode_base64(encoded).unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_decode_base64_no_padding() {
        let encoded = "SGVsbG8";
        let decoded = decode_base64(encoded).unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_extract_fm_field() {
        let fm = "name: my-skill\ndescription: A skill";
        assert_eq!(
            PluginManager::extract_fm_field(fm, "name"),
            Some("my-skill".to_owned())
        );
        assert_eq!(
            PluginManager::extract_fm_field(fm, "description"),
            Some("A skill".to_owned())
        );
        assert_eq!(PluginManager::extract_fm_field(fm, "missing"), None);
    }

    #[test]
    fn test_extract_fm_field_quoted() {
        let fm = "name: \"my skill\"\ntag: 'single'";
        assert_eq!(
            PluginManager::extract_fm_field(fm, "name"),
            Some("my skill".to_owned())
        );
        assert_eq!(
            PluginManager::extract_fm_field(fm, "tag"),
            Some("single".to_owned())
        );
    }
}
