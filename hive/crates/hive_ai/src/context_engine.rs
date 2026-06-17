//! Smart Context Curation Engine.
//!
//! Filters thousands of potential context sources (files, symbols, docs,
//! git history) down to the most relevant subset that fits within a token
//! budget. Uses TF-IDF scoring with heuristic boosts for filename matches,
//! symbol names, recency, and test files.

use chrono::{DateTime, Utc};
use hive_core::context::estimate_tokens;
use hive_fs::is_likely_binary;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tracing::debug;

use crate::memory::knowledge_graph::{EdgeKind, KnowledgeGraph, NodeKind};
use crate::quick_index::QuickIndex;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of seed nodes matched from a query before graph expansion.
/// Bounds the cost of graph augmentation regardless of query length.
const MAX_GRAPH_SEEDS: usize = 8;

/// Maximum number of related graph nodes emitted as `SourceType::Graph` sources
/// in a single curation pass. Keeps graph augmentation from crowding out the
/// primary RAG/semantic sources.
const MAX_GRAPH_SOURCES: usize = 8;

/// Fixed relevance boost applied to graph sources, mirroring the constant
/// boosts the scorer gives to `Pattern` / `ProjectKnowledge` sources. Tuned to
/// sit below project-knowledge (+1.0) but above an unmatched file, so a node
/// related to a matched entity ranks alongside a weak direct TF-IDF hit.
const GRAPH_SOURCE_BOOST: f64 = 0.4;

/// Common English stopwords filtered during keyword extraction.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "his", "how", "i", "if", "in", "into", "is", "it", "its", "let", "my", "no",
    "not", "of", "on", "or", "our", "she", "so", "than", "that", "the", "their", "them", "then",
    "there", "these", "they", "this", "to", "us", "was", "we", "were", "what", "when", "where",
    "which", "who", "will", "with", "you", "your",
];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The kind of context source (file, symbol, documentation, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    File,
    Symbol,
    Documentation,
    GitHistory,
    Dependency,
    Config,
    Test,
    /// A learned code pattern from the pattern library.
    Pattern,
    /// Learned user preferences injected as context.
    LearnedPreference,
    /// Project knowledge files (HIVE.md, README.md, etc.).
    ProjectKnowledge,
    /// A node related to the query's matched entities, surfaced from the
    /// knowledge graph (GraphRAG). Emitted during L2 curation when a non-empty
    /// `KnowledgeGraph` is present.
    Graph,
}

impl SourceType {
    /// A stable, human-readable label for this source type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Documentation => "documentation",
            Self::GitHistory => "git-history",
            Self::Dependency => "dependency",
            Self::Config => "config",
            Self::Test => "test",
            Self::Pattern => "pattern",
            Self::LearnedPreference => "learned-preference",
            Self::ProjectKnowledge => "project-knowledge",
            Self::Graph => "graph",
        }
    }
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A single context source with its content and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSource {
    pub path: String,
    pub content: String,
    pub source_type: SourceType,
    pub last_modified: DateTime<Utc>,
}

/// Relevance score for a context source, with reasons explaining the score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceScore {
    pub source_idx: usize,
    pub score: f64,
    pub reasons: Vec<String>,
}

/// Token and source budget constraints for context curation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    pub max_tokens: usize,
    pub max_sources: usize,
    /// Tokens reserved for the prompt/response (subtracted from max_tokens).
    pub reserved_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            max_sources: 50,
            reserved_tokens: 0,
        }
    }
}

/// The result of context curation: selected sources with scores and stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratedContext {
    pub sources: Vec<ContextSource>,
    pub scores: Vec<RelevanceScore>,
    pub total_tokens: usize,
    pub original_count: usize,
    pub selected_count: usize,
}

/// Aggregate statistics about the sources in a `ContextEngine`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStats {
    pub total_sources: usize,
    pub total_tokens_approx: usize,
    pub by_type: HashMap<SourceType, usize>,
}

// ---------------------------------------------------------------------------
// Context Tiers (intent-based retrieval gating)
// ---------------------------------------------------------------------------

/// Context loading tier based on user intent classification.
///
/// L0 = lightweight (chat, translation) — only preferences + knowledge files.
/// L1 = project-aware (creative, data analysis) — L0 + project structure.
/// L2 = full retrieval (coding, reasoning, agentic) — L1 + RAG + semantic search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextTier {
    L0,
    L1,
    L2,
}

impl ContextTier {
    /// Classify a context tier from a task-type keyword string.
    ///
    /// Maps the task types used by `CapabilityRouter::detect_task_type()` to
    /// the appropriate retrieval tier.
    pub fn from_task_keyword(task_type: &str) -> Self {
        match task_type.to_lowercase().as_str() {
            "general_chat" | "translation" | "summarization" => Self::L0,
            "creative_writing" | "instruction_following" | "data_analysis" => Self::L1,
            _ => Self::L2, // coding, reasoning, math, tool_use, agentic, vision
        }
    }
}

// ---------------------------------------------------------------------------
// Fact Extraction (from compaction summaries)
// ---------------------------------------------------------------------------

/// Category of an extracted fact from a compacted conversation summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactCategory {
    Preference,
    Decision,
    CodePattern,
    Fact,
}

/// A fact extracted from a compaction summary.
#[derive(Debug, Clone)]
pub struct ExtractedFact {
    pub category: FactCategory,
    pub content: String,
}

/// Parse structured facts from a compaction summary.
///
/// The summarization prompt is engineered to output lines prefixed with
/// `Preference:`, `Decision:`, `Pattern:`, or `Fact:`. This function
/// extracts those into typed `ExtractedFact` values.
pub fn extract_facts(summary: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();
    for line in summary.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Preference:") {
            let content = rest.trim().to_string();
            if !content.is_empty() {
                facts.push(ExtractedFact { category: FactCategory::Preference, content });
            }
        } else if let Some(rest) = trimmed.strip_prefix("Decision:") {
            let content = rest.trim().to_string();
            if !content.is_empty() {
                facts.push(ExtractedFact { category: FactCategory::Decision, content });
            }
        } else if let Some(rest) = trimmed.strip_prefix("Pattern:") {
            let content = rest.trim().to_string();
            if !content.is_empty() {
                facts.push(ExtractedFact { category: FactCategory::CodePattern, content });
            }
        } else if let Some(rest) = trimmed.strip_prefix("Fact:") {
            let content = rest.trim().to_string();
            if !content.is_empty() {
                facts.push(ExtractedFact { category: FactCategory::Fact, content });
            }
        }
    }
    facts
}

// ---------------------------------------------------------------------------
// Keyword / tokenization helpers
// ---------------------------------------------------------------------------

/// Tokenize text into lowercase word tokens, splitting on non-alphanumeric
/// characters (underscore preserved).
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

/// Return whether `word` is a stopword.
fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}

// ---------------------------------------------------------------------------
// ContextEngine
// ---------------------------------------------------------------------------

/// Smart context curation engine.
///
/// Collects context sources, scores them against a query using TF-IDF with
/// heuristic boosts, and greedily packs the highest-scoring sources into a
/// token budget.
pub struct ContextEngine {
    sources: Vec<ContextSource>,
    /// Cached IDF values keyed by term. Invalidated when sources change.
    idf_cache: HashMap<String, f64>,
    /// Knowledge graph used to augment curation with structurally-related
    /// entities (GraphRAG). Starts empty; an empty graph is a strict no-op, so
    /// curation behaves byte-for-byte identically until a graph is supplied via
    /// [`ContextEngine::set_graph`] or [`ContextEngine::rebuild_graph_from_index`].
    graph: KnowledgeGraph,
}

impl ContextEngine {
    /// Create a new, empty context engine.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            idf_cache: HashMap::new(),
            graph: KnowledgeGraph::new(),
        }
    }

    /// Add a pre-built context source.
    pub fn add_source(&mut self, source: ContextSource) {
        self.sources.push(source);
        self.idf_cache.clear();
    }

    /// Convenience: add a file source with the current timestamp.
    pub fn add_file(&mut self, path: &str, content: &str) {
        self.add_source(ContextSource {
            path: path.to_string(),
            content: content.to_string(),
            source_type: SourceType::File,
            last_modified: Utc::now(),
        });
    }

    /// Convenience: add a symbol source (e.g. a function/struct body).
    pub fn add_symbol(&mut self, name: &str, body: &str) {
        self.add_source(ContextSource {
            path: name.to_string(),
            content: body.to_string(),
            source_type: SourceType::Symbol,
            last_modified: Utc::now(),
        });
    }

    /// Add a learned code pattern as a context source.
    pub fn add_pattern(&mut self, description: &str, pattern_code: &str, language: &str) {
        self.add_source(ContextSource {
            path: format!("pattern::{}::{}", language, description),
            content: pattern_code.to_string(),
            source_type: SourceType::Pattern,
            last_modified: Utc::now(),
        });
    }

    /// Add learned user preferences as a context source.
    pub fn add_learned_preferences(&mut self, preferences_text: &str) {
        if preferences_text.is_empty() {
            return;
        }
        self.add_source(ContextSource {
            path: "learned::preferences".to_string(),
            content: preferences_text.to_string(),
            source_type: SourceType::LearnedPreference,
            last_modified: Utc::now(),
        });
    }

    /// Remove all ephemeral sources (File, Symbol, Test, etc.) while keeping
    /// persistent ones (ProjectKnowledge, LearnedPreference). Call this at the
    /// start of each message's context assembly to avoid TF-IDF index bloat.
    pub fn clear_ephemeral(&mut self) {
        self.sources.retain(|s| {
            matches!(
                s.source_type,
                SourceType::ProjectKnowledge | SourceType::LearnedPreference
            )
        });
        self.idf_cache.clear();
    }

    /// Add a project knowledge file (HIVE.md, README.md, etc.) as a context source.
    pub fn add_project_knowledge(&mut self, label: &str, content: &str) {
        if content.is_empty() {
            return;
        }
        self.add_source(ContextSource {
            path: format!("knowledge::{}", label),
            content: content.to_string(),
            source_type: SourceType::ProjectKnowledge,
            last_modified: Utc::now(),
        });
    }

    /// Recursively walk `dir_path`, read text files, and add them as
    /// `SourceType::File` sources. Returns the number of files indexed.
    pub fn index_directory(&mut self, dir_path: &str) -> anyhow::Result<usize> {
        let path = Path::new(dir_path);
        self.walk_directory(path)
    }

    // -- Knowledge graph (GraphRAG) ----------------------------------------

    /// Replace the engine's knowledge graph wholesale. This is the injection
    /// seam used to supply a graph built elsewhere (e.g. from a higher layer
    /// that has Obsidian links) or a synthetic graph in tests.
    ///
    /// Supplying [`KnowledgeGraph::new()`] (an empty graph) reverts curation to
    /// its pre-graph behaviour.
    pub fn set_graph(&mut self, graph: KnowledgeGraph) {
        self.graph = graph;
    }

    /// Number of nodes in the engine's knowledge graph. `0` means graph
    /// augmentation is inert.
    pub fn graph_node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// (Re)build the knowledge graph from an in-crate [`QuickIndex`] signal.
    ///
    /// The graph is a file/symbol structure derived entirely from data the
    /// `hive_ai` crate already produces — no new cross-crate dependency:
    ///
    /// * Each file with symbols becomes a `File` node.
    /// * Each symbol becomes a `Symbol` node with a `Reference` edge to the file
    ///   that defines it (`symbol -> file`).
    /// * Symbols defined in the same file are linked pairwise with
    ///   `CoOccurrence` edges, so neighbours of a matched symbol surface its
    ///   siblings.
    ///
    /// This is intentionally cheap and re-runnable; it discards any previously
    /// set graph. A `QuickIndex` with no symbols yields an empty graph (no-op).
    pub fn rebuild_graph_from_index(&mut self, index: &QuickIndex) {
        self.graph = build_graph_from_quick_index(index);
    }


    /// Curate the most relevant sources for `query` within `budget`.
    ///
    /// Algorithm:
    /// 1. Extract keywords from the query (tokenize + filter stopwords).
    /// 2. Compute TF-IDF relevance for every source.
    /// 3. Apply heuristic boosts (filename match, symbol match, recency, tests).
    /// 4. Sort by score descending.
    /// 5. Greedily pack sources into the available token budget.
    pub fn curate(&mut self, query: &str, budget: &ContextBudget) -> CuratedContext {
        let original_count = self.sources.len();

        if self.sources.is_empty() {
            return CuratedContext {
                sources: Vec::new(),
                scores: Vec::new(),
                total_tokens: 0,
                original_count: 0,
                selected_count: 0,
            };
        }

        // Step 1: extract query keywords.
        let query_keywords = self.extract_keywords(query);
        let query_terms: Vec<&str> = query_keywords.iter().map(|s| s.as_str()).collect();

        // Rebuild IDF cache if empty (invalidated on source add).
        if self.idf_cache.is_empty() {
            self.rebuild_idf_cache();
        }

        // Step 2 + 3: score each source.
        let mut scored: Vec<RelevanceScore> = self
            .sources
            .iter()
            .enumerate()
            .map(|(idx, source)| self.score_source(idx, source, &query_terms, query))
            .collect();

        // Step 4: sort by score descending.
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 5: greedy packing into budget.
        let available_tokens = budget.max_tokens.saturating_sub(budget.reserved_tokens);
        let mut total_tokens = 0usize;
        let mut selected_sources = Vec::new();
        let mut selected_scores = Vec::new();

        for rs in &scored {
            if selected_sources.len() >= budget.max_sources {
                break;
            }
            let source = &self.sources[rs.source_idx];
            let tokens = self.estimate_source_tokens(source);
            if total_tokens + tokens > available_tokens {
                continue;
            }
            total_tokens += tokens;
            selected_sources.push(source.clone());
            selected_scores.push(rs.clone());
        }

        // Step 6 (additive): augment with knowledge-graph neighbours.
        //
        // Strictly a no-op when the graph is empty: `augment_with_graph` returns
        // immediately, leaving `selected_*` and `total_tokens` untouched, so the
        // pre-graph curation output is reproduced byte-for-byte.
        self.augment_with_graph(
            &query_terms,
            budget,
            available_tokens,
            &mut total_tokens,
            &mut selected_sources,
            &mut selected_scores,
        );

        let selected_count = selected_sources.len();
        debug!(
            "Curated {}/{} sources ({} tokens) for query '{}'",
            selected_count, original_count, total_tokens, query
        );

        CuratedContext {
            sources: selected_sources,
            scores: selected_scores,
            total_tokens,
            original_count,
            selected_count,
        }
    }

    /// Compute the TF-IDF score for `document` against `query_terms`.
    pub fn compute_tf_idf(&self, query_terms: &[&str], document: &str) -> f64 {
        if query_terms.is_empty() || document.is_empty() {
            return 0.0;
        }

        let doc_tokens = tokenize(document);
        let doc_len = doc_tokens.len() as f64;
        if doc_len == 0.0 {
            return 0.0;
        }

        // Term frequency in document.
        let mut tf_counts: HashMap<&str, usize> = HashMap::new();
        for token in &doc_tokens {
            for &qt in query_terms {
                if token == qt {
                    *tf_counts.entry(qt).or_insert(0) += 1;
                }
            }
        }

        let mut score = 0.0;
        for &term in query_terms {
            let tf = tf_counts.get(term).copied().unwrap_or(0) as f64 / doc_len;
            let idf = self.compute_idf(term);
            score += tf * idf;
        }

        score
    }

    /// Compute inverse document frequency: `ln(N / df)`.
    /// Returns 0.0 if the term does not appear in any source.
    pub fn compute_idf(&self, term: &str) -> f64 {
        if let Some(&cached) = self.idf_cache.get(term) {
            return cached;
        }

        let n = self.sources.len() as f64;
        if n == 0.0 {
            return 0.0;
        }
        let df = self
            .sources
            .iter()
            .filter(|s| {
                let lower = s.content.to_lowercase();
                lower.contains(term)
            })
            .count() as f64;

        if df == 0.0 {
            return 0.0;
        }
        (n / df).ln()
    }

    /// Extract keywords from text: tokenize, lowercase, and filter stopwords.
    pub fn extract_keywords(&self, text: &str) -> Vec<String> {
        tokenize(text)
            .into_iter()
            .filter(|w| !is_stopword(w))
            .collect()
    }

    /// Estimate the token count of a source's content using
    /// `hive_core::context::estimate_tokens`.
    pub fn estimate_source_tokens(&self, source: &ContextSource) -> usize {
        estimate_tokens(&source.content)
    }

    /// Return aggregate statistics about the sources in this engine.
    pub fn summary_stats(&self) -> ContextStats {
        let mut by_type: HashMap<SourceType, usize> = HashMap::new();
        let mut total_tokens = 0usize;

        for source in &self.sources {
            *by_type.entry(source.source_type).or_insert(0) += 1;
            total_tokens += estimate_tokens(&source.content);
        }

        ContextStats {
            total_sources: self.sources.len(),
            total_tokens_approx: total_tokens,
            by_type,
        }
    }

    // -- Private helpers ----------------------------------------------------

    /// Rebuild the IDF cache from all current sources.
    fn rebuild_idf_cache(&mut self) {
        let n = self.sources.len() as f64;
        if n == 0.0 {
            return;
        }

        // Collect the unique tokens per source.
        let doc_token_sets: Vec<HashSet<String>> = self
            .sources
            .iter()
            .map(|s| tokenize(&s.content).into_iter().collect())
            .collect();

        // Gather all unique terms.
        let all_terms: HashSet<&str> = doc_token_sets
            .iter()
            .flat_map(|s| s.iter().map(|t| t.as_str()))
            .collect();

        for term in all_terms {
            let df = doc_token_sets
                .iter()
                .filter(|set| set.contains(term))
                .count() as f64;
            let idf = if df == 0.0 { 0.0 } else { (n / df).ln() };
            self.idf_cache.insert(term.to_string(), idf);
        }
    }

    /// Score a single source against the query. Returns a `RelevanceScore`
    /// with the raw TF-IDF score plus heuristic boosts.
    fn score_source(
        &self,
        idx: usize,
        source: &ContextSource,
        query_terms: &[&str],
        query_raw: &str,
    ) -> RelevanceScore {
        let mut score = self.compute_tf_idf(query_terms, &source.content);
        let mut reasons: Vec<String> = Vec::new();

        if score > 0.0 {
            reasons.push(format!("tf-idf: {:.4}", score));
        }

        // Boost: filename/path contains a query term (+0.5).
        let path_lower = source.path.to_lowercase();
        let query_lower = query_raw.to_lowercase();
        let has_filename_match = query_terms.iter().any(|term| path_lower.contains(term));
        if has_filename_match {
            score += 0.5;
            reasons.push("filename match (+0.5)".to_string());
        }

        // Boost: symbol name match (+0.3) — only for Symbol sources.
        if source.source_type == SourceType::Symbol {
            let name_lower = source.path.to_lowercase();
            let has_symbol_match = query_terms.iter().any(|term| name_lower.contains(term));
            if has_symbol_match {
                score += 0.3;
                reasons.push("symbol name match (+0.3)".to_string());
            }
        }

        // Boost: recently modified (+0.2) — within the last hour.
        let age = Utc::now().signed_duration_since(source.last_modified);
        if age.num_hours() < 1 {
            score += 0.2;
            reasons.push("recent modification (+0.2)".to_string());
        }

        // Boost: test files when querying code (+0.1).
        let looks_like_code_query = query_lower.contains("fn ")
            || query_lower.contains("struct ")
            || query_lower.contains("impl ")
            || query_lower.contains("test")
            || query_lower.contains("error")
            || query_lower.contains("bug");
        if source.source_type == SourceType::Test && looks_like_code_query {
            score += 0.1;
            reasons.push("test file for code query (+0.1)".to_string());
        }

        // Boost: project knowledge files always rank highly (+1.0).
        if source.source_type == SourceType::ProjectKnowledge {
            score += 1.0;
            reasons.push("project knowledge file (+1.0)".to_string());
        }

        RelevanceScore {
            source_idx: idx,
            score,
            reasons,
        }
    }

    /// Augment the curated set with knowledge-graph neighbours of the query's
    /// matched entities (GraphRAG).
    ///
    /// This runs after the normal greedy packing and only ever *appends*. It is
    /// a strict no-op when the graph is empty — the early return below leaves
    /// every `&mut` argument untouched — so curation is byte-for-byte unchanged
    /// when no graph has been supplied.
    ///
    /// Steps:
    /// 1. Match query terms against node ids/labels to find up to
    ///    `MAX_GRAPH_SEEDS` seed nodes.
    /// 2. Collect undirected neighbours of the seeds (the "related" nodes),
    ///    excluding the seeds themselves and anything already selected.
    /// 3. Emit each as a `SourceType::Graph` source, scored with the same
    ///    TF-IDF pipeline plus a fixed graph boost, deduplicated by path and
    ///    bounded by the token / source budget.
    #[allow(clippy::too_many_arguments)]
    fn augment_with_graph(
        &self,
        query_terms: &[&str],
        budget: &ContextBudget,
        available_tokens: usize,
        total_tokens: &mut usize,
        selected_sources: &mut Vec<ContextSource>,
        selected_scores: &mut Vec<RelevanceScore>,
    ) {
        // Guard: an empty graph contributes nothing. This is the invariant that
        // keeps non-graph curation unchanged.
        if self.graph.node_count() == 0 || query_terms.is_empty() {
            return;
        }

        // Step 1: match query terms against node ids/labels to seed the search.
        // A node is a seed if any query term is a substring of its (lowercased)
        // id or label — the same containment heuristic the path/symbol boosts
        // use, applied to graph entities.
        let mut seeds: Vec<String> = Vec::new();
        let mut seen_seed: HashSet<String> = HashSet::new();
        for (id, node) in self.graph.nodes() {
            if seeds.len() >= MAX_GRAPH_SEEDS {
                break;
            }
            let id_lower = id.to_lowercase();
            let label_lower = node.label.to_lowercase();
            let matched = query_terms
                .iter()
                .any(|t| id_lower.contains(t) || label_lower.contains(t));
            if matched && seen_seed.insert(id.to_string()) {
                seeds.push(id.to_string());
            }
        }
        if seeds.is_empty() {
            return;
        }

        // Set of paths already present so graph sources never duplicate an
        // already-selected file/symbol or another graph node.
        let mut present: HashSet<String> =
            selected_sources.iter().map(|s| s.path.clone()).collect();
        let seed_set: HashSet<&str> = seeds.iter().map(|s| s.as_str()).collect();

        // Step 2: gather related node ids (neighbours of every seed), in a
        // deterministic order.
        let mut related: Vec<String> = Vec::new();
        let mut seen_related: HashSet<String> = HashSet::new();
        for seed in &seeds {
            for nbr in self.graph.neighbors(seed) {
                if seed_set.contains(nbr.as_str()) {
                    continue; // a seed itself is not a "related" emission
                }
                if seen_related.insert(nbr.clone()) {
                    related.push(nbr);
                }
            }
        }
        related.sort(); // determinism independent of seed iteration order

        // Step 3: build, score, dedupe, and budget-pack graph sources.
        let mut graph_scored: Vec<(ContextSource, RelevanceScore)> = Vec::new();
        for related_id in &related {
            let Some(node) = self.graph.node(related_id) else {
                continue;
            };
            // The graph node's "path" is its id; skip if already selected.
            if present.contains(related_id) {
                continue;
            }

            // Find which seed this node connects to, for an explanation.
            let via = seeds
                .iter()
                .find(|s| self.graph.neighbors(s).iter().any(|n| n == related_id))
                .map(|s| s.as_str())
                .unwrap_or("query");
            let relation = self
                .graph
                .explain_path(via, related_id)
                .map(|labels| labels.join(" -> "))
                .unwrap_or_else(|| node.label.clone());

            let content = format!(
                "Graph relation: {} ({}). Connected via: {}.",
                node.label,
                graph_node_kind_label(node.kind),
                relation,
            );

            let source = ContextSource {
                path: format!("graph::{related_id}"),
                content,
                source_type: SourceType::Graph,
                last_modified: Utc::now(),
            };

            // Score consistently with the rest of the pipeline: TF-IDF over the
            // synthetic content plus a fixed graph boost (mirrors Pattern /
            // ProjectKnowledge constant boosts).
            let mut score = self.compute_tf_idf(query_terms, &source.content);
            let mut reasons = Vec::new();
            if score > 0.0 {
                reasons.push(format!("tf-idf: {score:.4}"));
            }
            score += GRAPH_SOURCE_BOOST;
            reasons.push(format!("graph neighbour of '{via}' (+{GRAPH_SOURCE_BOOST})"));

            graph_scored.push((
                source,
                RelevanceScore {
                    // Index into the curated `sources` slice (graph sources are
                    // synthetic and not part of `self.sources`).
                    source_idx: selected_sources.len() + graph_scored.len(),
                    score,
                    reasons,
                },
            ));
        }

        // Highest-scoring graph nodes first; tie-break by path for determinism.
        graph_scored.sort_by(|a, b| {
            b.1.score
                .partial_cmp(&a.1.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.path.cmp(&b.0.path))
        });

        let mut emitted = 0usize;
        for (source, mut rs) in graph_scored {
            if emitted >= MAX_GRAPH_SOURCES || selected_sources.len() >= budget.max_sources {
                break;
            }
            if present.contains(&source.path) {
                continue;
            }
            let tokens = self.estimate_source_tokens(&source);
            if *total_tokens + tokens > available_tokens {
                continue;
            }
            *total_tokens += tokens;
            present.insert(source.path.clone());
            // Re-point the score index to the actual final slot.
            rs.source_idx = selected_sources.len();
            selected_sources.push(source);
            selected_scores.push(rs);
            emitted += 1;
        }
    }

    /// Recursively walk a directory and add text files as sources.
    fn walk_directory(&mut self, path: &Path) -> anyhow::Result<usize> {
        let mut count = 0;

        let entries: Vec<_> = fs::read_dir(path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {}: {}", path.display(), e))?
            .collect();

        for entry in entries {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files/directories.
            if file_name.starts_with('.') {
                continue;
            }

            if entry_path.is_dir() {
                count += self.walk_directory(&entry_path)?;
            } else if entry_path.is_file() {
                if is_likely_binary(&entry_path) {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&entry_path) {
                    let path_str = entry_path.to_string_lossy().to_string();
                    let source_type = infer_source_type(&entry_path);
                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(DateTime::<Utc>::from)
                        .unwrap_or_else(Utc::now);

                    self.add_source(ContextSource {
                        path: path_str,
                        content,
                        source_type,
                        last_modified: modified,
                    });
                    count += 1;
                }
            }
        }

        Ok(count)
    }
}

impl Default for ContextEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/// A short human-readable label for a graph node's kind, for inclusion in the
/// synthetic content of a `SourceType::Graph` source.
fn graph_node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::File => "file",
        NodeKind::Symbol => "symbol",
        NodeKind::Note => "note",
        NodeKind::Memory => "memory",
        NodeKind::Other => "entity",
    }
}

/// Build a [`KnowledgeGraph`] from a [`QuickIndex`] — an in-`hive_ai` signal,
/// so no new cross-crate dependency is introduced.
///
/// The structure is a file/symbol reference + co-occurrence graph:
///
/// * each defining file becomes a `File` node,
/// * each symbol becomes a `Symbol` node with a `Reference` edge to its file
///   (`symbol -> file`), and
/// * symbols sharing a file get pairwise `CoOccurrence` edges so a matched
///   symbol surfaces its siblings as neighbours.
///
/// Node ids are stable: the relative file path for files, and
/// `"<file>::<name>"` for symbols (so identically-named symbols in different
/// files stay distinct). The graph is empty when the index has no symbols,
/// which makes graph augmentation a no-op.
fn build_graph_from_quick_index(index: &QuickIndex) -> KnowledgeGraph {
    let mut graph = KnowledgeGraph::new();

    // Group symbol ids by their defining file.
    let mut by_file: HashMap<&str, Vec<String>> = HashMap::new();

    for sym in &index.key_symbols {
        let file = sym.file.as_str();
        let sym_id = format!("{file}::{}", sym.name);

        graph.add_node(file, NodeKind::File, file);
        graph.add_node(sym_id.clone(), NodeKind::Symbol, sym.name.clone());
        // symbol references the file that defines it.
        graph.add_edge(&sym_id, file, EdgeKind::Reference, 1.0);

        by_file.entry(file).or_default().push(sym_id);
    }

    // Link symbols that co-occur in the same file (pairwise, undirected by
    // convention — a single directed edge suffices since `neighbors` is
    // undirected).
    for ids in by_file.values() {
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                graph.add_edge(&ids[i], &ids[j], EdgeKind::CoOccurrence, 1.0);
            }
        }
    }

    graph
}

/// Infer the `SourceType` from a file path based on common patterns.
fn infer_source_type(path: &Path) -> SourceType {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if name.contains("test") || name.starts_with("test_") || name.ends_with("_test.rs") {
        return SourceType::Test;
    }
    if ext == "md" || ext == "txt" || ext == "adoc" || ext == "rst" {
        return SourceType::Documentation;
    }
    if name == "cargo.toml"
        || name == "package.json"
        || name == "go.mod"
        || name == "requirements.txt"
    {
        return SourceType::Dependency;
    }
    if ext == "toml" || ext == "yaml" || ext == "yml" || ext == "json" || ext == "ini" {
        return SourceType::Config;
    }

    SourceType::File
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a source with sensible defaults.
    fn make_source(path: &str, content: &str, source_type: SourceType) -> ContextSource {
        ContextSource {
            path: path.to_string(),
            content: content.to_string(),
            source_type,
            last_modified: Utc::now(),
        }
    }

    fn default_budget() -> ContextBudget {
        ContextBudget {
            max_tokens: 100_000,
            max_sources: 100,
            reserved_tokens: 0,
        }
    }

    // -- Core curate behavior -----------------------------------------------

    #[test]
    fn test_empty_engine_curate() {
        let mut engine = ContextEngine::new();
        let result = engine.curate("anything", &default_budget());

        assert_eq!(result.original_count, 0);
        assert_eq!(result.selected_count, 0);
        assert_eq!(result.total_tokens, 0);
        assert!(result.sources.is_empty());
        assert!(result.scores.is_empty());
    }

    #[test]
    fn test_add_source_and_curate() {
        let mut engine = ContextEngine::new();
        engine.add_source(make_source(
            "main.rs",
            "fn main() { println!(\"hello world\"); }",
            SourceType::File,
        ));

        let result = engine.curate("main hello", &default_budget());

        assert_eq!(result.original_count, 1);
        assert_eq!(result.selected_count, 1);
        assert!(result.total_tokens > 0);
        assert!(!result.scores.is_empty());
        assert!(result.scores[0].score > 0.0);
    }

    #[test]
    fn test_relevance_scoring_prefers_matching_content() {
        let mut engine = ContextEngine::new();
        engine.add_source(make_source(
            "math.rs",
            "fn add(a: i32, b: i32) -> i32 { a + b }",
            SourceType::File,
        ));
        engine.add_source(make_source(
            "greet.rs",
            "fn greet(name: &str) { println!(\"Hello {}\", name); }",
            SourceType::File,
        ));

        let result = engine.curate("add numbers", &default_budget());

        // math.rs should rank higher because it contains "add".
        assert_eq!(result.selected_count, 2);
        assert_eq!(result.sources[0].path, "math.rs");
    }

    #[test]
    fn test_budget_token_limit_respected() {
        let mut engine = ContextEngine::new();
        // Each source is ~50 chars => ~13 tokens.
        for i in 0..20 {
            engine.add_file(
                &format!("file_{}.rs", i),
                &format!("fn func_{}() {{ /* body with some content */ }}", i),
            );
        }

        let budget = ContextBudget {
            max_tokens: 30, // Very tight — should fit only a couple.
            max_sources: 100,
            reserved_tokens: 0,
        };
        let result = engine.curate("func", &budget);

        assert!(result.total_tokens <= 30);
        assert!(result.selected_count < 20);
    }

    #[test]
    fn test_budget_source_limit_respected() {
        let mut engine = ContextEngine::new();
        for i in 0..20 {
            engine.add_file(&format!("file_{}.rs", i), "fn hello() {}");
        }

        let budget = ContextBudget {
            max_tokens: 100_000,
            max_sources: 3,
            reserved_tokens: 0,
        };
        let result = engine.curate("hello", &budget);

        assert!(result.selected_count <= 3);
    }

    #[test]
    fn test_filename_match_boost() {
        let mut engine = ContextEngine::new();
        // Source whose path contains "auth" but content does not.
        engine.add_source(make_source(
            "auth_handler.rs",
            "fn process_request(r: Request) -> Response { r.into() }",
            SourceType::File,
        ));
        // Source whose content mentions auth but path does not.
        engine.add_source(make_source(
            "handler.rs",
            "fn auth_check(token: &str) -> bool { !token.is_empty() }",
            SourceType::File,
        ));

        let result = engine.curate("auth", &default_budget());

        // auth_handler.rs should get the filename boost and rank first.
        assert_eq!(result.sources[0].path, "auth_handler.rs");
        let first_score = &result.scores[0];
        assert!(first_score.reasons.iter().any(|r| r.contains("filename")));
    }

    // -- Keyword extraction -------------------------------------------------

    #[test]
    fn test_keyword_extraction() {
        let engine = ContextEngine::new();
        let keywords = engine.extract_keywords("the quick brown fox");
        assert!(keywords.contains(&"quick".to_string()));
        assert!(keywords.contains(&"brown".to_string()));
        assert!(keywords.contains(&"fox".to_string()));
    }

    #[test]
    fn test_stopword_filtering() {
        let engine = ContextEngine::new();
        let keywords = engine.extract_keywords("the a an is in to of and or");
        assert!(keywords.is_empty(), "All stopwords should be filtered");
    }

    // -- TF-IDF computation -------------------------------------------------

    #[test]
    fn test_tf_idf_computation() {
        let mut engine = ContextEngine::new();
        engine.add_file("a.rs", "fn alpha beta gamma");
        engine.add_file("b.rs", "fn delta epsilon alpha");

        // "alpha" appears in both docs, "gamma" in one.
        let score_alpha = engine.compute_tf_idf(&["alpha"], "fn alpha beta gamma");
        let score_gamma = engine.compute_tf_idf(&["gamma"], "fn alpha beta gamma");

        // "gamma" is rarer (higher IDF) so its TF-IDF should be higher.
        assert!(
            score_gamma > score_alpha,
            "Rarer term 'gamma' should score higher: gamma={}, alpha={}",
            score_gamma,
            score_alpha
        );
    }

    #[test]
    fn test_tf_idf_empty_inputs() {
        let engine = ContextEngine::new();
        assert_eq!(engine.compute_tf_idf(&[], "some content"), 0.0);
        assert_eq!(engine.compute_tf_idf(&["query"], ""), 0.0);
    }

    // -- Ranking and stats --------------------------------------------------

    #[test]
    fn test_multiple_sources_ranking() {
        let mut engine = ContextEngine::new();
        engine.add_file("config.toml", "database_url = localhost");
        engine.add_file(
            "database.rs",
            "fn connect_database(url: &str) { /* connect */ }",
        );
        engine.add_file(
            "utils.rs",
            "fn format_string(s: &str) -> String { s.to_string() }",
        );

        let result = engine.curate("database connect", &default_budget());

        // database.rs should rank first (content + filename match).
        assert!(!result.sources.is_empty());
        assert_eq!(result.sources[0].path, "database.rs");
    }

    #[test]
    fn test_summary_stats() {
        let mut engine = ContextEngine::new();
        engine.add_source(make_source("a.rs", "fn a() {}", SourceType::File));
        engine.add_source(make_source("b.rs", "fn b() {}", SourceType::File));
        engine.add_source(make_source(
            "c_test.rs",
            "#[test] fn t() {}",
            SourceType::Test,
        ));
        engine.add_source(make_source(
            "readme.md",
            "# Title",
            SourceType::Documentation,
        ));

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 4);
        assert!(stats.total_tokens_approx > 0);
        assert_eq!(stats.by_type[&SourceType::File], 2);
        assert_eq!(stats.by_type[&SourceType::Test], 1);
        assert_eq!(stats.by_type[&SourceType::Documentation], 1);
    }

    #[test]
    fn test_source_type_variants() {
        // Verify all SourceType variants can be serialized round-tripped.
        let variants = [
            SourceType::File,
            SourceType::Symbol,
            SourceType::Documentation,
            SourceType::GitHistory,
            SourceType::Dependency,
            SourceType::Config,
            SourceType::Test,
            SourceType::Pattern,
            SourceType::LearnedPreference,
            SourceType::ProjectKnowledge,
            SourceType::Graph,
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: SourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }
        // Graph serializes in snake_case like the other variants.
        assert_eq!(serde_json::to_string(&SourceType::Graph).unwrap(), "\"graph\"");
    }

    // -- Convenience methods ------------------------------------------------

    #[test]
    fn test_add_file_convenience() {
        let mut engine = ContextEngine::new();
        engine.add_file("main.rs", "fn main() {}");

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 1);
        assert_eq!(stats.by_type[&SourceType::File], 1);
    }

    #[test]
    fn test_add_symbol_convenience() {
        let mut engine = ContextEngine::new();
        engine.add_symbol("MyStruct::process", "fn process(&self) { todo!() }");

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 1);
        assert_eq!(stats.by_type[&SourceType::Symbol], 1);
    }

    #[test]
    fn test_reserved_tokens_reduces_budget() {
        let mut engine = ContextEngine::new();
        // ~100 chars => ~25 tokens each.
        for i in 0..10 {
            let content = format!(
                "fn function_{}() {{ let x = {}; let y = x + 1; println!(\"{{x}} {{y}}\"); }}",
                i, i
            );
            engine.add_file(&format!("f_{}.rs", i), &content);
        }

        let tight_budget = ContextBudget {
            max_tokens: 60,
            max_sources: 100,
            reserved_tokens: 40,
        };
        let result = engine.curate("function", &tight_budget);

        // Only 20 tokens available after reservation.
        assert!(result.total_tokens <= 20);
    }

    #[test]
    fn test_curated_context_serialization() {
        let mut engine = ContextEngine::new();
        engine.add_file("test.rs", "fn test() {}");
        let result = engine.curate("test", &default_budget());

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CuratedContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.selected_count, result.selected_count);
        assert_eq!(deserialized.original_count, result.original_count);
    }

    #[test]
    fn test_infer_source_type_from_path() {
        assert_eq!(
            infer_source_type(Path::new("src/tests/foo_test.rs")),
            SourceType::Test
        );
        assert_eq!(
            infer_source_type(Path::new("README.md")),
            SourceType::Documentation
        );
        assert_eq!(
            infer_source_type(Path::new("Cargo.toml")),
            SourceType::Dependency
        );
        assert_eq!(
            infer_source_type(Path::new("config.yaml")),
            SourceType::Config
        );
        assert_eq!(
            infer_source_type(Path::new("src/main.rs")),
            SourceType::File
        );
    }

    // -- Context tier tests ------------------------------------------------

    #[test]
    fn test_context_tier_classification() {
        assert_eq!(ContextTier::from_task_keyword("general_chat"), ContextTier::L0);
        assert_eq!(ContextTier::from_task_keyword("translation"), ContextTier::L0);
        assert_eq!(ContextTier::from_task_keyword("summarization"), ContextTier::L0);
        assert_eq!(ContextTier::from_task_keyword("creative_writing"), ContextTier::L1);
        assert_eq!(ContextTier::from_task_keyword("data_analysis"), ContextTier::L1);
        assert_eq!(ContextTier::from_task_keyword("coding"), ContextTier::L2);
        assert_eq!(ContextTier::from_task_keyword("reasoning"), ContextTier::L2);
        assert_eq!(ContextTier::from_task_keyword("tool_use"), ContextTier::L2);
        assert_eq!(ContextTier::from_task_keyword("unknown"), ContextTier::L2);
    }

    #[test]
    fn test_clear_ephemeral_keeps_durable_sources() {
        let mut engine = ContextEngine::new();
        engine.add_file("temp.rs", "fn temp() {}");
        engine.add_symbol("MyStruct", "struct MyStruct {}");
        engine.add_project_knowledge("README", "# Project");
        engine.add_learned_preferences("Prefers concise code");

        assert_eq!(engine.summary_stats().total_sources, 4);

        engine.clear_ephemeral();

        let stats = engine.summary_stats();
        assert_eq!(stats.total_sources, 2);
        assert_eq!(stats.by_type.get(&SourceType::ProjectKnowledge).copied().unwrap_or(0), 1);
        assert_eq!(stats.by_type.get(&SourceType::LearnedPreference).copied().unwrap_or(0), 1);
        assert_eq!(stats.by_type.get(&SourceType::File).copied().unwrap_or(0), 0);
    }

    // -- Fact extraction tests ---------------------------------------------

    #[test]
    fn test_extract_facts_parses_structured_lines() {
        let summary = "Preference: User prefers Rust over Python\n\
                       Decision: Use SQLite for persistence\n\
                       Pattern: Always use Result<T> for error handling\n\
                       Fact: Project has 21 crates\n\
                       This is just a regular summary line.";

        let facts = extract_facts(summary);
        assert_eq!(facts.len(), 4);
        assert_eq!(facts[0].category, FactCategory::Preference);
        assert_eq!(facts[0].content, "User prefers Rust over Python");
        assert_eq!(facts[1].category, FactCategory::Decision);
        assert_eq!(facts[2].category, FactCategory::CodePattern);
        assert_eq!(facts[3].category, FactCategory::Fact);
    }

    #[test]
    fn test_extract_facts_empty_content_skipped() {
        let summary = "Preference:\nFact: Something real";
        let facts = extract_facts(summary);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, FactCategory::Fact);
    }

    // -- GraphRAG augmentation tests ---------------------------------------

    use crate::memory::knowledge_graph::{EdgeKind, KnowledgeGraph, NodeKind};

    /// Build a synthetic graph where the query term "auth" matches a seed node
    /// that is connected to two related nodes the query does NOT mention.
    fn auth_graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        // Seed: matched by the query keyword "auth".
        g.add_node("auth_service", NodeKind::Symbol, "auth_service");
        // Related neighbours — not mentioned in the query at all.
        g.add_node("token_store", NodeKind::Symbol, "token_store");
        g.add_node("session_cache", NodeKind::Symbol, "session_cache");
        g.add_edge("auth_service", "token_store", EdgeKind::Reference, 1.0);
        g.add_edge("auth_service", "session_cache", EdgeKind::CoOccurrence, 1.0);
        // A wholly unrelated, disconnected node — must never surface.
        g.add_node("unrelated_widget", NodeKind::Symbol, "unrelated_widget");
        g
    }

    #[test]
    fn test_graph_augments_with_related_nodes() {
        let mut engine = ContextEngine::new();
        // One ordinary source so curate() runs its normal path.
        engine.add_file("main.rs", "fn main() { let x = compute(); }");
        engine.set_graph(auth_graph());
        assert!(engine.graph_node_count() > 0);

        let result = engine.curate("auth login flow", &default_budget());

        // Graph sources for the seed's neighbours must be present.
        let graph_paths: Vec<&str> = result
            .sources
            .iter()
            .filter(|s| s.source_type == SourceType::Graph)
            .map(|s| s.path.as_str())
            .collect();
        assert!(
            graph_paths.contains(&"graph::token_store"),
            "expected token_store neighbour, got {graph_paths:?}"
        );
        assert!(
            graph_paths.contains(&"graph::session_cache"),
            "expected session_cache neighbour, got {graph_paths:?}"
        );
        // The seed itself is not emitted as a related node...
        assert!(!graph_paths.contains(&"graph::auth_service"));
        // ...and the disconnected, unmatched node never appears.
        assert!(!graph_paths.contains(&"graph::unrelated_widget"));

        // Each graph source carries a graph-neighbour reason for explainability.
        let graph_scores: Vec<&RelevanceScore> = result
            .scores
            .iter()
            .zip(result.sources.iter())
            .filter(|(_, s)| s.source_type == SourceType::Graph)
            .map(|(rs, _)| rs)
            .collect();
        assert!(!graph_scores.is_empty());
        for rs in graph_scores {
            assert!(rs.reasons.iter().any(|r| r.contains("graph neighbour")));
        }
    }

    #[test]
    fn test_empty_graph_leaves_curation_unchanged() {
        // Two identical engines; only one differs by having an explicitly-empty
        // graph set. Their curated output must be identical.
        let build = || {
            let mut e = ContextEngine::new();
            e.add_file("main.rs", "fn main() { let x = auth(); }");
            e.add_file("util.rs", "fn helper() {}");
            e.add_project_knowledge("README", "# Project with auth");
            e
        };

        let mut baseline = build();
        let baseline_result = baseline.curate("auth helper", &default_budget());

        let mut with_empty_graph = build();
        with_empty_graph.set_graph(KnowledgeGraph::new());
        assert_eq!(with_empty_graph.graph_node_count(), 0);
        let graph_result = with_empty_graph.curate("auth helper", &default_budget());

        // No graph sources at all.
        assert!(
            graph_result
                .sources
                .iter()
                .all(|s| s.source_type != SourceType::Graph),
            "empty graph must not emit any Graph sources"
        );

        // Byte-for-byte equivalent curation: same selection, order, tokens.
        assert_eq!(graph_result.selected_count, baseline_result.selected_count);
        assert_eq!(graph_result.original_count, baseline_result.original_count);
        assert_eq!(graph_result.total_tokens, baseline_result.total_tokens);
        let baseline_paths: Vec<&str> =
            baseline_result.sources.iter().map(|s| s.path.as_str()).collect();
        let graph_paths: Vec<&str> =
            graph_result.sources.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(graph_paths, baseline_paths);
    }

    #[test]
    fn test_graph_no_seed_match_is_noop() {
        // Non-empty graph, but the query matches no node => no graph sources.
        let mut engine = ContextEngine::new();
        engine.add_file("main.rs", "fn main() {}");
        engine.set_graph(auth_graph());

        let result = engine.curate("completely different topic", &default_budget());
        assert!(result
            .sources
            .iter()
            .all(|s| s.source_type != SourceType::Graph));
    }

    #[test]
    fn test_graph_sources_respect_source_budget() {
        let mut engine = ContextEngine::new();
        engine.add_file("main.rs", "fn main() { auth(); }");
        engine.set_graph(auth_graph());

        // Only allow a single source total — the non-graph file fills it, so no
        // graph source can be appended.
        let budget = ContextBudget {
            max_tokens: 100_000,
            max_sources: 1,
            reserved_tokens: 0,
        };
        let result = engine.curate("auth", &budget);
        assert!(result.selected_count <= 1);
        assert!(result
            .sources
            .iter()
            .all(|s| s.source_type != SourceType::Graph));
    }

    #[test]
    fn test_rebuild_graph_from_index_builds_file_symbol_graph() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("auth.rs"),
            "pub fn auth_login() {}\npub fn auth_logout() {}\n",
        )
        .unwrap();

        let index = crate::quick_index::QuickIndex::build(dir.path());
        let mut engine = ContextEngine::new();
        engine.rebuild_graph_from_index(&index);

        // File + at least the two symbols => non-empty graph.
        assert!(
            engine.graph_node_count() >= 3,
            "expected file + symbol nodes, got {}",
            engine.graph_node_count()
        );

        engine.add_file("driver.rs", "fn caller() { auth_login(); }");
        let result = engine.curate("auth_login", &default_budget());

        // Querying one symbol should surface its co-occurring sibling and/or
        // its defining file as graph neighbours.
        let has_graph_source = result
            .sources
            .iter()
            .any(|s| s.source_type == SourceType::Graph);
        assert!(
            has_graph_source,
            "expected at least one Graph source from the index-built graph"
        );
    }
}
