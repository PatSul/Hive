# Plugin Import System Design

**Date:** 2026-02-25
**Status:** Approved
**Approach:** B — New PluginManager service + extended UI

## Summary

Add the ability to import skills into Hive from external sources: GitHub repos, URLs, and local files. Supports both single SKILL.md files and full plugin packages using the superpowers format (`plugin.json` + `skills/` + `commands/` + `agents/`).

## Requirements

- Import via three methods: GitHub `owner/repo`, raw URL, local file/directory
- Support single SKILL.md files and full plugin packages (plugin.json manifest)
- Installed plugins displayed as collapsible groups with individual skill toggles
- Version checking on Skills panel open with "Update available" badge
- Security scanning with warnings (non-blocking) during import preview
- All imported skill instructions run through existing 6-pattern injection scanner

## Data Model

### New Types (`hive_agents/src/plugin_types.rs`)

```rust
/// Parsed from plugin.json
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: PluginAuthor,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Vec<String>,
    pub skills_path: Option<String>,    // e.g. "./skills/"
    pub commands_path: Option<String>,  // e.g. "./commands/"
    pub agents_path: Option<String>,    // e.g. "./agents/"
}

pub struct PluginAuthor {
    pub name: String,
    pub email: Option<String>,
}

/// Parsed from SKILL.md (YAML frontmatter + markdown body)
pub struct ParsedSkill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

/// Parsed from commands/*.md
pub struct ParsedCommand {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

/// Where a plugin was imported from
pub enum PluginSource {
    GitHub { owner: String, repo: String, branch: Option<String> },
    Url(String),
    Local(PathBuf),
}

/// An installed plugin group (persisted to plugins.json)
pub struct InstalledPlugin {
    pub id: String,                     // UUID
    pub name: String,
    pub version: String,
    pub author: PluginAuthor,
    pub description: String,
    pub source: PluginSource,
    pub installed_at: DateTime<Utc>,
    pub skills: Vec<InstalledSkill>,    // Reuses existing type
    pub commands: Vec<InstalledCommand>,
}

pub struct InstalledCommand {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source_file: String,
}

/// Pre-install preview returned by PluginManager
pub struct PluginPreview {
    pub manifest: PluginManifest,
    pub skills: Vec<ParsedSkill>,
    pub commands: Vec<ParsedCommand>,
    pub security_warnings: Vec<SecurityIssue>,
}

/// Version check result
pub struct UpdateAvailable {
    pub plugin_id: String,
    pub plugin_name: String,
    pub current_version: String,
    pub latest_version: String,
    pub source: PluginSource,
}
```

### Extended Existing Types

```rust
// SkillMarketplace — add field:
pub installed_plugins: Vec<InstalledPlugin>,

// SkillsData (UI) — add fields:
pub installed_plugins: Vec<UiInstalledPlugin>,
pub import_state: ImportState,

pub enum ImportState {
    Closed,
    SelectMethod,
    InputGitHub(String),
    InputUrl(String),
    InputLocal(Option<PathBuf>),
    Fetching,
    Preview(PluginPreview),
    Installing,
    Done(Result<String, String>),
}
```

## PluginManager Service

### Responsibilities

```
PluginManager
  fetch_from_github(owner, repo) -> PluginPreview
  fetch_from_url(url) -> PluginPreview
  load_from_local(path) -> PluginPreview
  check_for_updates(installed_plugins) -> Vec<UpdateAvailable>
  parse_manifest(json) -> PluginManifest
  parse_skill_md(content) -> ParsedSkill
  parse_command_md(content) -> ParsedCommand
```

### Fetch Flow

All three import methods converge on the same pipeline:

```
Input (GitHub / URL / Local)
    |
    v
Detect format:
  Has plugin.json? -> Full plugin (parse manifest, enumerate skills/, commands/, agents/)
  Single .md file? -> Wrap as single-skill plugin (synthetic manifest)
    |
    v
Parse all SKILL.md files (YAML frontmatter + markdown body)
Parse all command .md files
    |
    v
Run security scan on each skill's instructions -> collect warnings
    |
    v
Return PluginPreview { manifest, skills, commands, security_warnings }
```

### GitHub Fetching

Uses GitHub REST API via reqwest (no git clone):

1. `GET /repos/{owner}/{repo}` — get default branch
2. `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1` — get full file tree
3. Detect `plugin.json` or `.claude-plugin/plugin.json` at root
4. `GET /repos/{owner}/{repo}/contents/{path}` — fetch individual files (base64 encoded)

### Version Checking

- Triggered on Skills panel open
- Throttled to at most once per hour (cached in `plugin_cache.json`)
- For each installed plugin with GitHub or URL source, fetch only `plugin.json`
- Compare version strings via semver
- Non-blocking: panel renders immediately, update badges appear as checks complete

### Registration

```rust
// globals.rs
pub struct AppPluginManager(pub PluginManager);
impl Global for AppPluginManager {}

// main.rs init_services()
let plugin_manager = PluginManager::new(reqwest::Client::new());
cx.set_global(AppPluginManager(plugin_manager));
```

## UI Changes

### Import Button

Added to Skills panel header next to refresh:

```
┌─ Skills ──────────────────────────────────────┐
│  12 skills enabled    [Import +]  [Refresh]   │
```

Dropdown on click:

```
┌──────────────────────┐
│  From GitHub...      │
│  From URL...         │
│  From Local File...  │
└──────────────────────┘
```

### Import Preview Screen

Replaces tab content area during import flow:

```
┌─ Import Plugin ───────────────────────────────┐
│  superpowers v4.3.1                           │
│  by Jesse Vincent                              │
│  "Core skills library for Claude Code..."      │
│                                                │
│  Warning: 3 security warnings (expandable)     │
│                                                │
│  Skills (12):                                  │
│  [x] brainstorming                             │
│  [x] systematic-debugging                      │
│  [x] test-driven-development                   │
│  [x] writing-plans                             │
│  ...                                           │
│                                                │
│  Commands (3):                                 │
│  [x] brainstorm                                │
│  [x] write-plan                                │
│  [x] execute-plan                              │
│                                                │
│  [Cancel]                    [Install Selected]│
└────────────────────────────────────────────────┘
```

### Updated Installed Tab

Grouped by plugin, collapsible:

```
┌─ Installed ───────────────────────────────────┐
│  v superpowers v4.3.1        [Update] [Remove]│
│    brainstorming                    [on/off]  │
│    systematic-debugging             [on/off]  │
│    test-driven-development          [on/off]  │
│    ...8 more                                  │
│                                                │
│  > my-custom-plugin v1.0.0          [Remove]  │
│                                                │
│  -- Individual Skills --                       │
│    code-review (built-in)           [on/off]  │
│    web-search (built-in)            [on/off]  │
└────────────────────────────────────────────────┘
```

### New Actions

```rust
// Import flow
PluginImportOpen
PluginImportFromGitHub(owner_repo: String)
PluginImportFromUrl(url: String)
PluginImportFromLocal(path: String)
PluginImportCancel
PluginImportConfirm
PluginImportToggleSkill(index: usize)

// Plugin management
PluginRemove(plugin_id: String)
PluginUpdate(plugin_id: String)
PluginToggleExpand(plugin_id: String)
```

## Persistence

### Plugin Storage (`~/.hive/plugins.json`)

```json
{
  "plugins": [
    {
      "id": "uuid",
      "name": "superpowers",
      "version": "4.3.1",
      "author": { "name": "Jesse Vincent" },
      "description": "Core skills library...",
      "source": { "type": "github", "owner": "obra", "repo": "superpowers" },
      "installed_at": "2026-02-25T12:00:00Z",
      "skills": [
        {
          "name": "brainstorming",
          "description": "...",
          "instructions": "full markdown body",
          "source_file": "skills/brainstorming/SKILL.md",
          "enabled": true,
          "integrity_hash": "sha256:..."
        }
      ],
      "commands": [
        {
          "name": "brainstorm",
          "description": "...",
          "instructions": "...",
          "source_file": "commands/brainstorm.md"
        }
      ]
    }
  ]
}
```

JSON file chosen over SQLite because plugin count is small (dozens), full instructions must be in memory for dispatch, and it's consistent with other Hive config files.

### Version Check Cache (`~/.hive/plugin_cache.json`)

```json
{
  "last_checked": "2026-02-25T12:00:00Z",
  "versions": {
    "plugin-uuid": {
      "latest_version": "4.4.0",
      "checked_at": "2026-02-25T12:00:00Z"
    }
  }
}
```

Throttled to once per hour to avoid hammering GitHub API.

### Boot Sequence

In `main.rs` `init_services()`:
1. Create `PluginManager` with `reqwest::Client`
2. Load `plugins.json` into `SkillMarketplace.installed_plugins`
3. Register each enabled plugin skill into `SkillsRegistry` for `/command` dispatch
4. Set `AppPluginManager` global

## File Layout

### New Files

```
hive/crates/hive_agents/src/plugin_manager.rs   ~400 lines
hive/crates/hive_agents/src/plugin_types.rs      ~150 lines
```

### Modified Files

```
hive_agents/src/lib.rs                 mod + re-export
hive_agents/src/skill_marketplace.rs   installed_plugins, load/save plugins.json
hive_agents/Cargo.toml                 no new deps needed
hive_ui_core/src/actions.rs            Plugin* actions
hive_ui_core/src/globals.rs            AppPluginManager global
hive_ui_panels/src/panels/skills.rs    import button, dialog, grouped installed view
hive_ui/src/workspace.rs               handle Plugin* actions
hive_app/src/main.rs                   init PluginManager, load plugins, register global
```

### Unchanged

- `skills.rs` — registry stays as-is, plugins call `registry.install()` per skill
- `skill_authoring.rs` — autonomous skill creation is separate
- `config.rs` — no new config fields (plugin data self-contained)
- All other panels

## Dependency Flow

```
plugin_types.rs (pure data, no deps)
       ^
plugin_manager.rs (reqwest, serde_json, sha2)
       ^
skill_marketplace.rs (uses PluginManager for fetch, owns persistence)
       ^
workspace.rs (dispatches actions between UI and marketplace)
       ^
skills.rs panel (renders UI, emits actions)
```
