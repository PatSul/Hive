//! Plugin types — data model for imported plugin packages.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::skill_marketplace::SecurityIssue;

// ---------------------------------------------------------------------------
// Manifest types (parsed from plugin.json)
// ---------------------------------------------------------------------------

/// Author information embedded in a plugin manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

/// Parsed representation of a `plugin.json` manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub author: PluginAuthor,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(rename = "skills", default)]
    pub skills_path: Option<String>,
    #[serde(rename = "commands", default)]
    pub commands_path: Option<String>,
    #[serde(rename = "agents", default)]
    pub agents_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsed content types
// ---------------------------------------------------------------------------

/// A skill definition parsed from a plugin package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSkill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

/// A command definition parsed from a plugin package.
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
        #[serde(default)]
        branch: Option<String>,
    },
    Url(String),
    Local {
        path: String,
    },
}

// ---------------------------------------------------------------------------
// Installed plugin types
// ---------------------------------------------------------------------------

/// A command that has been installed as part of a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledCommand {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

/// A skill that has been installed as part of a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
    pub enabled: bool,
    pub integrity_hash: String,
}

/// A fully installed plugin with all its content.
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
// Preview & update types
// ---------------------------------------------------------------------------

/// Preview of a plugin before installation, including security analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPreview {
    pub manifest: PluginManifest,
    pub skills: Vec<ParsedSkill>,
    pub commands: Vec<ParsedCommand>,
    pub security_warnings: Vec<SecurityIssue>,
}

/// Indicates that a newer version of an installed plugin is available.
#[derive(Debug, Clone)]
pub struct UpdateAvailable {
    pub plugin_id: String,
    pub plugin_name: String,
    pub current_version: String,
    pub latest_version: String,
    pub source: PluginSource,
}

// ---------------------------------------------------------------------------
// Persistence types
// ---------------------------------------------------------------------------

/// Top-level store persisted to `~/.hive/plugins.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginStore {
    pub plugins: Vec<InstalledPlugin>,
}

/// Cache for version-check results to avoid repeated network lookups.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCache {
    #[serde(default)]
    pub last_checked: Option<DateTime<Utc>>,
    #[serde(default)]
    pub versions: HashMap<String, CachedVersion>,
}

/// A single cached version entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedVersion {
    pub latest_version: String,
    pub checked_at: DateTime<Utc>,
}
