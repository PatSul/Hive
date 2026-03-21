# OpenSpec Integration — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate OpenSpec's spec-driven development concepts (Change, Delta, Archive, AI-assisted Proposals) into HiveCode's existing `hive_agents` crate so the coordinator can manage structured change workflows.

**Architecture:** Extend the existing `Spec` type with a domain field, then add two new modules: `changes.rs` (Change/Delta/Archive types + ChangeManager) and `spec_store.rs` (file persistence for specs and changes). The `Coordinator` gets a `plan_from_change()` method that uses delta context for better task decomposition. An `generate_proposal()` function uses `AiExecutor` to produce structured proposals from a short user description.

**Tech Stack:** Rust, serde (JSON serialization), chrono (timestamps), uuid (IDs), hive_fs::FileService (file I/O), existing AiExecutor trait.

---

## Chunk 1: Core Types — Delta, Change, Spec Domain Extension

### Task 1: Add `domain` field to Spec

**Files:**
- Modify: `hive/crates/hive_agents/src/specs.rs:87-122`

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module at the bottom of `specs.rs`:

```rust
#[test]
fn spec_domain_defaults_to_none() {
    let spec = Spec::new("id", "Title", "Desc");
    assert!(spec.domain.is_none());
}

#[test]
fn spec_domain_can_be_set() {
    let mut spec = Spec::new("id", "Auth", "Authentication spec");
    spec.domain = Some("auth".into());
    assert_eq!(spec.domain.as_deref(), Some("auth"));
}

#[test]
fn spec_domain_survives_serialization() {
    let mut spec = Spec::new("id", "Auth", "Desc");
    spec.domain = Some("auth".into());
    let json = serde_json::to_string(&spec).unwrap();
    let parsed: Spec = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.domain.as_deref(), Some("auth"));
}

#[test]
fn spec_without_domain_deserializes_from_old_format() {
    // Simulates loading a spec serialized before domain was added.
    let json = r#"{
        "id": "old-id",
        "title": "Old Spec",
        "description": "From before domain existed",
        "status": "draft",
        "sections": {},
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "version": 1,
        "auto_update": true
    }"#;
    let spec: Spec = serde_json::from_str(json).unwrap();
    assert!(spec.domain.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents spec_domain -- --nocapture`
Expected: FAIL — `Spec` has no field `domain`

- [ ] **Step 3: Add the `domain` field to `Spec`**

In `specs.rs`, add the field to the `Spec` struct (after `auto_update`):

```rust
/// Optional domain grouping (e.g. "auth", "billing", "agent").
/// Used to organize specs into `specs/{domain}/` directories.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub domain: Option<String>,
```

And in `Spec::new()`, add `domain: None,` to the struct initializer.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents spec_domain -- --nocapture`
Expected: 4 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/specs.rs
git commit -m "feat(specs): add optional domain field to Spec for OpenSpec grouping"
```

---

### Task 2: DeltaKind and DeltaEntry types

**Files:**
- Create: `hive/crates/hive_agents/src/changes.rs`

- [ ] **Step 1: Write the failing test**

Create `hive/crates/hive_agents/src/changes.rs` with the test module only (types not yet defined):

```rust
//! OpenSpec-inspired Change management — structured change proposals with
//! delta-based spec modifications and archive-with-merge workflows.

use serde::{Deserialize, Serialize};

// Types will go here.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_kind_variants() {
        let added = DeltaKind::Added;
        let modified = DeltaKind::Modified;
        let removed = DeltaKind::Removed;
        // Verify serialization round-trips.
        let json = serde_json::to_string(&added).unwrap();
        assert_eq!(json, r#""added""#);
        let json = serde_json::to_string(&modified).unwrap();
        assert_eq!(json, r#""modified""#);
        let json = serde_json::to_string(&removed).unwrap();
        assert_eq!(json, r#""removed""#);
    }

    #[test]
    fn delta_entry_new() {
        let entry = DeltaEntry::new("d1", DeltaKind::Added, "Requirements", "OAuth support", "MUST support OAuth 2.0 flows");
        assert_eq!(entry.id, "d1");
        assert!(matches!(entry.kind, DeltaKind::Added));
        assert_eq!(entry.section, "Requirements");
        assert_eq!(entry.title, "OAuth support");
        assert_eq!(entry.content, "MUST support OAuth 2.0 flows");
    }

    #[test]
    fn delta_entry_serialization_roundtrip() {
        let entry = DeltaEntry::new("d1", DeltaKind::Modified, "Plan", "Step 2", "Updated approach");
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: DeltaEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "d1");
        assert!(matches!(parsed.kind, DeltaKind::Modified));
        assert_eq!(parsed.title, "Step 2");
    }
}
```

- [ ] **Step 2: Add `pub mod changes;` to `lib.rs`**

In `hive/crates/hive_agents/src/lib.rs`, add after the `pub mod specs;` line:

```rust
pub mod changes;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p hive_agents delta_kind -- --nocapture`
Expected: FAIL — types `DeltaKind` and `DeltaEntry` not found

- [ ] **Step 4: Implement DeltaKind and DeltaEntry**

Add above the `#[cfg(test)]` block in `changes.rs`:

```rust
// ---------------------------------------------------------------------------
// Delta — what's changing in a spec
// ---------------------------------------------------------------------------

/// The kind of change a delta entry represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaKind {
    /// New behavior being added.
    Added,
    /// Existing behavior being modified.
    Modified,
    /// Existing behavior being removed.
    Removed,
}

/// A single delta entry within a Change — one atomic spec modification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaEntry {
    pub id: String,
    pub kind: DeltaKind,
    /// Which spec section this delta targets (e.g. "Requirements", "Plan").
    pub section: String,
    pub title: String,
    pub content: String,
}

impl DeltaEntry {
    pub fn new(
        id: impl Into<String>,
        kind: DeltaKind,
        section: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            section: section.into(),
            title: title.into(),
            content: content.into(),
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p hive_agents delta_ -- --nocapture`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/changes.rs hive/crates/hive_agents/src/lib.rs
git commit -m "feat(changes): add DeltaKind and DeltaEntry types for OpenSpec deltas"
```

---

### Task 3: Change, ChangeStatus, and ChangeProposal types

**Files:**
- Modify: `hive/crates/hive_agents/src/changes.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `changes.rs`:

```rust
#[test]
fn change_status_serialization() {
    let proposed = ChangeStatus::Proposed;
    let json = serde_json::to_string(&proposed).unwrap();
    assert_eq!(json, r#""proposed""#);

    let archived = ChangeStatus::Archived;
    let json = serde_json::to_string(&archived).unwrap();
    assert_eq!(json, r#""archived""#);
}

#[test]
fn change_proposal_new() {
    let proposal = ChangeProposal::new("Add OAuth", "No auth exists");
    assert_eq!(proposal.intent, "Add OAuth");
    assert_eq!(proposal.problems_solved, "No auth exists");
    assert!(proposal.scope_description.is_empty());
    assert!(proposal.design_notes.is_empty());
}

#[test]
fn change_new_defaults() {
    let change = Change::new("add-oauth", "auth-spec-1");
    assert_eq!(change.name, "add-oauth");
    assert_eq!(change.spec_id, "auth-spec-1");
    assert!(matches!(change.status, ChangeStatus::Proposed));
    assert!(change.proposal.is_none());
    assert!(change.delta_entries.is_empty());
    assert!(change.scope_paths.is_empty());
    assert_eq!(change.version, 1);
}

#[test]
fn change_serialization_roundtrip() {
    let mut change = Change::new("add-oauth", "spec-1");
    change.scope_paths = vec!["hive_ai/".into(), "hive_agents/".into()];
    change.proposal = Some(ChangeProposal {
        intent: "Add OAuth".into(),
        problems_solved: "No auth".into(),
        scope_description: "AI and agent crates".into(),
        design_notes: "Use existing persona pattern".into(),
    });
    change.delta_entries.push(DeltaEntry::new(
        "d1", DeltaKind::Added, "Requirements", "OAuth", "MUST support OAuth",
    ));

    let json = serde_json::to_string(&change).unwrap();
    let parsed: Change = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "add-oauth");
    assert_eq!(parsed.scope_paths.len(), 2);
    assert!(parsed.proposal.is_some());
    assert_eq!(parsed.delta_entries.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents change_ -- --nocapture`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement ChangeStatus, ChangeProposal, and Change**

Add above the `#[cfg(test)]` block in `changes.rs` (after `DeltaEntry`):

```rust
// ---------------------------------------------------------------------------
// Change Status
// ---------------------------------------------------------------------------

/// Lifecycle status of a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    /// Change proposed but not yet approved.
    Proposed,
    /// Change approved and ready for implementation.
    Approved,
    /// Change is being implemented.
    InProgress,
    /// Implementation complete, ready for archive.
    Completed,
    /// Archived — deltas merged into parent spec.
    Archived,
}

// ---------------------------------------------------------------------------
// Change Proposal
// ---------------------------------------------------------------------------

/// Structured proposal describing intent and scope of a change.
/// Can be human-authored or AI-generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeProposal {
    /// What this change intends to accomplish.
    pub intent: String,
    /// What problems this change solves.
    pub problems_solved: String,
    /// Description of the scope (which parts of the system are affected).
    pub scope_description: String,
    /// Technical design notes and approach.
    pub design_notes: String,
}

impl ChangeProposal {
    pub fn new(intent: impl Into<String>, problems_solved: impl Into<String>) -> Self {
        Self {
            intent: intent.into(),
            problems_solved: problems_solved.into(),
            scope_description: String::new(),
            design_notes: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Change
// ---------------------------------------------------------------------------

/// A proposed modification to a spec, containing delta entries that describe
/// what's being added, modified, or removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    /// Kebab-case name (e.g. "add-oauth-support").
    pub name: String,
    pub status: ChangeStatus,
    /// The spec this change targets.
    pub spec_id: String,
    /// AI-generated or human-authored proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<ChangeProposal>,
    /// Delta entries describing spec modifications.
    pub delta_entries: Vec<DeltaEntry>,
    /// File/directory paths this change affects.
    #[serde(default)]
    pub scope_paths: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub version: u32,
}

impl Change {
    pub fn new(name: impl Into<String>, spec_id: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            status: ChangeStatus::Proposed,
            spec_id: spec_id.into(),
            proposal: None,
            delta_entries: Vec::new(),
            scope_paths: Vec::new(),
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }

    /// Bump version and update timestamp.
    fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = chrono::Utc::now();
    }

    /// Count of delta entries by kind.
    pub fn delta_count(&self, kind: DeltaKind) -> usize {
        self.delta_entries.iter().filter(|d| d.kind == kind).count()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents change_ -- --nocapture`
Expected: 4 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/changes.rs
git commit -m "feat(changes): add Change, ChangeStatus, ChangeProposal types"
```

---

## Chunk 2: ChangeManager — CRUD, Deltas, and Archive

### Task 4: ChangeManager with CRUD operations

**Files:**
- Modify: `hive/crates/hive_agents/src/changes.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `changes.rs`:

```rust
fn make_manager_with_change() -> (ChangeManager, String) {
    let mut mgr = ChangeManager::new();
    let id = mgr.create_change("add-oauth", "spec-1");
    (mgr, id)
}

#[test]
fn create_change_returns_unique_ids() {
    let mut mgr = ChangeManager::new();
    let id1 = mgr.create_change("change-a", "spec-1");
    let id2 = mgr.create_change("change-b", "spec-1");
    assert_ne!(id1, id2);
    assert_eq!(mgr.changes.len(), 2);
}

#[test]
fn get_change_returns_correct_data() {
    let (mgr, id) = make_manager_with_change();
    let change = mgr.get_change(&id).unwrap();
    assert_eq!(change.name, "add-oauth");
    assert_eq!(change.spec_id, "spec-1");
    assert!(matches!(change.status, ChangeStatus::Proposed));
}

#[test]
fn get_change_not_found() {
    let mgr = ChangeManager::new();
    assert!(mgr.get_change("nonexistent").is_none());
}

#[test]
fn set_status_transitions() {
    let (mut mgr, id) = make_manager_with_change();
    mgr.set_status(&id, ChangeStatus::Approved).unwrap();
    assert!(matches!(mgr.get_change(&id).unwrap().status, ChangeStatus::Approved));

    mgr.set_status(&id, ChangeStatus::InProgress).unwrap();
    assert!(matches!(mgr.get_change(&id).unwrap().status, ChangeStatus::InProgress));
}

#[test]
fn set_status_not_found() {
    let mut mgr = ChangeManager::new();
    assert!(mgr.set_status("missing", ChangeStatus::Approved).is_err());
}

#[test]
fn set_proposal() {
    let (mut mgr, id) = make_manager_with_change();
    let proposal = ChangeProposal::new("Add OAuth", "No auth");
    mgr.set_proposal(&id, proposal).unwrap();
    let change = mgr.get_change(&id).unwrap();
    assert!(change.proposal.is_some());
    assert_eq!(change.proposal.as_ref().unwrap().intent, "Add OAuth");
    assert_eq!(change.version, 2);
}

#[test]
fn set_scope_paths() {
    let (mut mgr, id) = make_manager_with_change();
    mgr.set_scope_paths(&id, vec!["src/auth/".into(), "src/api/".into()]).unwrap();
    let change = mgr.get_change(&id).unwrap();
    assert_eq!(change.scope_paths.len(), 2);
    assert_eq!(change.version, 2);
}

#[test]
fn get_active_changes() {
    let mut mgr = ChangeManager::new();
    let id1 = mgr.create_change("c1", "spec-1");
    let _id2 = mgr.create_change("c2", "spec-1");
    mgr.set_status(&id1, ChangeStatus::Archived).unwrap();
    let active = mgr.get_active_changes();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "c2");
}

#[test]
fn get_changes_for_spec() {
    let mut mgr = ChangeManager::new();
    mgr.create_change("c1", "spec-A");
    mgr.create_change("c2", "spec-B");
    mgr.create_change("c3", "spec-A");
    let for_a = mgr.get_changes_for_spec("spec-A");
    assert_eq!(for_a.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents create_change -- --nocapture`
Expected: FAIL — `ChangeManager` not defined

- [ ] **Step 3: Implement ChangeManager with CRUD**

Add above the `#[cfg(test)]` block in `changes.rs`:

```rust
// ---------------------------------------------------------------------------
// Change Manager
// ---------------------------------------------------------------------------

/// Archived change record — stores the pre-merge snapshot alongside metadata.
/// Defined here so `ChangeManager` can reference it; populated by `archive_change` (Task 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeArchive {
    pub change_id: String,
    pub change_name: String,
    pub spec_id: String,
    pub archived_at: chrono::DateTime<chrono::Utc>,
    /// Snapshot of the spec *before* the delta merge was applied.
    pub pre_merge_snapshot: String,
    /// The delta entries that were merged.
    pub merged_deltas: Vec<DeltaEntry>,
    /// The full proposal at time of archive.
    pub proposal: Option<ChangeProposal>,
}

/// Manages a collection of changes with CRUD, delta operations, and archiving.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangeManager {
    pub changes: std::collections::HashMap<String, Change>,
    pub archives: Vec<ChangeArchive>,
}

impl ChangeManager {
    pub fn new() -> Self {
        Self {
            changes: std::collections::HashMap::new(),
            archives: Vec::new(),
        }
    }

    /// Create a new change targeting a spec. Returns the change ID.
    pub fn create_change(
        &mut self,
        name: impl Into<String>,
        spec_id: impl Into<String>,
    ) -> String {
        let change = Change::new(name, spec_id);
        let id = change.id.clone();
        self.changes.insert(id.clone(), change);
        id
    }

    /// Get an immutable reference to a change by ID.
    pub fn get_change(&self, id: &str) -> Option<&Change> {
        self.changes.get(id)
    }

    /// Get a mutable reference to a change by ID.
    pub fn get_change_mut(&mut self, id: &str) -> Option<&mut Change> {
        self.changes.get_mut(id)
    }

    /// Update the status of a change.
    pub fn set_status(&mut self, id: &str, status: ChangeStatus) -> Result<(), String> {
        let change = self.changes.get_mut(id)
            .ok_or_else(|| format!("Change not found: {id}"))?;
        change.status = status;
        change.bump_version();
        Ok(())
    }

    /// Set the proposal on a change.
    pub fn set_proposal(&mut self, id: &str, proposal: ChangeProposal) -> Result<(), String> {
        let change = self.changes.get_mut(id)
            .ok_or_else(|| format!("Change not found: {id}"))?;
        change.proposal = Some(proposal);
        change.bump_version();
        Ok(())
    }

    /// Set the scope paths on a change.
    pub fn set_scope_paths(&mut self, id: &str, paths: Vec<String>) -> Result<(), String> {
        let change = self.changes.get_mut(id)
            .ok_or_else(|| format!("Change not found: {id}"))?;
        change.scope_paths = paths;
        change.bump_version();
        Ok(())
    }

    /// Return all non-archived changes.
    pub fn get_active_changes(&self) -> Vec<&Change> {
        self.changes.values()
            .filter(|c| !matches!(c.status, ChangeStatus::Archived))
            .collect()
    }

    /// Return all changes targeting a specific spec.
    pub fn get_changes_for_spec(&self, spec_id: &str) -> Vec<&Change> {
        self.changes.values()
            .filter(|c| c.spec_id == spec_id)
            .collect()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents -- create_change get_change set_status set_proposal set_scope get_active get_changes_for --nocapture`
Expected: All 9 CRUD tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/changes.rs
git commit -m "feat(changes): add ChangeManager with CRUD operations"
```

---

### Task 5: ChangeManager delta operations

**Files:**
- Modify: `hive/crates/hive_agents/src/changes.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
#[test]
fn add_delta_to_change() {
    let (mut mgr, id) = make_manager_with_change();
    let delta = DeltaEntry::new("d1", DeltaKind::Added, "Requirements", "OAuth", "MUST support OAuth");
    mgr.add_delta(&id, delta).unwrap();

    let change = mgr.get_change(&id).unwrap();
    assert_eq!(change.delta_entries.len(), 1);
    assert_eq!(change.delta_count(DeltaKind::Added), 1);
    assert_eq!(change.version, 2);
}

#[test]
fn add_multiple_deltas() {
    let (mut mgr, id) = make_manager_with_change();
    mgr.add_delta(&id, DeltaEntry::new("d1", DeltaKind::Added, "Requirements", "A", "a")).unwrap();
    mgr.add_delta(&id, DeltaEntry::new("d2", DeltaKind::Modified, "Plan", "B", "b")).unwrap();
    mgr.add_delta(&id, DeltaEntry::new("d3", DeltaKind::Removed, "Notes", "C", "c")).unwrap();

    let change = mgr.get_change(&id).unwrap();
    assert_eq!(change.delta_entries.len(), 3);
    assert_eq!(change.delta_count(DeltaKind::Added), 1);
    assert_eq!(change.delta_count(DeltaKind::Modified), 1);
    assert_eq!(change.delta_count(DeltaKind::Removed), 1);
}

#[test]
fn add_delta_not_found() {
    let mut mgr = ChangeManager::new();
    let delta = DeltaEntry::new("d1", DeltaKind::Added, "Requirements", "X", "x");
    assert!(mgr.add_delta("missing", delta).is_err());
}

#[test]
fn remove_delta_from_change() {
    let (mut mgr, id) = make_manager_with_change();
    mgr.add_delta(&id, DeltaEntry::new("d1", DeltaKind::Added, "Requirements", "A", "a")).unwrap();
    mgr.add_delta(&id, DeltaEntry::new("d2", DeltaKind::Modified, "Plan", "B", "b")).unwrap();

    mgr.remove_delta(&id, "d1").unwrap();
    let change = mgr.get_change(&id).unwrap();
    assert_eq!(change.delta_entries.len(), 1);
    assert_eq!(change.delta_entries[0].id, "d2");
}

#[test]
fn remove_delta_not_found() {
    let (mut mgr, id) = make_manager_with_change();
    assert!(mgr.remove_delta(&id, "nonexistent").is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents add_delta remove_delta -- --nocapture`
Expected: FAIL — methods not defined

- [ ] **Step 3: Implement delta operations on ChangeManager**

Add to the `impl ChangeManager` block:

```rust
    /// Add a delta entry to a change.
    pub fn add_delta(&mut self, change_id: &str, delta: DeltaEntry) -> Result<(), String> {
        let change = self.changes.get_mut(change_id)
            .ok_or_else(|| format!("Change not found: {change_id}"))?;
        change.delta_entries.push(delta);
        change.bump_version();
        Ok(())
    }

    /// Remove a delta entry from a change by delta ID.
    pub fn remove_delta(&mut self, change_id: &str, delta_id: &str) -> Result<(), String> {
        let change = self.changes.get_mut(change_id)
            .ok_or_else(|| format!("Change not found: {change_id}"))?;
        let before = change.delta_entries.len();
        change.delta_entries.retain(|d| d.id != delta_id);
        if change.delta_entries.len() == before {
            return Err(format!("Delta not found: {delta_id}"));
        }
        change.bump_version();
        Ok(())
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents add_delta remove_delta -- --nocapture`
Expected: 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/changes.rs
git commit -m "feat(changes): add delta add/remove operations to ChangeManager"
```

---

### Task 6: ChangeManager archive with snapshot + merge

**Files:**
- Modify: `hive/crates/hive_agents/src/changes.rs`

This implements decision C: snapshot the pre-merge spec state, merge ADDED/MODIFIED deltas into the spec, hard-delete entries targeted by REMOVED deltas, then move the change to the archive.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
use crate::specs::{Spec, SpecEntry, SpecManager, SpecSection};

#[test]
fn archive_change_merges_added_deltas() {
    let mut spec_mgr = SpecManager::new();
    let spec_id = spec_mgr.create_spec("Auth Spec", "Authentication");

    let mut change_mgr = ChangeManager::new();
    let change_id = change_mgr.create_change("add-oauth", &spec_id);
    change_mgr.add_delta(&change_id, DeltaEntry::new(
        "d1", DeltaKind::Added, "Requirements", "OAuth", "MUST support OAuth 2.0",
    )).unwrap();

    change_mgr.archive_change(&change_id, &mut spec_mgr).unwrap();

    // Spec should now have the new requirement.
    let spec = spec_mgr.get_spec(&spec_id).unwrap();
    let reqs = spec.sections.get(&SpecSection::Requirements).unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].title, "OAuth");

    // Change should be archived.
    assert!(change_mgr.get_change(&change_id).is_none());
    assert_eq!(change_mgr.archives.len(), 1);
    assert_eq!(change_mgr.archives[0].change_name, "add-oauth");
}

#[test]
fn archive_change_removes_entries_from_spec() {
    let mut spec_mgr = SpecManager::new();
    let spec_id = spec_mgr.create_spec("Auth Spec", "Authentication");
    spec_mgr.add_entry(&spec_id, SpecSection::Requirements,
        SpecEntry::new("old-1", "Basic Auth", "MUST support basic auth")).unwrap();

    let mut change_mgr = ChangeManager::new();
    let change_id = change_mgr.create_change("remove-basic", &spec_id);
    change_mgr.add_delta(&change_id, DeltaEntry::new(
        "d1", DeltaKind::Removed, "Requirements", "Basic Auth", "Deprecated",
    )).unwrap();

    change_mgr.archive_change(&change_id, &mut spec_mgr).unwrap();

    let spec = spec_mgr.get_spec(&spec_id).unwrap();
    let reqs = spec.sections.get(&SpecSection::Requirements).unwrap();
    assert!(reqs.is_empty());
}

#[test]
fn archive_change_stores_pre_merge_snapshot() {
    let mut spec_mgr = SpecManager::new();
    let spec_id = spec_mgr.create_spec("Auth Spec", "Authentication");
    spec_mgr.add_entry(&spec_id, SpecSection::Requirements,
        SpecEntry::new("r1", "Existing", "Already here")).unwrap();

    let mut change_mgr = ChangeManager::new();
    let change_id = change_mgr.create_change("modify-existing", &spec_id);
    change_mgr.add_delta(&change_id, DeltaEntry::new(
        "d1", DeltaKind::Added, "Requirements", "New", "Added after",
    )).unwrap();

    change_mgr.archive_change(&change_id, &mut spec_mgr).unwrap();

    // The archive should contain a snapshot of the spec BEFORE the merge.
    let archive = &change_mgr.archives[0];
    assert!(archive.pre_merge_snapshot.contains("Existing"));
    assert!(!archive.pre_merge_snapshot.contains("New"));
}

#[test]
fn archive_change_not_found() {
    let mut spec_mgr = SpecManager::new();
    let mut change_mgr = ChangeManager::new();
    assert!(change_mgr.archive_change("missing", &mut spec_mgr).is_err());
}

#[test]
fn archive_change_spec_not_found() {
    let mut spec_mgr = SpecManager::new();
    let mut change_mgr = ChangeManager::new();
    let change_id = change_mgr.create_change("orphan", "nonexistent-spec");
    assert!(change_mgr.archive_change(&change_id, &mut spec_mgr).is_err());
}

#[test]
fn archive_preserves_merged_deltas_and_proposal() {
    let mut spec_mgr = SpecManager::new();
    let spec_id = spec_mgr.create_spec("Spec", "Test");

    let mut change_mgr = ChangeManager::new();
    let change_id = change_mgr.create_change("with-proposal", &spec_id);
    change_mgr.set_proposal(&change_id, ChangeProposal::new("Intent", "Problems")).unwrap();
    change_mgr.add_delta(&change_id, DeltaEntry::new(
        "d1", DeltaKind::Added, "Plan", "Step 1", "Do the thing",
    )).unwrap();

    change_mgr.archive_change(&change_id, &mut spec_mgr).unwrap();

    let archive = &change_mgr.archives[0];
    assert_eq!(archive.merged_deltas.len(), 1);
    assert!(archive.proposal.is_some());
    assert_eq!(archive.proposal.as_ref().unwrap().intent, "Intent");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents archive_change -- --nocapture`
Expected: FAIL — `archive_change` method not defined

- [ ] **Step 3: Implement archive_change**

Add to the `impl ChangeManager` block:

```rust
    /// Archive a change: snapshot the pre-merge spec state, merge deltas into
    /// the spec, hard-delete REMOVED entries, and move the change to archives.
    pub fn archive_change(
        &mut self,
        change_id: &str,
        spec_mgr: &mut crate::specs::SpecManager,
    ) -> Result<(), String> {
        // Validate the change exists.
        let change = self.changes.get(change_id)
            .ok_or_else(|| format!("Change not found: {change_id}"))?;

        let spec_id = change.spec_id.clone();

        // Validate the target spec exists.
        let spec = spec_mgr.get_spec(&spec_id)
            .ok_or_else(|| format!("Target spec not found: {spec_id}"))?;

        // 1. Snapshot the spec BEFORE merging.
        let pre_merge_snapshot = spec_mgr.export_markdown(&spec_id)
            .unwrap_or_else(|_| String::from("[snapshot failed]"));

        // Clone change data before we remove it from the map.
        let change = self.changes.remove(change_id)
            .ok_or_else(|| format!("Change not found: {change_id}"))?;

        // 2. Merge deltas into the spec.
        let spec = spec_mgr.get_spec_mut(&spec_id)
            .ok_or_else(|| format!("Target spec not found: {spec_id}"))?;

        for delta in &change.delta_entries {
            let section = match_spec_section(&delta.section);

            match delta.kind {
                DeltaKind::Added => {
                    let entry = crate::specs::SpecEntry::new(
                        &delta.id,
                        &delta.title,
                        &delta.content,
                    );
                    spec.sections.entry(section).or_default().push(entry);
                }
                DeltaKind::Modified => {
                    // Find existing entry by title and update content.
                    let entries = spec.sections.entry(section).or_default();
                    if let Some(existing) = entries.iter_mut().find(|e| e.title == delta.title) {
                        existing.content = delta.content.clone();
                    } else {
                        // If not found, treat as an add.
                        let entry = crate::specs::SpecEntry::new(
                            &delta.id,
                            &delta.title,
                            &delta.content,
                        );
                        entries.push(entry);
                    }
                }
                DeltaKind::Removed => {
                    // Hard-delete entries matching the title.
                    let entries = spec.sections.entry(section).or_default();
                    entries.retain(|e| e.title != delta.title);
                }
            }
        }

        // Bump spec version after merge.
        spec.version += 1;
        spec.updated_at = chrono::Utc::now();

        // 3. Create archive record.
        let archive = ChangeArchive {
            change_id: change.id.clone(),
            change_name: change.name.clone(),
            spec_id: change.spec_id.clone(),
            archived_at: chrono::Utc::now(),
            pre_merge_snapshot,
            merged_deltas: change.delta_entries.clone(),
            proposal: change.proposal.clone(),
        };
        self.archives.push(archive);

        Ok(())
    }
```

Also add the section-matching helper function (outside the impl block):

```rust
// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Match a free-form section name string to a SpecSection enum variant.
fn match_spec_section(s: &str) -> crate::specs::SpecSection {
    match s.to_lowercase().as_str() {
        "requirements" | "requirement" => crate::specs::SpecSection::Requirements,
        "plan" | "plans" => crate::specs::SpecSection::Plan,
        "progress" => crate::specs::SpecSection::Progress,
        "notes" | "note" => crate::specs::SpecSection::Notes,
        _ => crate::specs::SpecSection::Notes, // default fallback
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents archive_change -- --nocapture`
Expected: 6 tests PASS

- [ ] **Step 5: Run all changes.rs tests together**

Run: `cargo test -p hive_agents changes -- --nocapture`
Expected: All tests in the module PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/changes.rs
git commit -m "feat(changes): archive_change with snapshot + merge + hard delete"
```

---

## Chunk 3: File Persistence, AI Proposals, and Coordinator Integration

### Task 7: SpecStore — file-based persistence for specs and changes

**Files:**
- Create: `hive/crates/hive_agents/src/spec_store.rs`
- Modify: `hive/crates/hive_agents/src/lib.rs`

The SpecStore writes specs to `{base}/specs/{domain}/{name}.json` and changes to `{base}/changes/active/{name}/change.json`. It uses `hive_fs::FileService` for validated file I/O.

- [ ] **Step 1: Write the failing tests**

Create `hive/crates/hive_agents/src/spec_store.rs`:

```rust
//! File-based persistence for specs and changes.
//!
//! Writes specs alongside code in `specs/{domain}/` directories and changes
//! in `changes/active/{name}/` directories, following the OpenSpec convention.

use std::path::{Path, PathBuf};

use crate::changes::{Change, ChangeManager};
use crate::specs::{Spec, SpecManager};

// Implementation will go here.

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
```

- [ ] **Step 2: Add `pub mod spec_store;` to `lib.rs`**

In `hive/crates/hive_agents/src/lib.rs`, add after the `pub mod changes;` line:

```rust
pub mod spec_store;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p hive_agents spec_path change_path archive_path save_and_load list_specs -- --nocapture`
Expected: FAIL — functions not defined

- [ ] **Step 4: Implement SpecStore functions**

Add above the `#[cfg(test)]` block in `spec_store.rs`:

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p hive_agents spec_store -- --nocapture`
Expected: All 7 tests PASS

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_agents/src/spec_store.rs hive/crates/hive_agents/src/lib.rs
git commit -m "feat(spec_store): file-based persistence for specs and changes"
```

---

### Task 8: AI-assisted proposal generation

**Files:**
- Modify: `hive/crates/hive_agents/src/changes.rs`

This adds a `generate_proposal()` async function that uses `AiExecutor` to produce a structured `ChangeProposal` from a short user description plus scope paths.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `changes.rs`:

```rust
use crate::hivemind::AiExecutor;
use hive_ai::types::{ChatRequest, ChatResponse, FinishReason, TokenUsage};

struct MockExecutor {
    response: String,
}

impl MockExecutor {
    fn with_proposal_response() -> Self {
        Self {
            response: r#"{
                "intent": "Add OAuth 2.0 authentication support",
                "problems_solved": "No standardized auth flow exists",
                "scope_description": "Auth module and API endpoints",
                "design_notes": "Use existing persona pattern for role-based access"
            }"#.into(),
        }
    }
}

impl AiExecutor for MockExecutor {
    async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
        Ok(ChatResponse {
            content: self.response.clone(),
            model: "mock".into(),
            usage: TokenUsage {
                prompt_tokens: 50,
                completion_tokens: 100,
                total_tokens: 150,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
            finish_reason: FinishReason::Stop,
            thinking: None,
            tool_calls: None,
        })
    }
}

#[tokio::test]
async fn generate_proposal_from_description() {
    let executor = MockExecutor::with_proposal_response();
    let proposal = generate_proposal(
        &executor,
        "Add OAuth support to the auth module",
        &["src/auth/".into(), "src/api/".into()],
        "claude-sonnet-4-5-20250929",
    ).await.unwrap();

    assert_eq!(proposal.intent, "Add OAuth 2.0 authentication support");
    assert!(!proposal.problems_solved.is_empty());
    assert!(!proposal.scope_description.is_empty());
    assert!(!proposal.design_notes.is_empty());
}

#[tokio::test]
async fn generate_proposal_handles_invalid_json() {
    let executor = MockExecutor { response: "not json".into() };
    let result = generate_proposal(
        &executor,
        "Do something",
        &[],
        "model",
    ).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents generate_proposal -- --nocapture`
Expected: FAIL — function not defined

- [ ] **Step 3: Implement generate_proposal**

Add above the `#[cfg(test)]` block in `changes.rs`:

```rust
use hive_ai::types::{ChatMessage, ChatRequest, MessageRole};

// ---------------------------------------------------------------------------
// AI-Assisted Proposal Generation
// ---------------------------------------------------------------------------

/// Use an AI executor to generate a structured change proposal from a
/// short user description and scope paths.
pub async fn generate_proposal<E: crate::hivemind::AiExecutor>(
    executor: &E,
    description: &str,
    scope_paths: &[String],
    model: &str,
) -> Result<ChangeProposal, String> {
    let scope_str = if scope_paths.is_empty() {
        "Not specified".to_string()
    } else {
        scope_paths.join(", ")
    };

    let prompt = format!(
        "Generate a structured change proposal for the following request.\n\n\
         Description: {description}\n\
         Scope paths: {scope_str}\n\n\
         Return ONLY a JSON object with these fields:\n\
         - \"intent\": What this change intends to accomplish (1-2 sentences)\n\
         - \"problems_solved\": What problems this change solves (1-2 sentences)\n\
         - \"scope_description\": Which parts of the system are affected (1-2 sentences)\n\
         - \"design_notes\": Suggested technical approach (2-3 sentences)"
    );

    let request = ChatRequest {
        messages: vec![ChatMessage::text(MessageRole::User, prompt)],
        model: model.to_string(),
        max_tokens: 1024,
        temperature: Some(0.3),
        system_prompt: Some(
            "You are a software architect. Return valid JSON only, no markdown fences.".into(),
        ),
        tools: None,
        cache_system_prompt: false,
    };

    let response = executor.execute(&request).await?;
    parse_proposal(&response.content)
}

/// Parse the AI response into a ChangeProposal.
fn parse_proposal(response: &str) -> Result<ChangeProposal, String> {
    let content = response
        .trim()
        .strip_prefix("```json")
        .or_else(|| response.trim().strip_prefix("```"))
        .unwrap_or(response.trim());
    let content = content.strip_suffix("```").unwrap_or(content).trim();

    serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse proposal: {e}"))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents generate_proposal -- --nocapture`
Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/changes.rs
git commit -m "feat(changes): AI-assisted proposal generation via AiExecutor"
```

---

### Task 9: Coordinator::plan_from_change()

**Files:**
- Modify: `hive/crates/hive_agents/src/coordinator.rs`

Adds a `plan_from_change()` method that uses the change's deltas, proposal, and scope paths to give the AI better context for task decomposition.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `coordinator.rs`:

```rust
use crate::changes::{Change, ChangeProposal, DeltaEntry, DeltaKind};

#[tokio::test]
async fn plan_from_change_uses_delta_context() {
    let json_response = r#"[
        {"id": "t1", "description": "Implement OAuth handler", "persona": "implement", "dependencies": [], "priority": 1}
    ]"#;
    let executor = MockExecutor::new(json_response);
    let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);

    let mut change = Change::new("add-oauth", "spec-1");
    change.proposal = Some(ChangeProposal {
        intent: "Add OAuth".into(),
        problems_solved: "No auth".into(),
        scope_description: "Auth module".into(),
        design_notes: "Use persona pattern".into(),
    });
    change.scope_paths = vec!["src/auth/".into()];
    change.delta_entries.push(DeltaEntry::new(
        "d1", DeltaKind::Added, "Requirements", "OAuth", "MUST support OAuth",
    ));

    let plan = coordinator.plan_from_change(&change).await.unwrap();
    assert_eq!(plan.tasks.len(), 1);
    assert_eq!(plan.tasks[0].description, "Implement OAuth handler");
}

#[tokio::test]
async fn plan_from_change_handles_ai_failure() {
    let executor = MockExecutor::failing();
    let coordinator = Coordinator::new(CoordinatorConfig::default(), executor);
    let change = Change::new("will-fail", "spec-1");
    let result = coordinator.plan_from_change(&change).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hive_agents plan_from_change -- --nocapture`
Expected: FAIL — method not defined

- [ ] **Step 3: Implement plan_from_change**

Add to the `impl<E: AiExecutor + 'static> Coordinator<E>` block in `coordinator.rs`, after `plan_from_spec`:

```rust
    /// Use AI to decompose a Change (with deltas and proposal context) into
    /// a task plan. Provides richer context than `plan_from_spec` because it
    /// includes the structured proposal, delta entries, and scope paths.
    pub async fn plan_from_change(
        &self,
        change: &crate::changes::Change,
    ) -> Result<TaskPlan, String> {
        let proposal_ctx = match &change.proposal {
            Some(p) => format!(
                "Intent: {}\nProblems solved: {}\nScope: {}\nDesign: {}",
                p.intent, p.problems_solved, p.scope_description, p.design_notes
            ),
            None => "No proposal provided.".to_string(),
        };

        let deltas_ctx: String = change.delta_entries.iter().map(|d| {
            format!("  [{:?}] {}: {} — {}", d.kind, d.section, d.title, d.content)
        }).collect::<Vec<_>>().join("\n");

        let scope_ctx = if change.scope_paths.is_empty() {
            "Not specified".to_string()
        } else {
            change.scope_paths.join(", ")
        };

        let prompt = format!(
            "Decompose the following change into concrete tasks.\n\n\
             Change: {}\n\
             Proposal:\n{proposal_ctx}\n\n\
             Delta entries (what's changing):\n{deltas_ctx}\n\n\
             Scope paths: {scope_ctx}\n\n\
             For each task, specify:\n\
             - A short ID (e.g. task-1, task-2)\n\
             - A description\n\
             - Which persona should handle it (investigate, implement, verify, critique, debug, code_review)\n\
             - Dependencies (other task IDs that must complete first)\n\
             - Priority (1=highest, 5=lowest)\n\n\
             Return ONLY a JSON array of objects with fields: id, description, persona, dependencies, priority.",
            change.name
        );

        let request = ChatRequest {
            messages: vec![ChatMessage::text(MessageRole::User, prompt)],
            model: self.config.model_for_coordination.clone(),
            max_tokens: 4096,
            temperature: Some(0.2),
            system_prompt: Some(
                "You are a project planning assistant. Return valid JSON only.".into(),
            ),
            tools: None,
            cache_system_prompt: false,
        };

        let response = self.executor.execute(&request).await?;
        parse_task_plan(&response.content)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hive_agents plan_from_change -- --nocapture`
Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/coordinator.rs
git commit -m "feat(coordinator): plan_from_change for OpenSpec-style task decomposition"
```

---

## Chunk 4: Export Wiring and Final Integration

### Task 10: Wire up all exports in lib.rs

**Files:**
- Modify: `hive/crates/hive_agents/src/lib.rs`

- [ ] **Step 1: Add exports for new types**

In `lib.rs`, add the following `pub use` statements (after the existing `pub use specs::...` line):

```rust
pub use changes::{
    Change, ChangeArchive, ChangeManager, ChangeProposal, ChangeStatus,
    DeltaEntry, DeltaKind, generate_proposal,
};
pub use spec_store::{
    save_spec, load_spec, save_change, load_change, list_specs,
    spec_file_path, change_dir_path, archive_dir_path,
};
```

- [ ] **Step 2: Verify the full crate compiles**

Run: `cargo build -p hive_agents`
Expected: Compiles with no errors

- [ ] **Step 3: Run all tests in the crate**

Run: `cargo test -p hive_agents -- --nocapture`
Expected: All existing tests + all new tests PASS (0 failures)

- [ ] **Step 4: Commit**

```bash
git add hive/crates/hive_agents/src/lib.rs
git commit -m "feat(lib): export OpenSpec types — Change, Delta, SpecStore, generate_proposal"
```

---

## Summary of Changes

| File | Action | What |
|------|--------|------|
| `hive/crates/hive_agents/src/specs.rs` | Modify | Add `domain: Option<String>` field |
| `hive/crates/hive_agents/src/changes.rs` | Create | DeltaKind, DeltaEntry, Change, ChangeStatus, ChangeProposal, ChangeArchive, ChangeManager, `generate_proposal()` |
| `hive/crates/hive_agents/src/spec_store.rs` | Create | `save_spec`, `load_spec`, `save_change`, `load_change`, `list_specs`, path helpers |
| `hive/crates/hive_agents/src/coordinator.rs` | Modify | Add `plan_from_change()` method |
| `hive/crates/hive_agents/src/lib.rs` | Modify | Add `mod changes`, `mod spec_store`, export new types |

**No new crate dependencies** — all needed crates (serde, uuid, chrono, hive_fs, hive_ai) are already in `Cargo.toml`.

**Total new tests:** ~30 unit tests across 3 files.
