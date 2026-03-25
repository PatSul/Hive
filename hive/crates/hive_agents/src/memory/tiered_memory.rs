//! TieredMemory orchestrator — routes queries and writes across all memory layers.
//!
//! This is the top-level entry point for the tiered memory architecture.
//! It coordinates the four active layers (Hot, Warm, Cold, Archive) and
//! provides unified query, flush, and session management operations.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::archive::ArchiveService;
use super::session_state::SessionState;
use super::vector_bridge::VectorMemoryBridge;
use super::TargetLayer;
use crate::collective_memory::{CollectiveMemory, MemoryEntry};

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

/// A query across one or more memory layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// The search text / query string.
    pub text: String,
    /// Maximum number of results to return.
    pub max_results: usize,
    /// Which layers to search.
    pub layers: Vec<TargetLayer>,
    /// Recency bias: 0.0 = pure relevance, 1.0 = pure recency.
    pub recency_bias: f64,
}

/// A unified result from any memory layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredEntry {
    /// The textual content of the result.
    pub content: String,
    /// Which layer this result came from.
    pub source_layer: TargetLayer,
    /// Combined relevance/recency score.
    pub score: f64,
    /// ISO-8601 timestamp (or empty if unavailable).
    pub timestamp: String,
    /// Category label.
    pub category: String,
}

/// Result of a cross-layer memory query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQueryResult {
    /// The matched entries, sorted by score descending and deduplicated.
    pub entries: Vec<TieredEntry>,
}

// ---------------------------------------------------------------------------
// TieredMemory orchestrator
// ---------------------------------------------------------------------------

/// Orchestrates queries and writes across all memory layers.
///
/// Owns the session state (L1 Hot), an optional vector bridge (L2 Warm),
/// a shared `CollectiveMemory` (L3 Cold), and an `ArchiveService` (L4 Archive).
pub struct TieredMemory {
    session: SessionState,
    vectors: Option<Arc<dyn VectorMemoryBridge>>,
    collective: Arc<CollectiveMemory>,
    archive: ArchiveService,
}

impl TieredMemory {
    /// Create a new orchestrator with the given layer backends.
    pub fn new(
        session: SessionState,
        vectors: Option<Arc<dyn VectorMemoryBridge>>,
        collective: Arc<CollectiveMemory>,
        archive: ArchiveService,
    ) -> Self {
        Self {
            session,
            vectors,
            collective,
            archive,
        }
    }

    /// Immutable access to the current session state.
    pub fn session(&self) -> &SessionState {
        &self.session
    }

    /// Mutable access to the current session state.
    pub fn session_mut(&mut self) -> &mut SessionState {
        &mut self.session
    }

    /// Query one or more memory layers and return unified, deduplicated results.
    ///
    /// For each requested layer:
    /// - **Hot**: searches session decisions and entity names by substring.
    /// - **Warm**: calls `vectors.query_vectors()` if a bridge is available.
    /// - **Cold**: calls `collective.recall()` with a substring query.
    /// - **Archive**: calls `archive.query_daily_logs()` for the last 30 days.
    ///
    /// Results are scored, adjusted for recency bias, deduplicated via Jaccard
    /// word overlap (threshold 0.8), sorted by score descending, and truncated
    /// to `max_results`.
    pub async fn query(&self, q: &MemoryQuery) -> Result<MemoryQueryResult, String> {
        let mut candidates: Vec<TieredEntry> = Vec::new();

        for layer in &q.layers {
            match layer {
                TargetLayer::Hot => {
                    self.query_hot(&q.text, &mut candidates);
                }
                TargetLayer::Warm => {
                    self.query_warm(&q.text, q.max_results, &mut candidates)
                        .await?;
                }
                TargetLayer::Cold => {
                    self.query_cold(&q.text, q.max_results, &mut candidates)?;
                }
                TargetLayer::Archive => {
                    self.query_archive(&q.text, &mut candidates)?;
                }
            }
        }

        // Apply recency bias to scores.
        for entry in &mut candidates {
            let recency_factor = match entry.source_layer {
                TargetLayer::Hot => 1.0,
                TargetLayer::Warm => 0.7,
                TargetLayer::Cold => 0.5,
                TargetLayer::Archive => 0.3,
            };
            entry.score =
                entry.score * (1.0 - q.recency_bias) + recency_factor * q.recency_bias;
        }

        // Sort by score descending.
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate using Jaccard word overlap.
        let deduped = Self::deduplicate(candidates);

        // Truncate to max_results.
        let entries: Vec<TieredEntry> = deduped.into_iter().take(q.max_results).collect();

        Ok(MemoryQueryResult { entries })
    }

    /// Drain all pending writes from the session and dispatch each to its
    /// target layer.
    ///
    /// Returns the number of writes successfully flushed.
    pub async fn flush_session(&mut self) -> Result<usize, String> {
        let pending = self.session.drain_pending();
        let count = pending.len();

        for write in pending {
            match write.target_layer {
                TargetLayer::Hot => {
                    // Hot writes go back into the session as decisions.
                    self.session
                        .log_decision(&write.content, None);
                }
                TargetLayer::Warm => {
                    if let Some(ref bridge) = self.vectors {
                        bridge
                            .store_vector(
                                &write.content,
                                &write.category.as_str(),
                                write.importance,
                            )
                            .await?;
                    }
                }
                TargetLayer::Cold => {
                    let entry = MemoryEntry {
                        id: 0,
                        category: write.category,
                        content: write.content,
                        tags: Vec::new(),
                        source_run_id: None,
                        source_team_id: None,
                        relevance_score: write.importance as f64,
                        created_at: chrono::Utc::now().to_rfc3339(),
                        last_accessed: chrono::Utc::now().to_rfc3339(),
                        access_count: 0,
                        source_session_id: Some(self.session.session_id.clone()),
                        is_consolidated: false,
                    };
                    self.collective
                        .remember(&entry)
                        .map_err(|e| format!("cold layer write failed: {e}"))?;
                }
                TargetLayer::Archive => {
                    // Archive writes are accumulated and written via
                    // archive_daily(). For now, redirect to Cold so they
                    // aren't lost.
                    let entry = MemoryEntry {
                        id: 0,
                        category: write.category,
                        content: write.content,
                        tags: Vec::new(),
                        source_run_id: None,
                        source_team_id: None,
                        relevance_score: write.importance as f64,
                        created_at: chrono::Utc::now().to_rfc3339(),
                        last_accessed: chrono::Utc::now().to_rfc3339(),
                        access_count: 0,
                        source_session_id: Some(self.session.session_id.clone()),
                        is_consolidated: false,
                    };
                    self.collective
                        .remember(&entry)
                        .map_err(|e| format!("archive-redirect write failed: {e}"))?;
                }
            }
        }

        Ok(count)
    }

    /// Consolidate unconsolidated entries from `CollectiveMemory` into a daily
    /// archive log for the given `date`.
    ///
    /// Uses `recall` with an empty query and a limit of 1000 to gather
    /// entries, then writes them to the archive markdown file.
    pub fn archive_daily(&mut self, date: NaiveDate) -> Result<std::path::PathBuf, String> {
        let entries = self
            .collective
            .recall("", None, None, 1000)
            .map_err(|e| format!("failed to recall entries for archiving: {e}"))?;

        if entries.is_empty() {
            return Err("no entries to archive".to_string());
        }

        self.archive.consolidate_to_daily_log(date, &entries)
    }

    /// Replace the current session with a fresh one.
    pub fn new_session(&mut self, session_id: String) {
        self.session = SessionState::new(session_id);
    }

    /// Attempt to recover session state from a WAL file.
    ///
    /// Returns `true` if a valid session was recovered, `false` if the WAL
    /// file did not exist.
    pub fn recover_session(&mut self, wal_path: &Path) -> Result<bool, String> {
        match SessionState::recover_from_wal(wal_path)? {
            Some(recovered) => {
                self.session = recovered;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Flush the current session state to a WAL file for crash recovery.
    pub fn flush_wal(&self, wal_path: &Path) -> Result<(), String> {
        self.session.flush_to_wal(wal_path)
    }

    // -----------------------------------------------------------------------
    // Private query helpers
    // -----------------------------------------------------------------------

    /// Search the Hot layer (session decisions + entity names) by substring.
    fn query_hot(&self, text: &str, out: &mut Vec<TieredEntry>) {
        let lower = text.to_lowercase();

        // Search decisions.
        for decision in &self.session.decisions_log {
            if decision.content.to_lowercase().contains(&lower) {
                out.push(TieredEntry {
                    content: decision.content.clone(),
                    source_layer: TargetLayer::Hot,
                    score: 0.9,
                    timestamp: decision.timestamp.to_rfc3339(),
                    category: "decision".to_string(),
                });
            }
        }

        // Search entity names.
        for (name, info) in &self.session.entity_cache {
            if name.to_lowercase().contains(&lower) {
                out.push(TieredEntry {
                    content: format!("{} ({})", info.name, info.entity_type),
                    source_layer: TargetLayer::Hot,
                    score: 0.9,
                    timestamp: info.first_seen.to_rfc3339(),
                    category: "entity".to_string(),
                });
            }
        }
    }

    /// Search the Warm layer via the vector bridge.
    async fn query_warm(
        &self,
        text: &str,
        limit: usize,
        out: &mut Vec<TieredEntry>,
    ) -> Result<(), String> {
        let bridge = match &self.vectors {
            Some(b) => b,
            None => return Ok(()), // No vector bridge configured.
        };

        let results = bridge.query_vectors(text, limit).await?;

        for r in results {
            let timestamp = match &r.source {
                super::vector_bridge::VectorSource::Memory { timestamp, .. } => timestamp.clone(),
                super::vector_bridge::VectorSource::Chunk { .. } => String::new(),
            };
            out.push(TieredEntry {
                content: r.content,
                source_layer: TargetLayer::Warm,
                score: r.score as f64,
                timestamp,
                category: r.category,
            });
        }

        Ok(())
    }

    /// Search the Cold layer via CollectiveMemory.
    fn query_cold(
        &self,
        text: &str,
        limit: usize,
        out: &mut Vec<TieredEntry>,
    ) -> Result<(), String> {
        let entries = self
            .collective
            .recall(text, None, None, limit)
            .map_err(|e| format!("cold query failed: {e}"))?;

        for entry in entries {
            out.push(TieredEntry {
                content: entry.content,
                source_layer: TargetLayer::Cold,
                score: entry.relevance_score,
                timestamp: entry.created_at,
                category: entry.category.as_str().to_string(),
            });
        }

        Ok(())
    }

    /// Search the Archive layer (daily logs for the last 30 days).
    fn query_archive(&self, text: &str, out: &mut Vec<TieredEntry>) -> Result<(), String> {
        let today = chrono::Utc::now().date_naive();
        let from = today - chrono::Duration::days(30);

        let keyword = if text.is_empty() {
            None
        } else {
            Some(text)
        };

        let archive_entries = self
            .archive
            .query_daily_logs(from, today, keyword)?;

        for ae in archive_entries {
            out.push(TieredEntry {
                content: ae.content,
                source_layer: TargetLayer::Archive,
                score: 0.5,
                timestamp: ae.date.to_string(),
                category: "archive".to_string(),
            });
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Deduplication
    // -----------------------------------------------------------------------

    /// Remove near-duplicate entries using Jaccard word overlap.
    ///
    /// Iterates through `candidates` (assumed sorted by score descending).
    /// An entry is kept if its word-level Jaccard similarity with every
    /// already-accepted entry is <= 0.8. Otherwise it is skipped.
    fn deduplicate(candidates: Vec<TieredEntry>) -> Vec<TieredEntry> {
        let mut accepted: Vec<TieredEntry> = Vec::new();
        let mut accepted_word_sets: Vec<HashSet<String>> = Vec::new();

        for candidate in candidates {
            let candidate_words = Self::word_set(&candidate.content);

            let is_dup = accepted_word_sets.iter().any(|accepted_words| {
                jaccard_similarity(&candidate_words, accepted_words) > 0.8
            });

            if !is_dup {
                accepted_word_sets.push(candidate_words);
                accepted.push(candidate);
            }
        }

        accepted
    }

    /// Extract a set of lowercase words from a string.
    fn word_set(text: &str) -> HashSet<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|w| w.to_string())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Jaccard similarity
// ---------------------------------------------------------------------------

/// Compute the Jaccard similarity coefficient between two word sets.
///
/// Returns a value in `[0.0, 1.0]` where 1.0 means identical sets.
/// Returns 0.0 if both sets are empty.
fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    intersection / union
}
