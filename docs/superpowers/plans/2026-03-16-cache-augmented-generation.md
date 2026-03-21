# Cache-Augmented Generation (CAG) Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a CAG mode that loads entire small codebases into the LLM context window, bypassing RAG/TF-IDF scoring, with automatic strategy detection and Anthropic prompt caching to reduce cost.

**Architecture:** Extend existing `ContextEngine` with a `load_all()` method that bypasses scoring. Add `to_full_snapshot()` to `QuickIndex` to read actual file contents (not just symbols). Enable the already-implemented Anthropic prompt caching by setting `cache_system_prompt: true` on the `ChatRequest` after `prepare_stream()`. Add a `context_strategy` config field to let users choose Auto/CAG/RAG. In Auto mode, detect project size and pick the optimal strategy.

**Tech Stack:** Rust, GPUI, hive_ai, hive_core, hive_fs

---

## Chunk 1: Core Types and Config

### Task 1: Add `ContextStrategy` enum to hive_core

**Files:**
- Modify: `hive/crates/hive_core/src/config.rs:269-398` (HiveConfig struct)
- Modify: `hive/crates/hive_core/src/lib.rs` (re-export)

- [ ] **Step 1: Write the failing test**

Add to the bottom of the existing `#[cfg(test)] mod tests` block in `config.rs`:

```rust
#[test]
fn context_strategy_from_str() {
    assert_eq!("auto".parse::<ContextStrategy>().unwrap(), ContextStrategy::Auto);
    assert_eq!("cag".parse::<ContextStrategy>().unwrap(), ContextStrategy::Cag);
    assert_eq!("rag".parse::<ContextStrategy>().unwrap(), ContextStrategy::Rag);
    assert!("invalid".parse::<ContextStrategy>().is_err());
}

#[test]
fn context_strategy_display() {
    assert_eq!(ContextStrategy::Auto.to_string(), "auto");
    assert_eq!(ContextStrategy::Cag.to_string(), "cag");
    assert_eq!(ContextStrategy::Rag.to_string(), "rag");
}

#[test]
fn context_strategy_serde_roundtrip() {
    let auto = ContextStrategy::Auto;
    let json = serde_json::to_string(&auto).unwrap();
    assert_eq!(json, "\"auto\"");
    let parsed: ContextStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, ContextStrategy::Auto);
}

#[test]
fn hive_config_default_context_strategy() {
    let cfg = HiveConfig::default();
    assert_eq!(cfg.context_strategy, ContextStrategy::Auto);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --package hive_core context_strategy`
Expected: FAIL — `ContextStrategy` type does not exist.

- [ ] **Step 3: Write the ContextStrategy enum**

Add above the `HiveConfig` struct definition (before line 269) in `config.rs`:

```rust
/// Strategy for assembling AI context from project files.
///
/// - `Auto`: detect project size and pick CAG or RAG automatically.
/// - `Cag`: always load the entire codebase into context (best for small projects).
/// - `Rag`: always use TF-IDF retrieval (best for large projects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextStrategy {
    #[default]
    Auto,
    Cag,
    Rag,
}

impl std::fmt::Display for ContextStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Cag => write!(f, "cag"),
            Self::Rag => write!(f, "rag"),
        }
    }
}

impl std::str::FromStr for ContextStrategy {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cag" => Ok(Self::Cag),
            "rag" => Ok(Self::Rag),
            other => Err(format!("unknown context strategy: {other}")),
        }
    }
}
```

- [ ] **Step 4: Add field to HiveConfig**

Add to `HiveConfig` struct after the `context_format` field (after line 397):

```rust
    /// Context assembly strategy: "auto" (default), "cag", or "rag".
    #[serde(default)]
    pub context_strategy: ContextStrategy,
```

Add to `Default for HiveConfig` (after `context_format: String::new(),` on line 470):

```rust
            context_strategy: ContextStrategy::Auto,
```

- [ ] **Step 5: Re-export from hive_core lib.rs**

In `hive/crates/hive_core/src/lib.rs` line 53, change:

```rust
pub use config::HiveConfig;
```

to:

```rust
pub use config::{ContextStrategy, HiveConfig};
```

Also add `cag_token_budget` to the context re-export on line 54-57:

```rust
pub use context::{
    CompactionResult, ContextMessage, ContextSummary, ContextWindow, cag_token_budget,
    estimate_tokens, model_context_size,
};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --package hive_core context_strategy`
Expected: All 4 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add hive/crates/hive_core/src/config.rs
git commit -m "feat(core): add ContextStrategy enum (Auto/Cag/Rag) to HiveConfig"
```

---

### Task 2: Add CAG threshold constant to hive_core context.rs

**Files:**
- Modify: `hive/crates/hive_core/src/context.rs:1-26`

This adds the token threshold used to auto-detect whether a project fits in the context window.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `context.rs`:

```rust
#[test]
fn cag_budget_for_model() {
    // Claude: 200k context → 60% = 120,000 tokens for CAG
    assert_eq!(cag_token_budget("claude-sonnet-4"), 120_000);
    // GPT-4o: 128k → 60% = 76,800
    assert_eq!(cag_token_budget("gpt-4o"), 76_800);
    // Unknown model: 8k → 60% = 4,800
    assert_eq!(cag_token_budget("unknown-model"), 4_800);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --package hive_core cag_budget`
Expected: FAIL — `cag_budget_for_model` does not exist.

- [ ] **Step 3: Implement the function**

Add after the `model_context_size()` function (after line 371) in `context.rs`:

```rust
/// Fraction of the model's context window available for CAG source loading.
/// The remaining 40% is reserved for system prompt, conversation history,
/// and model response.
const CAG_CONTEXT_FRACTION: f64 = 0.60;

/// Compute the token budget available for loading project sources in CAG mode.
///
/// Returns `model_context_size * 0.60` — the maximum number of tokens that
/// can be filled with source files before leaving room for the prompt,
/// conversation, and response.
pub fn cag_token_budget(model_id: &str) -> usize {
    let ctx = model_context_size(model_id);
    (ctx as f64 * CAG_CONTEXT_FRACTION) as usize
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package hive_core cag_budget`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_core/src/context.rs
git commit -m "feat(core): add cag_token_budget() for CAG context sizing"
```

---

## Chunk 2: ContextEngine `load_all()` — CAG Bypass

### Task 3: Add `load_all()` method to ContextEngine

**Files:**
- Modify: `hive/crates/hive_ai/src/context_engine.rs`

The `load_all()` method bypasses TF-IDF scoring and packs as many sources as possible into the budget, prioritized by source type: ProjectKnowledge first, then Config, then Files, then everything else.

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block in `context_engine.rs`:

```rust
#[test]
fn load_all_packs_by_type_priority() {
    let mut engine = ContextEngine::new();
    // Add a project knowledge source (highest priority)
    engine.add_project_knowledge("README", "# My Project\n\nA small tool.");
    // Add a config file
    engine.add_source(ContextSource {
        path: "Cargo.toml".into(),
        content: "[package]\nname = \"demo\"".into(),
        source_type: SourceType::Config,
        last_modified: Utc::now(),
    });
    // Add a regular file
    engine.add_file("src/main.rs", "fn main() { println!(\"hello\"); }");

    let budget = ContextBudget {
        max_tokens: 1000,
        max_sources: 100,
        reserved_tokens: 0,
    };
    let result = engine.load_all(&budget);

    // All 3 sources should fit in 1000 tokens
    assert_eq!(result.selected_count, 3);
    assert_eq!(result.original_count, 3);
    // Project knowledge should be first (highest priority)
    assert_eq!(result.sources[0].source_type, SourceType::ProjectKnowledge);
}

#[test]
fn load_all_respects_token_budget() {
    let mut engine = ContextEngine::new();
    // Each source ~250 tokens (1000 chars / 4)
    let big_content = "x".repeat(1000);
    for i in 0..10 {
        engine.add_file(&format!("file_{i}.rs"), &big_content);
    }
    // Total ~2500 tokens, budget is 600
    let budget = ContextBudget {
        max_tokens: 600,
        max_sources: 100,
        reserved_tokens: 0,
    };
    let result = engine.load_all(&budget);

    // Should fit only 2 files (2 × 250 = 500 < 600, 3 × 250 = 750 > 600)
    assert_eq!(result.selected_count, 2);
    assert!(result.total_tokens <= 600);
}

#[test]
fn load_all_empty_engine() {
    let mut engine = ContextEngine::new();
    let budget = ContextBudget::default();
    let result = engine.load_all(&budget);
    assert_eq!(result.selected_count, 0);
    assert_eq!(result.total_tokens, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package hive_ai load_all`
Expected: FAIL — `load_all` method does not exist.

- [ ] **Step 3: Implement `load_all()`**

Add this method to the `impl ContextEngine` block, after the `curate()` method (after line 296):

```rust
    /// Load all sources into the context without scoring, for CAG mode.
    ///
    /// Instead of TF-IDF scoring, sources are prioritized by type:
    /// 1. ProjectKnowledge (README, HIVE.md, etc.)
    /// 2. Config files (Cargo.toml, package.json, etc.)
    /// 3. Everything else (Files, Symbols, Tests, etc.)
    ///
    /// Within each priority tier, sources keep their insertion order.
    /// Sources are greedily packed until the token budget is exhausted.
    pub fn load_all(&self, budget: &ContextBudget) -> CuratedContext {
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

        // Priority order: ProjectKnowledge > Config > everything else.
        let priority = |st: SourceType| -> u8 {
            match st {
                SourceType::ProjectKnowledge => 0,
                SourceType::Config => 1,
                SourceType::LearnedPreference => 2,
                _ => 3,
            }
        };

        let mut indexed: Vec<(usize, &ContextSource)> =
            self.sources.iter().enumerate().collect();
        indexed.sort_by_key(|(_, s)| priority(s.source_type));

        let available_tokens = budget.max_tokens.saturating_sub(budget.reserved_tokens);
        let mut total_tokens = 0usize;
        let mut selected_sources = Vec::new();
        let mut selected_scores = Vec::new();

        for (idx, source) in &indexed {
            if selected_sources.len() >= budget.max_sources {
                break;
            }
            let tokens = self.estimate_source_tokens(source);
            if total_tokens + tokens > available_tokens {
                continue; // skip oversized, try smaller ones
            }
            total_tokens += tokens;
            selected_sources.push((*source).clone());
            selected_scores.push(RelevanceScore {
                source_idx: *idx,
                score: 1.0, // uniform score in CAG mode
                reasons: vec!["cag: full load".to_string()],
            });
        }

        let selected_count = selected_sources.len();
        debug!(
            "CAG load_all: packed {}/{} sources ({} tokens)",
            selected_count, original_count, total_tokens
        );

        CuratedContext {
            sources: selected_sources,
            scores: selected_scores,
            total_tokens,
            original_count,
            selected_count,
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package hive_ai load_all`
Expected: All 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ai/src/context_engine.rs
git commit -m "feat(ai): add ContextEngine::load_all() for CAG mode bypass"
```

---

## Chunk 3: QuickIndex Full Snapshot

### Task 4: Add `to_full_snapshot()` to QuickIndex

**Files:**
- Modify: `hive/crates/hive_ai/src/quick_index.rs`

This method reads actual file contents (not just symbols/metadata) and packs them into a context string up to a token budget. It is the heart of CAG — it produces the "entire codebase in context" payload.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `quick_index.rs`:

```rust
#[test]
fn full_snapshot_reads_file_contents() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("main.rs"), "fn main() {\n    println!(\"hello\");\n}").unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/util.rs"), "pub fn trim(s: &str) -> &str { s.trim() }").unwrap();

    let idx = QuickIndex::build(root);
    let snapshot = idx.to_full_snapshot(10_000); // 10k token budget

    // Should contain actual file contents
    assert!(snapshot.contains("fn main()"), "missing main.rs content");
    assert!(snapshot.contains("pub fn add"), "missing lib.rs content");
    assert!(snapshot.contains("pub fn trim"), "missing util.rs content");
}

#[test]
fn full_snapshot_respects_token_budget() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Each file ~250 tokens (1000 chars / 4)
    let big = "// padding\n".repeat(91); // ~1001 chars
    for i in 0..20 {
        std::fs::write(root.join(format!("file_{i}.rs")), &big).unwrap();
    }
    let idx = QuickIndex::build(root);
    let snapshot = idx.to_full_snapshot(1_000); // Only 1000 tokens budget

    // Should not contain all 20 files — budget caps it
    let file_count = snapshot.matches("```").count() / 2; // each file has open+close
    assert!(file_count < 20, "budget not enforced: got {file_count} files");
    // Rough token check: ~4 chars/token
    let est_tokens = snapshot.len().div_ceil(4);
    assert!(est_tokens <= 1_200, "snapshot too large: ~{est_tokens} tokens");
}

#[test]
fn full_snapshot_skips_binary_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();
    // Write a binary-looking file
    std::fs::write(root.join("image.png"), &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A]).unwrap();
    let idx = QuickIndex::build(root);
    let snapshot = idx.to_full_snapshot(10_000);

    assert!(snapshot.contains("fn main()"));
    assert!(!snapshot.contains("image.png"), "binary file should be skipped");
}

#[test]
fn full_snapshot_empty_project() {
    let dir = tempfile::tempdir().unwrap();
    let idx = QuickIndex::build(dir.path());
    let snapshot = idx.to_full_snapshot(10_000);
    // Should still have the header even if no files
    assert!(snapshot.contains("Project Snapshot"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package hive_ai full_snapshot`
Expected: FAIL — `to_full_snapshot` does not exist.

- [ ] **Step 3: Implement `to_full_snapshot()`**

Add this method to the `impl QuickIndex` block, after `to_context_string_xml()` (around line 420):

```rust
    /// Generate a full-content snapshot of the project for CAG mode.
    ///
    /// Unlike `to_context_string()` which only includes metadata and symbols,
    /// this method reads actual file contents and packs them into the output
    /// up to `max_tokens`. Files are prioritized:
    ///
    /// 1. Config/manifest files (Cargo.toml, package.json, etc.)
    /// 2. Recently modified files (by mtime, newest first)
    /// 3. Remaining files alphabetically
    ///
    /// Binary files and files larger than 100KB are skipped.
    /// Each file is wrapped in a fenced code block with its relative path.
    pub fn to_full_snapshot(&self, max_tokens: usize) -> String {
        use hive_core::context::estimate_tokens;
        use std::fs;

        let mut out = String::with_capacity(max_tokens * 4);
        out.push_str("# Project Snapshot (CAG)\n\n");
        out.push_str(&self.file_tree.summary);
        out.push('\n');

        // Collect all files from the project tree.
        let mut files: Vec<PathBuf> = Vec::new();
        Self::collect_files_recursive(&self.project_root, &mut files);

        // Classify into priority tiers.
        let config_names: HashSet<&str> = [
            "Cargo.toml", "package.json", "tsconfig.json", "pyproject.toml",
            "requirements.txt", "go.mod", "Makefile", "CMakeLists.txt",
            "HIVE.md", "README.md", ".claude.md",
        ].into_iter().collect();

        let mut config_files = Vec::new();
        let mut rest_files = Vec::new();

        for path in &files {
            let rel = path.strip_prefix(&self.project_root)
                .unwrap_or(path);
            let name = rel.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if config_names.contains(name) {
                config_files.push(path.clone());
            } else {
                rest_files.push(path.clone());
            }
        }

        // Sort rest by mtime (newest first), falling back to alphabetical.
        rest_files.sort_by(|a, b| {
            let mtime_a = fs::metadata(a).and_then(|m| m.modified()).ok();
            let mtime_b = fs::metadata(b).and_then(|m| m.modified()).ok();
            match (mtime_a, mtime_b) {
                (Some(a), Some(b)) => b.cmp(&a), // newest first
                _ => a.cmp(b),
            }
        });

        let ordered: Vec<PathBuf> = config_files.into_iter()
            .chain(rest_files)
            .collect();

        // Header consumes some tokens.
        let mut used_tokens = estimate_tokens(&out);
        let header_note = format!(
            "\n_Showing files from {} total. Strategy: CAG (full context load)._\n\n",
            self.file_tree.total_files
        );
        out.push_str(&header_note);
        used_tokens += estimate_tokens(&header_note);

        const MAX_FILE_SIZE: u64 = 100_000; // 100KB
        let mut files_included = 0usize;

        for path in &ordered {
            if used_tokens >= max_tokens {
                break;
            }
            // Skip binary or oversized files.
            let meta = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.len() > MAX_FILE_SIZE {
                continue;
            }
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue, // binary or unreadable
            };
            if content.is_empty() {
                continue;
            }

            let rel = path.strip_prefix(&self.project_root)
                .unwrap_or(path)
                .to_string_lossy();
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            // Build the block and check budget.
            let block = format!("## {rel}\n```{ext}\n{content}\n```\n\n");
            let block_tokens = estimate_tokens(&block);

            if used_tokens + block_tokens > max_tokens {
                // Try to fit a truncated version if >50% fits.
                let remaining = max_tokens.saturating_sub(used_tokens);
                if remaining > block_tokens / 2 {
                    let char_budget = remaining * 4; // ~4 chars/token
                    let truncated: String = content.chars().take(char_budget).collect();
                    let trunc_block = format!(
                        "## {rel} (truncated)\n```{ext}\n{truncated}\n```\n\n"
                    );
                    used_tokens += estimate_tokens(&trunc_block);
                    out.push_str(&trunc_block);
                    files_included += 1;
                }
                break;
            }

            used_tokens += block_tokens;
            out.push_str(&block);
            files_included += 1;
        }

        // Footer
        let remaining = self.file_tree.total_files.saturating_sub(files_included);
        if remaining > 0 {
            out.push_str(&format!(
                "\n_({remaining} files omitted — exceeded {max_tokens} token budget)_\n"
            ));
        }

        out
    }

    /// Recursively collect all non-hidden, non-skipped files.
    fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Skip hidden and known junk directories.
                if name.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    if SKIP_DIRS.contains(&name) {
                        continue;
                    }
                    Self::collect_files_recursive(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
    }
```

**Important:** `SKIP_DIRS` is already defined as a constant in `quick_index.rs` (line ~19). The `collect_files_recursive` helper reuses it. You **must** add `use std::collections::HashSet;` to the imports at the top of `quick_index.rs` (alongside the existing `use std::collections::HashMap;` on line 9), because `to_full_snapshot()` uses `HashSet<&str>` for the config file name lookup.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package hive_ai full_snapshot`
Expected: All 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ai/src/quick_index.rs
git commit -m "feat(ai): add QuickIndex::to_full_snapshot() for CAG mode"
```

---

## Chunk 4: Enable Prompt Caching

### Task 5: Enable prompt caching in `prepare_stream`

**Files:**
- Modify: `hive/crates/hive_ai/src/service.rs:473-492`

The Anthropic provider already fully supports prompt caching (`CacheControl` struct, system prompt array format, tool breakpoint caching). It's just never turned on because `cache_system_prompt` is hardcoded to `false` in `prepare_stream()`. We add a `cache_prompt` parameter.

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `service.rs`:

```rust
#[test]
fn prepare_stream_with_cache_prompt() {
    let svc = AiService::new(test_config());
    let messages = vec![ChatMessage::text(MessageRole::User, "Hello")];
    let result = svc.prepare_stream_cached(
        messages,
        "claude-opus-4-20250514",
        None,
        None,
        true, // cache_prompt
    );
    assert!(result.is_some());
    let (_provider, request) = result.unwrap();
    assert!(request.cache_system_prompt);
}

#[test]
fn prepare_stream_cached_false() {
    let svc = AiService::new(test_config());
    let messages = vec![ChatMessage::text(MessageRole::User, "Hello")];
    let result = svc.prepare_stream_cached(
        messages,
        "claude-opus-4-20250514",
        None,
        None,
        false,
    );
    assert!(result.is_some());
    let (_provider, request) = result.unwrap();
    assert!(!request.cache_system_prompt);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package hive_ai prepare_stream_cached`
Expected: FAIL — `prepare_stream_cached` does not exist.

- [ ] **Step 3: Implement `prepare_stream_cached()`**

Add a new method right after `prepare_stream()` (after line 492) in `service.rs`:

```rust
    /// Like `prepare_stream()` but allows enabling prompt caching.
    ///
    /// When `cache_prompt` is `true` and the resolved provider supports it
    /// (currently Anthropic), the system prompt and tool definitions are
    /// tagged with `cache_control: {"type": "ephemeral"}`, enabling the
    /// provider to cache and reuse them across requests.
    pub fn prepare_stream_cached(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        system_prompt: Option<String>,
        tools: Option<Vec<ToolDefinition>>,
        cache_prompt: bool,
    ) -> Option<(Arc<dyn AiProvider>, ChatRequest)> {
        let (_provider_type, provider, resolved_model) =
            self.resolve_provider_smart(&messages, model)?;
        let request = ChatRequest {
            messages,
            model: resolved_model,
            max_tokens: 4096,
            temperature: None,
            system_prompt,
            tools,
            cache_system_prompt: cache_prompt,
        };
        Some((provider, request))
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package hive_ai prepare_stream_cached`
Expected: Both tests PASS.

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ai/src/service.rs
git commit -m "feat(ai): add prepare_stream_cached() with prompt caching support"
```

---

## Chunk 5: Wire CAG Into the Chat Pipeline

### Task 6: Add `should_use_cag()` detection logic

**Files:**
- Modify: `hive/crates/hive_ai/src/quick_index.rs`

Add a method that checks whether the project is small enough for CAG mode.

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `quick_index.rs`:

```rust
#[test]
fn estimate_project_tokens_basic() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Write 5 files, each 100 chars (~25 tokens each) = ~125 total
    for i in 0..5 {
        std::fs::write(root.join(format!("f{i}.rs")), "x".repeat(100)).unwrap();
    }
    let idx = QuickIndex::build(root);
    let est = idx.estimate_project_tokens();
    // 5 files × 25 tokens = 125, with some tolerance for overhead
    assert!(est >= 100 && est <= 200, "got {est}");
}

#[test]
fn should_use_cag_small_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();
    let idx = QuickIndex::build(root);
    // Tiny project, 120k budget — should be CAG
    assert!(idx.fits_in_cag_budget(120_000));
}

#[test]
fn should_use_cag_large_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Write many files that exceed a small budget
    let big = "x".repeat(4000); // ~1000 tokens each
    for i in 0..200 {
        std::fs::write(root.join(format!("file_{i}.rs")), &big).unwrap();
    }
    let idx = QuickIndex::build(root);
    // 200 files × 1000 tokens = 200k — won't fit in 120k
    assert!(!idx.fits_in_cag_budget(120_000));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --package hive_ai should_use_cag estimate_project_tokens`
Expected: FAIL — methods do not exist.

- [ ] **Step 3: Implement the methods**

Add to the `impl QuickIndex` block in `quick_index.rs`:

```rust
    /// Estimate the total token count of all readable source files in the project.
    ///
    /// This walks the file tree (respecting SKIP_DIRS) and sums up
    /// `estimate_tokens()` for every readable text file under 100KB.
    /// Binary files are skipped.
    pub fn estimate_project_tokens(&self) -> usize {
        use hive_core::context::estimate_tokens;

        let mut files = Vec::new();
        Self::collect_files_recursive(&self.project_root, &mut files);

        let mut total = 0usize;
        for path in &files {
            let meta = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.len() > 100_000 {
                continue;
            }
            // Use file size as a proxy (avoids reading every file).
            // ~4 chars/token, and most source files are UTF-8 single-byte.
            total += (meta.len() as usize).div_ceil(4);
        }
        total
    }

    /// Check whether this project's source files fit within a CAG token budget.
    ///
    /// Returns `true` if the estimated total project tokens are ≤ `budget`.
    /// Used by the Auto strategy to decide between CAG and RAG.
    pub fn fits_in_cag_budget(&self, budget: usize) -> bool {
        self.estimate_project_tokens() <= budget
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package hive_ai should_use_cag estimate_project_tokens`
Expected: All 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add hive/crates/hive_ai/src/quick_index.rs
git commit -m "feat(ai): add QuickIndex::estimate_project_tokens() and fits_in_cag_budget()"
```

---

### Task 7: Wire CAG mode into workspace.rs context assembly

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs`

This is the integration task. The context assembly pipeline lives in a `let ai_messages = { ... };` block (lines 2808–3098) that returns `augmented`. We need to:
1. Compute `use_cag` **before** line 2808 so it's available both inside the block and later at line 3232 for `prepare_stream_cached`.
2. Inside the block: skip RAG/SemanticSearch/ContextEngine when CAG, and use `to_full_snapshot()` instead of `to_context_string()` for the QuickIndex section.
3. After the block: use `prepare_stream_cached` instead of `prepare_stream` when CAG.

**Current code structure (for reference):**
```
Line 2805: let ai_messages = self.chat_service.read(cx).build_ai_messages();
Line 2808: let ai_messages = {                    ← block start
Line 2811-2826:   RAG query
Line 2828-2878:   Semantic search
Line 2886-2920:   ContextEngine curate
Line 2922:        let mut augmented = ai_messages.clone();
Line 2926-2949:   Knowledge files injection
Line 2964-2990:   QuickIndex injection
Line 2997-3012:   Recalled memories injection
Line 3014-3030:   Retrieved context injection
Line 3032-3095:   Selected context files injection
Line 3097:        augmented                        ← block returns
Line 3098: };
...
Line 3232-3240:   prepare_stream call
```

- [ ] **Step 1: Read the current context assembly code**

Read `workspace.rs` lines 2795–3250 to understand the complete flow.

- [ ] **Step 2: Add strategy detection helper**

Add this as a free function at module level in `workspace.rs` (e.g., before the `impl Workspace` block or at the end of the file outside impl blocks):

```rust
/// Determine the effective context strategy based on config and project size.
fn resolve_context_strategy(
    strategy: hive_core::ContextStrategy,
    quick_index: Option<&hive_ai::QuickIndex>,
    model: &str,
) -> hive_core::ContextStrategy {
    use hive_core::ContextStrategy;

    match strategy {
        ContextStrategy::Cag | ContextStrategy::Rag => strategy,
        ContextStrategy::Auto => {
            if let Some(qi) = quick_index {
                let budget = hive_core::cag_token_budget(model);
                if qi.fits_in_cag_budget(budget) {
                    tracing::info!(
                        "Auto context strategy: CAG (project fits in {} token budget)",
                        budget
                    );
                    ContextStrategy::Cag
                } else {
                    tracing::info!(
                        "Auto context strategy: RAG (project exceeds {} token budget)",
                        budget
                    );
                    ContextStrategy::Rag
                }
            } else {
                ContextStrategy::Rag
            }
        }
    }
}
```

- [ ] **Step 3: Compute `use_cag` before the context assembly block**

Insert between line 2805 (`let ai_messages = ...build_ai_messages()`) and line 2808 (`let ai_messages = {`):

```rust
        // --- Context strategy resolution (before the block so use_cag is in scope later) ---
        let use_cag = {
            let config_strategy = if cx.has_global::<AppConfig>() {
                cx.global::<AppConfig>().0.get().context_strategy
            } else {
                hive_core::ContextStrategy::Auto
            };
            let qi_ref = if cx.has_global::<AppQuickIndex>() {
                Some(cx.global::<AppQuickIndex>().0.clone())
            } else {
                None
            };
            let effective = resolve_context_strategy(
                config_strategy,
                qi_ref.as_deref(),
                &model,
            );
            tracing::info!(
                strategy = %effective,
                "Context strategy resolved for chat request"
            );
            effective == hive_core::ContextStrategy::Cag
        };
```

- [ ] **Step 4: Wrap RAG/SemanticSearch/ContextEngine in `if !use_cag`**

Inside the `let ai_messages = {` block, wrap lines 2811–2920 (the RAG query, semantic search, and ContextEngine curation) in a conditional. The `all_context` variable still needs to exist in both paths.

Replace lines 2808–2920 with:

```rust
        let ai_messages = {
            let mut all_context = String::new();

            if !use_cag {
                // --- RAG path: existing retrieval pipeline ---

                // Pull from RAG document chunks
                if cx.has_global::<AppRagService>() {
                    // ... existing lines 2812-2826, unchanged ...
                }

                // Semantic search
                if cx.has_global::<AppSemanticSearch>() {
                    // ... existing lines 2828-2878, unchanged ...
                }

                let memory_context = String::new();

                // ContextEngine curation
                if cx.has_global::<AppContextEngine>() {
                    // ... existing lines 2886-2920, unchanged ...
                }
            } else {
                // --- CAG path: no retrieval needed, full snapshot injected below ---
            }

            // NOTE: The `memory_context` variable is only used in the RAG path
            // (line 2883). In CAG mode it stays empty. Define it here for both paths:
            let memory_context = if use_cag { String::new() } else { String::new() };
```

**Important:** Keep everything from line 2922 onward (`let mut augmented = ...`) unchanged, EXCEPT the QuickIndex injection section (lines 2964-2990).

- [ ] **Step 5: Replace QuickIndex injection with CAG-aware version**

Replace lines 2964–2990 (the QuickIndex injection block) with:

```rust
            // Inject project context: full snapshot (CAG) or lightweight index (RAG).
            if cx.has_global::<AppQuickIndex>() {
                let quick_ctx = if use_cag {
                    // CAG mode: load actual file contents up to the model's budget.
                    let cag_budget = hive_core::cag_token_budget(&model);
                    cx.global::<AppQuickIndex>().0.to_full_snapshot(cag_budget)
                } else {
                    // RAG mode: lightweight metadata-only index.
                    match ctx_format {
                        hive_ai::ContextFormat::Toon => {
                            cx.global::<AppQuickIndex>().0.to_context_string_toon()
                        }
                        hive_ai::ContextFormat::Xml => {
                            cx.global::<AppQuickIndex>().0.to_context_string_xml()
                        }
                        _ => cx.global::<AppQuickIndex>().0.to_context_string(),
                    }
                };
                if !quick_ctx.trim().is_empty() {
                    let qi_idx = augmented
                        .iter()
                        .position(|m| m.role != hive_ai::types::MessageRole::System)
                        .unwrap_or(0);
                    augmented.insert(
                        qi_idx,
                        hive_ai::types::ChatMessage {
                            role: hive_ai::types::MessageRole::System,
                            content: quick_ctx,
                            timestamp: chrono::Utc::now(),
                            tool_call_id: None,
                            tool_calls: None,
                        },
                    );
                }
            }
```

Everything else in the block (knowledge files, memories, retrieved context, selected files, `augmented` return) stays unchanged.

- [ ] **Step 6: Enable prompt caching at `prepare_stream`**

Replace lines 3232–3240 (the `prepare_stream` call) with:

```rust
        let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> = if cx
            .has_global::<AppAiService>()
        {
            if use_cag {
                // CAG: enable Anthropic prompt caching for the large context payload.
                cx.global::<AppAiService>()
                    .0
                    .prepare_stream_cached(
                        ai_messages.clone(),
                        &model,
                        system_prompt.clone(),
                        Some(tool_defs.clone()),
                        true,
                    )
            } else {
                cx.global::<AppAiService>()
                    .0
                    .prepare_stream(
                        ai_messages.clone(),
                        &model,
                        system_prompt.clone(),
                        Some(tool_defs.clone()),
                    )
            }
        } else {
            None
        };
```

- [ ] **Step 7: Build and verify compilation**

Run: `cargo build --package hive_ui`
Expected: Compiles without errors.

- [ ] **Step 8: Commit**

```bash
git add hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(ui): wire CAG mode into workspace context assembly pipeline"
```

---

### Task 8: Add status bar indicator for context strategy

**Files:**
- Modify: `hive/crates/hive_ui/src/statusbar.rs:10-21` (StatusBar struct)
- Modify: `hive/crates/hive_ui/src/statusbar.rs:48-66` (Default impl)
- Modify: `hive/crates/hive_ui/src/statusbar.rs:68-78` (render method)
- Modify: `hive/crates/hive_ui/src/workspace.rs` (set the field)

The `StatusBar` struct (statusbar.rs:10) has fields like `current_model: String`, `privacy_mode: bool`, etc. We add a `context_strategy: String` field shown as a small badge next to the model name.

- [ ] **Step 1: Add field to StatusBar struct**

In `hive/crates/hive_ui/src/statusbar.rs`, add after line 16 (`pub version: String,`):

```rust
    /// Active context strategy label (e.g. "CAG", "RAG", or empty for auto).
    pub context_strategy: String,
```

- [ ] **Step 2: Add default value**

In the `Default for StatusBar` impl (line 48-66), add after `version`:

```rust
            context_strategy: String::new(),
```

- [ ] **Step 3: Show badge in render**

In the `render()` method (line 68), after the model label is computed (line 71-78), build a strategy badge:

```rust
        let strategy_badge = if self.context_strategy.is_empty() {
            String::new()
        } else {
            format!(" [{}]", self.context_strategy)
        };
```

Then append it to the model display string. Find where `model` is used in the rendered output and change it to `format!("{model}{strategy_badge}")`.

- [ ] **Step 4: Set the field from workspace.rs**

In workspace.rs, in the context assembly section (after the `use_cag` computation in Task 7, Step 3), set the status bar field:

```rust
        self.status_bar.context_strategy = if use_cag {
            "CAG".to_string()
        } else {
            String::new() // don't show badge for default RAG mode
        };
```

- [ ] **Step 5: Build and verify**

Run: `cargo build --package hive_ui`
Expected: Compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add hive/crates/hive_ui/src/statusbar.rs hive/crates/hive_ui/src/workspace.rs
git commit -m "feat(ui): show CAG strategy badge in status bar"
```

---

## Chunk 6: Integration Test

### Task 9: End-to-end test with mock project

**Files:**
- Create: `hive/crates/hive_ai/tests/cag_integration.rs`

- [ ] **Step 1: Write integration test**

```rust
//! Integration test: CAG mode with a small mock project.

use hive_ai::{ContextBudget, ContextEngine, QuickIndex};
use hive_core::context::cag_token_budget;

#[test]
fn cag_full_pipeline_small_project() {
    // 1. Create a temp project with a few small files.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.1.0\"").unwrap();
    // Create src/ BEFORE writing files into it.
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {\n    greet();\n}\n").unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn greet() { println!(\"hi\"); }").unwrap();

    // 2. Build QuickIndex.
    let qi = QuickIndex::build(root);

    // 3. Auto-detect: should pick CAG for this tiny project.
    let budget = cag_token_budget("claude-sonnet-4"); // 120,000 tokens
    assert!(qi.fits_in_cag_budget(budget), "tiny project should fit in CAG budget");

    // 4. Generate full snapshot.
    let snapshot = qi.to_full_snapshot(budget);
    assert!(snapshot.contains("fn main()"));
    assert!(snapshot.contains("pub fn greet()"));
    assert!(snapshot.contains("[package]"));

    // 5. Also test ContextEngine load_all with the same sources.
    let mut engine = ContextEngine::new();
    engine.add_file("Cargo.toml", "[package]\nname = \"demo\"");
    engine.add_file("src/main.rs", "fn main() { greet(); }");
    engine.add_file("src/lib.rs", "pub fn greet() { println!(\"hi\"); }");

    let ce_budget = ContextBudget {
        max_tokens: budget,
        max_sources: 1000,
        reserved_tokens: 0,
    };
    let curated = engine.load_all(&ce_budget);
    assert_eq!(curated.selected_count, 3);
    assert!(curated.total_tokens < 100); // tiny files
}

#[test]
fn large_project_exceeds_cag_budget() {
    // Create a project large enough to exceed a realistic CAG budget.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // 500 files × 4000 chars ≈ 500,000 tokens — exceeds Claude's 120k CAG budget.
    let big = "x".repeat(4000);
    for i in 0..500 {
        std::fs::write(root.join(format!("file_{i}.rs")), &big).unwrap();
    }

    let qi = QuickIndex::build(root);
    // Should NOT fit in Claude's CAG budget (120k tokens).
    assert!(
        !qi.fits_in_cag_budget(cag_token_budget("claude-sonnet-4")),
        "500-file project should exceed CAG budget"
    );
    // Should also not fit in a tiny budget.
    assert!(!qi.fits_in_cag_budget(1_000));
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --package hive_ai --test cag_integration`
Expected: Both tests PASS.

- [ ] **Step 3: Commit**

```bash
git add hive/crates/hive_ai/tests/cag_integration.rs
git commit -m "test: add CAG integration tests for full pipeline"
```

---

## Summary of All Changes

| File | Change | Lines Added |
|------|--------|-------------|
| `hive_core/src/config.rs` | `ContextStrategy` enum + HiveConfig field | ~60 |
| `hive_core/src/context.rs` | `cag_token_budget()` function | ~15 |
| `hive_core/src/lib.rs` | Re-export `ContextStrategy` + `cag_token_budget` | ~3 |
| `hive_ai/src/context_engine.rs` | `load_all()` method | ~55 |
| `hive_ai/src/quick_index.rs` | `to_full_snapshot()`, `estimate_project_tokens()`, `fits_in_cag_budget()`, `collect_files_recursive()` | ~140 |
| `hive_ai/src/service.rs` | `prepare_stream_cached()` method | ~20 |
| `hive_ui/src/statusbar.rs` | `context_strategy` field + badge render | ~10 |
| `hive_ui/src/workspace.rs` | CAG strategy detection, pipeline branching, prompt caching, status bar set | ~60 |
| `hive_ai/tests/cag_integration.rs` | Integration tests (new file) | ~60 |

**Total new code: ~420 lines** (plus ~120 lines of tests across unit + integration)
