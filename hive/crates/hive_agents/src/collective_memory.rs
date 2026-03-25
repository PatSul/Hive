use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// MemoryCategory
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MemoryCategory {
    SuccessPattern,
    FailurePattern,
    ModelInsight,
    ConflictResolution,
    CodePattern,
    UserPreference,
    General,
}

impl MemoryCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SuccessPattern => "SuccessPattern",
            Self::FailurePattern => "FailurePattern",
            Self::ModelInsight => "ModelInsight",
            Self::ConflictResolution => "ConflictResolution",
            Self::CodePattern => "CodePattern",
            Self::UserPreference => "UserPreference",
            Self::General => "General",
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s {
            "SuccessPattern" => Self::SuccessPattern,
            "FailurePattern" => Self::FailurePattern,
            "ModelInsight" => Self::ModelInsight,
            "ConflictResolution" => Self::ConflictResolution,
            "CodePattern" => Self::CodePattern,
            "UserPreference" => Self::UserPreference,
            _ => Self::General,
        }
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// MemoryEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryEntry {
    pub id: i64,
    pub category: MemoryCategory,
    pub content: String,
    pub tags: Vec<String>,
    pub source_run_id: Option<String>,
    pub source_team_id: Option<String>,
    pub relevance_score: f64,
    pub created_at: String,
    pub last_accessed: String,
    pub access_count: u64,
    pub source_session_id: Option<String>,
    pub is_consolidated: bool,
}

impl MemoryEntry {
    /// Convenience constructor with sensible defaults.
    pub fn new(category: MemoryCategory, content: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: 0,
            category,
            content: content.into(),
            tags: Vec::new(),
            source_run_id: None,
            source_team_id: None,
            relevance_score: 1.0,
            created_at: now.clone(),
            last_accessed: now,
            access_count: 0,
            source_session_id: None,
            is_consolidated: false,
        }
    }
}

// ---------------------------------------------------------------------------
// MemoryStats
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub by_category: HashMap<MemoryCategory, usize>,
    pub avg_relevance: f64,
}

// ---------------------------------------------------------------------------
// MaintenanceReport
// ---------------------------------------------------------------------------

/// Report from a memory maintenance run.
#[derive(Debug)]
pub struct MaintenanceReport {
    /// Number of entries whose relevance was decayed.
    pub decayed: usize,
    /// Number of entries pruned (below relevance threshold).
    pub pruned: usize,
    /// Number of duplicate entries removed.
    pub deduplicated: usize,
}

// ---------------------------------------------------------------------------
// CollectiveMemory
// ---------------------------------------------------------------------------

pub struct CollectiveMemory {
    conn: Mutex<Connection>,
    /// Optional cortex event sender for publishing memory events.
    event_tx: std::sync::RwLock<Option<hive_learn::cortex::event_bus::CortexEventSender>>,
}

impl CollectiveMemory {
    /// Open (or create) a SQLite database at `path`.
    pub fn open(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("Failed to open database: {e}"))?;
        Self::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            event_tx: std::sync::RwLock::new(None),
        })
    }

    /// Create an in-memory SQLite database (useful for testing).
    pub fn in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory db: {e}"))?;
        Self::init_tables(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            event_tx: std::sync::RwLock::new(None),
        })
    }

    /// Set the cortex event sender for publishing memory events.
    pub fn set_event_tx(&self, tx: hive_learn::cortex::event_bus::CortexEventSender) {
        if let Ok(mut guard) = self.event_tx.write() {
            *guard = Some(tx);
        }
    }

    // -- private -------------------------------------------------------------

    fn init_tables(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                category        TEXT    NOT NULL,
                content         TEXT    NOT NULL,
                tags            TEXT    NOT NULL DEFAULT '[]',
                source_run_id   TEXT,
                source_team_id  TEXT,
                relevance_score REAL    NOT NULL DEFAULT 1.0,
                created_at      TEXT    NOT NULL,
                last_accessed   TEXT    NOT NULL,
                access_count    INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_memories_category
                ON memories(category);

            CREATE INDEX IF NOT EXISTS idx_memories_relevance
                ON memories(relevance_score DESC);",
        )
        .map_err(|e| format!("Failed to initialise tables: {e}"))?;

        Self::migrate_v2(conn)
    }

    /// Add `source_session_id` and `is_consolidated` columns if they don't
    /// already exist.  Safe to call multiple times (idempotent).
    fn migrate_v2(conn: &Connection) -> Result<(), String> {
        // Check which columns already exist via PRAGMA table_info.
        let mut stmt = conn
            .prepare("PRAGMA table_info(memories)")
            .map_err(|e| format!("PRAGMA error: {e}"))?;

        let columns: Vec<String> = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                Ok(name)
            })
            .map_err(|e| format!("PRAGMA query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        if !columns.iter().any(|c| c == "source_session_id") {
            conn.execute_batch(
                "ALTER TABLE memories ADD COLUMN source_session_id TEXT;",
            )
            .map_err(|e| format!("Migration error (source_session_id): {e}"))?;
        }

        if !columns.iter().any(|c| c == "is_consolidated") {
            conn.execute_batch(
                "ALTER TABLE memories ADD COLUMN is_consolidated INTEGER NOT NULL DEFAULT 0;",
            )
            .map_err(|e| format!("Migration error (is_consolidated): {e}"))?;
        }

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(source_session_id);",
        )
        .map_err(|e| format!("Migration error (session index): {e}"))?;

        Ok(())
    }

    /// Parse a row from the memories table into a `MemoryEntry`.
    fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<MemoryEntry, rusqlite::Error> {
        let id: i64 = row.get(0)?;
        let cat_str: String = row.get(1)?;
        let content: String = row.get(2)?;
        let tags_json: String = row.get(3)?;
        let source_run_id: Option<String> = row.get(4)?;
        let source_team_id: Option<String> = row.get(5)?;
        let relevance_score: f64 = row.get(6)?;
        let created_at: String = row.get(7)?;
        let last_accessed: String = row.get(8)?;
        let access_count: i64 = row.get(9)?;

        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_else(|e| {
            tracing::warn!("CollectiveMemory: corrupt tags JSON for entry {id}: {e}");
            Vec::new()
        });

        let source_session_id: Option<String> = row.get(10).unwrap_or(None);
        let is_consolidated: bool = row.get::<_, i32>(11).map(|v| v != 0).unwrap_or(false);

        Ok(MemoryEntry {
            id,
            category: MemoryCategory::parse_str(&cat_str),
            content,
            tags,
            source_run_id,
            source_team_id,
            relevance_score,
            created_at,
            last_accessed,
            access_count: access_count as u64,
            source_session_id,
            is_consolidated,
        })
    }

    // -- public API ----------------------------------------------------------

    /// Insert a new memory entry. Returns the new row id.
    pub fn remember(&self, entry: &MemoryEntry) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let tags_json =
            serde_json::to_string(&entry.tags).map_err(|e| format!("JSON error: {e}"))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO memories (category, content, tags, source_run_id, source_team_id,
                                   relevance_score, created_at, last_accessed, access_count,
                                   source_session_id, is_consolidated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                entry.category.as_str(),
                entry.content,
                tags_json,
                entry.source_run_id,
                entry.source_team_id,
                entry.relevance_score,
                now,
                now,
                entry.access_count as i64,
                entry.source_session_id,
                entry.is_consolidated as i32,
            ],
        )
        .map_err(|e| format!("Insert error: {e}"))?;

        let row_id = conn.last_insert_rowid();

        // Publish CollectiveMemoryEntry cortex event.
        if let Ok(guard) = self.event_tx.read() {
            if let Some(ref tx) = *guard {
                let cortex_category = match entry.category {
                    MemoryCategory::SuccessPattern => hive_learn::cortex::types::CortexMemoryCategory::SuccessPattern,
                    MemoryCategory::FailurePattern => hive_learn::cortex::types::CortexMemoryCategory::FailurePattern,
                    MemoryCategory::ModelInsight => hive_learn::cortex::types::CortexMemoryCategory::ModelInsight,
                    MemoryCategory::ConflictResolution => hive_learn::cortex::types::CortexMemoryCategory::ConflictResolution,
                    MemoryCategory::CodePattern => hive_learn::cortex::types::CortexMemoryCategory::CodePattern,
                    MemoryCategory::UserPreference => hive_learn::cortex::types::CortexMemoryCategory::UserPreference,
                    MemoryCategory::General => hive_learn::cortex::types::CortexMemoryCategory::General,
                };
                let _ = tx.send(hive_learn::cortex::event_bus::CortexEvent::CollectiveMemoryEntry {
                    category: cortex_category,
                    content: entry.content.clone(),
                    relevance_score: entry.relevance_score,
                });
            }
        }

        Ok(row_id)
    }

    /// Query memories.
    ///
    /// - `query`    — substring match against `content` (case-insensitive via LIKE).
    /// - `category` — optional filter on `MemoryCategory`.
    /// - `tags`     — optional: every supplied tag must appear in the stored JSON array.
    /// - `limit`    — max rows to return.
    pub fn recall(
        &self,
        query: &str,
        category: Option<MemoryCategory>,
        tags: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut sql = String::from(
            "SELECT id, category, content, tags, source_run_id, source_team_id,
                    relevance_score, created_at, last_accessed, access_count,
                    source_session_id, is_consolidated
             FROM memories
             WHERE content LIKE ?1",
        );

        if category.is_some() {
            sql.push_str(" AND category = ?2");
        }

        // Tag filtering: each required tag must appear as a JSON element.
        // We use `tags LIKE '%"tag"%'` for every tag — simple and sufficient for
        // JSON-encoded arrays of strings.
        let tag_clauses: Vec<String> = if let Some(t) = tags {
            t.iter()
                .enumerate()
                .map(|(i, _tag)| {
                    let param_idx = if category.is_some() { 3 + i } else { 2 + i };
                    format!(" AND tags LIKE ?{param_idx}")
                })
                .collect()
        } else {
            Vec::new()
        };
        for clause in &tag_clauses {
            sql.push_str(clause);
        }

        sql.push_str(" ORDER BY relevance_score DESC");
        sql.push_str(&format!(" LIMIT {limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Prepare error: {e}"))?;

        // Build a vector of boxed dyn ToSql so we can handle a dynamic number of params.
        let like_query = format!("%{query}%");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(like_query));

        if let Some(cat) = category {
            param_values.push(Box::new(cat.as_str().to_string()));
        }

        if let Some(t) = tags {
            for tag in t {
                param_values.push(Box::new(format!("%\"{tag}\"%")));
            }
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), Self::row_to_entry)
            .map_err(|e| format!("Query error: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }

        Ok(results)
    }

    /// Bump a memory's access metadata.
    ///
    /// Sets `last_accessed` to now, increments `access_count`, and gives a tiny
    /// relevance boost (x 1.01).
    pub fn touch(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE memories
             SET last_accessed   = ?1,
                 access_count    = access_count + 1,
                 relevance_score = relevance_score * 1.01
             WHERE id = ?2",
            params![now, id],
        )
        .map_err(|e| format!("Touch error: {e}"))?;

        Ok(())
    }

    /// Multiply every entry's `relevance_score` by `factor` (typically < 1.0 to
    /// decay stale memories). Returns the number of rows affected.
    pub fn decay_scores(&self, factor: f64) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let changed = conn
            .execute(
                "UPDATE memories SET relevance_score = relevance_score * ?1",
                params![factor],
            )
            .map_err(|e| format!("Decay error: {e}"))?;

        Ok(changed)
    }

    /// Delete all entries whose `relevance_score` is below `min_relevance`.
    /// Returns the number of rows deleted.
    pub fn prune(&self, min_relevance: f64) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM memories WHERE relevance_score < ?1",
                params![min_relevance],
            )
            .map_err(|e| format!("Prune error: {e}"))?;

        Ok(deleted)
    }

    /// Aggregate statistics across the memory store.
    pub fn stats(&self) -> Result<MemoryStats, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let total_entries: usize = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .map_err(|e| format!("Count error: {e}"))?;

        let avg_relevance: f64 = if total_entries == 0 {
            0.0
        } else {
            conn.query_row("SELECT AVG(relevance_score) FROM memories", [], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Avg error: {e}"))?
        };

        let mut by_category: HashMap<MemoryCategory, usize> = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT category, COUNT(*) FROM memories GROUP BY category")
                .map_err(|e| format!("Prepare error: {e}"))?;

            let rows = stmt
                .query_map([], |row| {
                    let cat_str: String = row.get(0)?;
                    let count: usize = row.get(1)?;
                    Ok((cat_str, count))
                })
                .map_err(|e| format!("Query error: {e}"))?;

            for row in rows {
                let (cat_str, count) = row.map_err(|e| format!("Row error: {e}"))?;
                by_category.insert(MemoryCategory::parse_str(&cat_str), count);
            }
        }

        Ok(MemoryStats {
            total_entries,
            by_category,
            avg_relevance,
        })
    }

    /// Return the total number of entries in the store.
    pub fn entry_count(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .map_err(|e| format!("Count error: {e}"))?;

        Ok(count)
    }

    /// Remove near-duplicate entries using Jaccard word similarity.
    ///
    /// Groups entries by category, compares content tokenized into words,
    /// and removes entries with similarity above `threshold` (0.0-1.0),
    /// keeping the entry with the higher relevance score.
    pub fn deduplicate(&self, threshold: f64) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        // Load all entries grouped by category.
        let mut stmt = conn
            .prepare(
                "SELECT id, category, content, tags, source_run_id, source_team_id,
                        relevance_score, created_at, last_accessed, access_count,
                        source_session_id, is_consolidated
                 FROM memories ORDER BY category, relevance_score DESC",
            )
            .map_err(|e| format!("Prepare error: {e}"))?;

        let entries: Vec<MemoryEntry> = stmt
            .query_map([], Self::row_to_entry)
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        // Group by category.
        let mut by_category: HashMap<String, Vec<&MemoryEntry>> = HashMap::new();
        for entry in &entries {
            by_category
                .entry(entry.category.as_str().to_string())
                .or_default()
                .push(entry);
        }

        let mut to_delete: Vec<i64> = Vec::new();

        for (_cat, group) in &by_category {
            // Tokenize each entry's content into word sets.
            let tokenized: Vec<std::collections::HashSet<String>> = group
                .iter()
                .map(|e| {
                    e.content
                        .split_whitespace()
                        .map(|w| w.to_lowercase())
                        .collect()
                })
                .collect();

            for i in 0..group.len() {
                if to_delete.contains(&group[i].id) {
                    continue;
                }
                for j in (i + 1)..group.len() {
                    if to_delete.contains(&group[j].id) {
                        continue;
                    }
                    // Jaccard similarity.
                    let intersection = tokenized[i].intersection(&tokenized[j]).count();
                    let union = tokenized[i].union(&tokenized[j]).count();
                    if union == 0 {
                        continue;
                    }
                    let similarity = intersection as f64 / union as f64;
                    if similarity >= threshold {
                        // Keep the one with higher relevance (group is sorted desc).
                        to_delete.push(group[j].id);
                    }
                }
            }
        }

        // Delete duplicates.
        let count = to_delete.len();
        for id in &to_delete {
            conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
                .map_err(|e| format!("Delete error: {e}"))?;
        }

        Ok(count)
    }

    /// Run full memory maintenance: decay, prune, then deduplicate.
    ///
    /// - `decay_factor`: multiplier for relevance scores (e.g. 0.98 for 2% daily decay)
    /// - `min_relevance`: entries below this threshold are pruned
    /// - `similarity_threshold`: entries above this Jaccard similarity are deduplicated
    pub fn maintenance(
        &self,
        decay_factor: f64,
        min_relevance: f64,
        similarity_threshold: f64,
    ) -> Result<MaintenanceReport, String> {
        let decayed = self.decay_scores(decay_factor)?;
        let pruned = self.prune(min_relevance)?;
        let deduplicated = self.deduplicate(similarity_threshold)?;

        Ok(MaintenanceReport {
            decayed,
            pruned,
            deduplicated,
        })
    }

    /// Get all memories from a specific session, ordered by creation time.
    pub fn get_session_memories(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, category, content, tags, source_run_id, source_team_id,
                        relevance_score, created_at, last_accessed, access_count,
                        source_session_id, is_consolidated
                 FROM memories
                 WHERE source_session_id = ?1
                 ORDER BY created_at ASC
                 LIMIT ?2",
            )
            .map_err(|e| format!("Prepare error: {e}"))?;

        let rows = stmt
            .query_map(params![session_id, limit as i64], Self::row_to_entry)
            .map_err(|e| format!("Query error: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| format!("Row error: {e}"))?);
        }

        Ok(results)
    }

    /// Mark a batch of memory entries as consolidated.
    pub fn consolidate_batch(&self, entry_ids: &[i64]) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut updated = 0usize;
        for id in entry_ids {
            let changed = conn
                .execute(
                    "UPDATE memories SET is_consolidated = 1 WHERE id = ?1",
                    params![id],
                )
                .map_err(|e| format!("Consolidate error: {e}"))?;
            updated += changed;
        }

        Ok(updated)
    }

    /// Count memories that haven't been consolidated yet.
    pub fn unconsolidated_count(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let count: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE is_consolidated = 0",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("Count error: {e}"))?;

        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(category: MemoryCategory, content: &str) -> MemoryEntry {
        MemoryEntry::new(category, content)
    }

    fn make_tagged_entry(category: MemoryCategory, content: &str, tags: &[&str]) -> MemoryEntry {
        let mut e = MemoryEntry::new(category, content);
        e.tags = tags.iter().map(|s| s.to_string()).collect();
        e
    }

    #[test]
    fn remember_and_recall_roundtrip() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let entry = make_entry(
            MemoryCategory::SuccessPattern,
            "Use batch inserts for speed",
        );

        let id = mem.remember(&entry).unwrap();
        assert!(id > 0);

        let results = mem.recall("batch", None, None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Use batch inserts for speed");
        assert_eq!(results[0].category, MemoryCategory::SuccessPattern);
    }

    #[test]
    fn recall_with_category_filter() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "pattern A"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::FailurePattern, "pattern B"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "pattern C"))
            .unwrap();

        let success_only = mem
            .recall("pattern", Some(MemoryCategory::SuccessPattern), None, 10)
            .unwrap();
        assert_eq!(success_only.len(), 2);
        for entry in &success_only {
            assert_eq!(entry.category, MemoryCategory::SuccessPattern);
        }

        let failure_only = mem
            .recall("pattern", Some(MemoryCategory::FailurePattern), None, 10)
            .unwrap();
        assert_eq!(failure_only.len(), 1);
        assert_eq!(failure_only[0].content, "pattern B");
    }

    #[test]
    fn recall_with_query_filter() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "alpha beta gamma"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "delta epsilon"))
            .unwrap();

        let hits = mem.recall("beta", None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "alpha beta gamma");

        let hits = mem.recall("epsilon", None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "delta epsilon");

        let hits = mem.recall("nonexistent", None, None, 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn recall_with_tag_filter() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_tagged_entry(
            MemoryCategory::CodePattern,
            "use iterators",
            &["rust", "performance"],
        ))
        .unwrap();
        mem.remember(&make_tagged_entry(
            MemoryCategory::CodePattern,
            "use channels",
            &["rust", "concurrency"],
        ))
        .unwrap();
        mem.remember(&make_tagged_entry(
            MemoryCategory::CodePattern,
            "use promises",
            &["javascript"],
        ))
        .unwrap();

        let rust_tags = vec!["rust".to_string()];
        let hits = mem.recall("use", None, Some(&rust_tags), 10).unwrap();
        assert_eq!(hits.len(), 2);

        let perf_tags = vec!["performance".to_string()];
        let hits = mem.recall("use", None, Some(&perf_tags), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "use iterators");
    }

    #[test]
    fn touch_boosts_access_count_and_relevance() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let entry = make_entry(MemoryCategory::ModelInsight, "gpt-4 good at reasoning");
        let id = mem.remember(&entry).unwrap();

        // Before touch
        let before = mem.recall("reasoning", None, None, 1).unwrap();
        assert_eq!(before[0].access_count, 0);
        let score_before = before[0].relevance_score;

        // Touch twice
        mem.touch(id).unwrap();
        mem.touch(id).unwrap();

        let after = mem.recall("reasoning", None, None, 1).unwrap();
        assert_eq!(after[0].access_count, 2);
        assert!(after[0].relevance_score > score_before);
    }

    #[test]
    fn decay_scores_reduces_all() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "entry one"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "entry two"))
            .unwrap();

        let affected = mem.decay_scores(0.5).unwrap();
        assert_eq!(affected, 2);

        let entries = mem.recall("entry", None, None, 10).unwrap();
        for e in &entries {
            assert!((e.relevance_score - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn prune_removes_low_relevance() {
        let mem = CollectiveMemory::in_memory().unwrap();

        // Insert a high-relevance entry and a default one.
        let mut keeper = make_entry(MemoryCategory::General, "keeper");
        keeper.relevance_score = 5.0;
        mem.remember(&keeper).unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "doomed"))
            .unwrap();

        // Decay all scores by 0.1 => keeper becomes 0.5, doomed becomes 0.1
        mem.decay_scores(0.1).unwrap();

        // Prune anything below 0.15 — only "doomed" (0.1) should be deleted.
        let deleted = mem.prune(0.15).unwrap();
        assert_eq!(deleted, 1);

        let remaining = mem.recall("", None, None, 10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content, "keeper");
    }

    #[test]
    fn stats_returns_correct_counts() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "a"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::SuccessPattern, "b"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::FailurePattern, "c"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "d"))
            .unwrap();

        let s = mem.stats().unwrap();
        assert_eq!(s.total_entries, 4);
        assert_eq!(s.by_category[&MemoryCategory::SuccessPattern], 2);
        assert_eq!(s.by_category[&MemoryCategory::FailurePattern], 1);
        assert_eq!(s.by_category[&MemoryCategory::General], 1);
        assert!((s.avg_relevance - 1.0).abs() < 0.001);
    }

    #[test]
    fn empty_database_recall_returns_empty() {
        let mem = CollectiveMemory::in_memory().unwrap();
        let results = mem.recall("anything", None, None, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn entry_count_works() {
        let mem = CollectiveMemory::in_memory().unwrap();
        assert_eq!(mem.entry_count().unwrap(), 0);

        mem.remember(&make_entry(MemoryCategory::General, "one"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "two"))
            .unwrap();

        assert_eq!(mem.entry_count().unwrap(), 2);
    }

    #[test]
    fn memory_category_display_and_roundtrip() {
        let cats = [
            MemoryCategory::SuccessPattern,
            MemoryCategory::FailurePattern,
            MemoryCategory::ModelInsight,
            MemoryCategory::ConflictResolution,
            MemoryCategory::CodePattern,
            MemoryCategory::UserPreference,
            MemoryCategory::General,
        ];
        for cat in &cats {
            let s = cat.to_string();
            let back = MemoryCategory::parse_str(&s);
            assert_eq!(*cat, back);
        }
        // Unknown string falls back to General.
        assert_eq!(MemoryCategory::parse_str("bogus"), MemoryCategory::General);
    }

    #[test]
    fn deduplicate_removes_near_duplicates() {
        let mem = CollectiveMemory::in_memory().unwrap();
        mem.remember(&make_entry(
            MemoryCategory::CodePattern,
            "use iterators for performance in rust code",
        ))
        .unwrap();
        mem.remember(&make_entry(
            MemoryCategory::CodePattern,
            "use iterators for performance in rust programs",
        ))
        .unwrap();
        mem.remember(&make_entry(
            MemoryCategory::CodePattern,
            "completely different content about databases",
        ))
        .unwrap();

        let removed = mem.deduplicate(0.7).unwrap();
        assert_eq!(removed, 1, "One near-duplicate should be removed");
        assert_eq!(mem.entry_count().unwrap(), 2);
    }

    #[test]
    fn maintenance_runs_all_three_operations() {
        let mem = CollectiveMemory::in_memory().unwrap();
        // Insert entries that will be affected by maintenance.
        mem.remember(&make_entry(MemoryCategory::General, "entry alpha"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "entry beta"))
            .unwrap();

        let report = mem.maintenance(0.98, 0.01, 0.85).unwrap();
        assert_eq!(report.decayed, 2);
        // Nothing pruned (0.98 * 1.0 = 0.98 > 0.01 threshold)
        assert_eq!(report.pruned, 0);
        // Nothing deduplicated (different content)
        assert_eq!(report.deduplicated, 0);
    }

    #[test]
    fn test_session_memories_roundtrip() {
        let mem = CollectiveMemory::in_memory().unwrap();

        let mut e1 = make_entry(MemoryCategory::General, "session-a first");
        e1.source_session_id = Some("session-a".to_string());
        mem.remember(&e1).unwrap();

        let mut e2 = make_entry(MemoryCategory::General, "session-b only");
        e2.source_session_id = Some("session-b".to_string());
        mem.remember(&e2).unwrap();

        let mut e3 = make_entry(MemoryCategory::General, "session-a second");
        e3.source_session_id = Some("session-a".to_string());
        mem.remember(&e3).unwrap();

        // No session (None)
        mem.remember(&make_entry(MemoryCategory::General, "no session"))
            .unwrap();

        let a_entries = mem.get_session_memories("session-a", 100).unwrap();
        assert_eq!(a_entries.len(), 2);
        assert_eq!(a_entries[0].content, "session-a first");
        assert_eq!(a_entries[1].content, "session-a second");
        for e in &a_entries {
            assert_eq!(e.source_session_id.as_deref(), Some("session-a"));
        }

        let b_entries = mem.get_session_memories("session-b", 100).unwrap();
        assert_eq!(b_entries.len(), 1);
        assert_eq!(b_entries[0].content, "session-b only");

        let none_entries = mem.get_session_memories("nonexistent", 100).unwrap();
        assert!(none_entries.is_empty());
    }

    #[test]
    fn test_consolidate_batch() {
        let mem = CollectiveMemory::in_memory().unwrap();

        let id1 = mem
            .remember(&make_entry(MemoryCategory::General, "entry 1"))
            .unwrap();
        let id2 = mem
            .remember(&make_entry(MemoryCategory::General, "entry 2"))
            .unwrap();
        let id3 = mem
            .remember(&make_entry(MemoryCategory::General, "entry 3"))
            .unwrap();

        // Consolidate first two
        let updated = mem.consolidate_batch(&[id1, id2]).unwrap();
        assert_eq!(updated, 2);

        // Verify flags via recall
        let all = mem.recall("entry", None, None, 10).unwrap();
        for e in &all {
            if e.id == id1 || e.id == id2 {
                assert!(e.is_consolidated, "entry {} should be consolidated", e.id);
            } else if e.id == id3 {
                assert!(!e.is_consolidated, "entry 3 should NOT be consolidated");
            }
        }
    }

    #[test]
    fn test_unconsolidated_count() {
        let mem = CollectiveMemory::in_memory().unwrap();

        let id1 = mem
            .remember(&make_entry(MemoryCategory::General, "a"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "b"))
            .unwrap();
        mem.remember(&make_entry(MemoryCategory::General, "c"))
            .unwrap();

        assert_eq!(mem.unconsolidated_count().unwrap(), 3);

        mem.consolidate_batch(&[id1]).unwrap();
        assert_eq!(mem.unconsolidated_count().unwrap(), 2);
    }

    #[test]
    fn test_migration_is_idempotent() {
        let conn =
            Connection::open_in_memory().expect("Failed to open in-memory db");
        CollectiveMemory::init_tables(&conn).unwrap();
        // Second call must not error.
        CollectiveMemory::migrate_v2(&conn).unwrap();
    }
}
