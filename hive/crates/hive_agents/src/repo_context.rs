//! Repository-context assembly for the headless `build-ticket` swarm.
//!
//! The `build_from_ticket` flow runs the Queen swarm on a free-text objective
//! and then parses the swarm's output with
//! [`crate::response_parser::parse_edits`], committing whatever full-file edits
//! it finds. Historically the objective was *just* the ticket title + body and
//! the swarm had no view of the repository, so it produced blind / hallucinated
//! edits.
//!
//! This module grounds the swarm: given a repo path and the ticket text, it
//! walks the repository, ranks source files by relevance to the ticket, and
//! emits a `## Repository context` markdown block (each file as a fenced code
//! block) that is injected into the objective. The swarm can then rewrite real
//! files instead of inventing them.
//!
//! Design constraints:
//! * **Self-contained** — only the gitignore-aware [`ignore::WalkBuilder`] (a
//!   workspace dep, also used by `hive_fs`) and [`hive_fs::is_likely_binary`]
//!   are reused. No heavy indexing crates are pulled in.
//! * **Defensive** — never panics. Unreadable files/dirs are skipped, not
//!   fatal. Output is bounded by an explicit token budget plus per-file and
//!   walk-wide caps, so a giant repo can never blow up memory or the prompt.

use std::path::Path;

use hive_fs::is_likely_binary;
use ignore::WalkBuilder;

/// Hard skip-list of directory names that never contain source worth feeding to
/// the swarm (build artifacts, VCS internals, vendored deps, our own worktrees).
/// `.git`, `target`, `node_modules`, `dist`, `build`, `.hive-worktrees`.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".hive-worktrees",
];

/// Skip any single file larger than this (bytes). Large files are almost never
/// the thing a ticket edits and they wreck the budget. ~256 KB.
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// When a selected file is large, truncate its embedded content to this many
/// bytes so one big-but-relevant file can't consume the whole budget.
const PER_FILE_CONTENT_CAP: usize = 24 * 1024;

/// Upper bound on how many candidate files we read while walking, so a repo
/// with hundreds of thousands of files stays bounded regardless of budget.
const MAX_CANDIDATES: usize = 4000;

/// Rough bytes-per-token estimate used to convert the token budget into a byte
/// budget for the assembled block (`tokens ≈ bytes / 4`).
const BYTES_PER_TOKEN: usize = 4;

/// Default token budget for the repository-context block (~14K tokens).
pub const DEFAULT_TOKEN_BUDGET: usize = 14_000;

/// A candidate source file discovered during the walk, with its relevance
/// score and (already-read) content.
struct Candidate {
    /// Path relative to the repo root, using `/` separators.
    rel_path: String,
    /// File content (UTF-8). Read once during the walk.
    content: String,
    /// Relevance score against the ticket tokens (higher = more relevant).
    score: u32,
}

/// English stopwords + a few code-noise tokens dropped before scoring so common
/// words don't dominate the keyword overlap.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "from", "into", "you", "your", "are", "but",
    "not", "all", "any", "can", "has", "have", "was", "were", "will", "would", "should", "could",
    "when", "what", "which", "while", "then", "than", "they", "them", "their", "there", "here",
    "fix", "add", "use", "using", "make", "new", "get", "set", "via",
];

/// Tokenize `text` into lowercase alphanumeric terms, dropping stopwords and
/// short (<= 2 char) tokens. Used for both the ticket and file content/paths.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// Build the unique set of ticket keywords used for scoring.
fn ticket_keywords(ticket_text: &str) -> Vec<String> {
    let mut kws = tokenize(ticket_text);
    kws.sort();
    kws.dedup();
    kws
}

/// Score a candidate file against the ticket keywords.
///
/// * A keyword appearing anywhere in the (relative) PATH is weighted heavily
///   (path hits are a strong signal — e.g. ticket "fix auth login" vs
///   `src/auth.rs`).
/// * Each occurrence of a keyword in the CONTENT adds a smaller amount, capped
///   per keyword so one keyword repeated thousands of times can't dominate.
fn score_file(rel_path: &str, content: &str, keywords: &[String]) -> u32 {
    if keywords.is_empty() {
        return 0;
    }
    let path_lower = rel_path.to_ascii_lowercase();
    // Tokenize content once; build a quick frequency view.
    let content_tokens = tokenize(content);

    let mut score: u32 = 0;
    for kw in keywords {
        // Path weight: substring match anywhere in the path is a strong signal.
        if path_lower.contains(kw.as_str()) {
            score = score.saturating_add(50);
        }
        // Content weight: term frequency, capped so it can't dominate.
        let freq = content_tokens.iter().filter(|t| *t == kw).count();
        if freq > 0 {
            let capped = freq.min(20) as u32;
            score = score.saturating_add(capped * 2);
        }
    }
    score
}

/// Assemble a `## Repository context` markdown block for the swarm objective.
///
/// Walks `repo_path` (gitignore-aware, hidden dirs and [`SKIP_DIRS`] excluded,
/// binary / oversized files skipped), ranks the readable text files by keyword
/// overlap with `ticket_text`, and selects the highest-scoring files whose
/// combined (possibly truncated) content fits within `token_budget` (estimated
/// as `bytes / 4`).
///
/// Returns an empty string when nothing relevant is found (no readable files,
/// or no file shares any keyword with the ticket), so callers can cheaply gate
/// on `is_empty()`.
///
/// Never panics: any walk / read error simply skips the offending entry.
pub fn assemble_repo_context(repo_path: &Path, ticket_text: &str, token_budget: usize) -> String {
    let keywords = ticket_keywords(ticket_text);
    if keywords.is_empty() {
        return String::new();
    }

    // --- Walk + collect scored candidates --------------------------------
    let mut candidates: Vec<Candidate> = Vec::new();

    let walker = WalkBuilder::new(repo_path)
        .hidden(true) // skip hidden files/dirs (.git etc.)
        .git_ignore(true) // honor .gitignore if present
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(|entry| {
            // Prune skip-listed directories early so we never descend into them.
            if entry.file_type().is_some_and(|ft| ft.is_dir())
                && let Some(name) = entry.file_name().to_str()
            {
                return !SKIP_DIRS.contains(&name);
            }
            true
        })
        .build();

    for entry in walker {
        if candidates.len() >= MAX_CANDIDATES {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // unreadable dir/file -> skip, not fatal
        };
        // Files only.
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();

        // Size cap (skip oversized files).
        match path.metadata() {
            Ok(md) if md.len() > MAX_FILE_BYTES => continue,
            Ok(_) => {}
            Err(_) => continue,
        }

        // Skip binaries (null-byte heuristic from hive_fs).
        if is_likely_binary(path) {
            continue;
        }

        // Read as UTF-8; skip anything that isn't.
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Relative path with forward slashes for stable display + scoring.
        let rel_path = match path.strip_prefix(repo_path) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => path.to_string_lossy().replace('\\', "/"),
        };
        if rel_path.is_empty() {
            continue;
        }

        let score = score_file(&rel_path, &content, &keywords);
        if score == 0 {
            continue; // irrelevant to the ticket
        }

        candidates.push(Candidate {
            rel_path,
            content,
            score,
        });
    }

    if candidates.is_empty() {
        return String::new();
    }

    // --- Rank: score desc, then shorter path, then path asc for stability -
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.rel_path.len().cmp(&b.rel_path.len()))
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });

    // --- Select within the byte budget -----------------------------------
    let byte_budget = token_budget.saturating_mul(BYTES_PER_TOKEN);
    let mut out = String::from("## Repository context\n\n");
    out.push_str(
        "The following files from the target repository are provided as grounding. \
         They are the current, real contents of these files:\n\n",
    );
    let header_len = out.len();
    let mut used = 0usize;

    for cand in &candidates {
        let (display_content, truncated) = if cand.content.len() > PER_FILE_CONTENT_CAP {
            // Truncate on a char boundary at or below the cap.
            let mut end = PER_FILE_CONTENT_CAP;
            while end > 0 && !cand.content.is_char_boundary(end) {
                end -= 1;
            }
            (&cand.content[..end], true)
        } else {
            (cand.content.as_str(), false)
        };

        let lang = lang_for(&cand.rel_path);
        let mut block = String::new();
        block.push_str(&format!("### {}\n\n", cand.rel_path));
        block.push_str(&format!("```{lang}\n"));
        block.push_str(display_content);
        if !display_content.ends_with('\n') {
            block.push('\n');
        }
        if truncated {
            block.push_str("/* ...truncated... */\n");
        }
        block.push_str("```\n\n");

        // Budget check: always allow at least the single most-relevant file
        // (when `used == 0` nothing has been added yet) so the block is never
        // empty when there *is* a relevant file.
        if used > 0 && used + block.len() > byte_budget {
            break;
        }
        out.push_str(&block);
        used += block.len();
    }

    if out.len() == header_len {
        // Nothing actually included (shouldn't happen given candidates > 0, but
        // be defensive): return empty so callers gate cleanly.
        return String::new();
    }

    out
}

/// Best-effort language hint for a fenced block from the file extension. Only
/// affects rendering of the prompt; not load-bearing for parsing.
fn lang_for(rel_path: &str) -> &'static str {
    let ext = rel_path.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "rust",
        "ts" | "tsx" => "ts",
        "js" | "jsx" | "mjs" | "cjs" => "js",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "rb" => "ruby",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "sh" | "bash" => "bash",
        "html" => "html",
        "css" => "css",
        "sql" => "sql",
        _ => "",
    }
}

/// The explicit instruction block appended after the repo context, telling the
/// swarm to ground every change in the shown files and emit COMPLETE-file edits
/// in the exact formats [`crate::response_parser::parse_edits`] understands.
pub const EDIT_FORMAT_INSTRUCTION: &str = "\
You are modifying THIS repository. Base every change strictly on the files \
shown above; do NOT invent the contents of files you have not seen. For each \
file you change or create, output the COMPLETE new file content as a fenced \
block exactly like:\n\
```<lang>:<relative/path>\n\
<full file content>\n\
```\n\
(or <edit path=\"<relative/path>\">full content</edit>). Keep changes minimal \
and focused on the ticket.";

/// Build the grounded swarm objective: the ticket text, then the assembled
/// repository-context block (if any), then the [`EDIT_FORMAT_INSTRUCTION`].
///
/// When no repository context can be assembled (no relevant files), the ticket
/// text and the edit-format instruction are still returned so the swarm is at
/// least told which output format to use.
pub fn build_grounded_objective(
    repo_path: &Path,
    ticket_text: &str,
    token_budget: usize,
) -> String {
    let context = assemble_repo_context(repo_path, ticket_text, token_budget);
    let mut objective = String::new();
    objective.push_str(ticket_text.trim());
    objective.push_str("\n\n");
    if !context.is_empty() {
        objective.push_str(&context);
        objective.push('\n');
    }
    objective.push_str(EDIT_FORMAT_INSTRUCTION);
    objective
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Temp repo with two source files of differing relevance to an auth ticket.
    fn auth_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/auth.rs"),
            "// authentication\npub fn login(user: &str, password: &str) -> bool {\n    \
             authenticate(user, password)\n}\nfn authenticate(_u: &str, _p: &str) -> bool { true }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/ui.rs"),
            "// rendering\npub fn render_button(label: &str) -> String {\n    \
             format!(\"<button>{label}</button>\")\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn selects_ticket_relevant_file() {
        let dir = auth_repo();
        let ctx = assemble_repo_context(dir.path(), "fix auth login", DEFAULT_TOKEN_BUDGET);
        assert!(ctx.contains("## Repository context"));
        assert!(ctx.contains("src/auth.rs"), "auth.rs must be included");
        // The login function content should be embedded for grounding.
        assert!(ctx.contains("pub fn login"));
    }

    #[test]
    fn ranks_relevant_file_before_irrelevant() {
        let dir = auth_repo();
        let ctx = assemble_repo_context(dir.path(), "fix auth login", DEFAULT_TOKEN_BUDGET);
        let auth_pos = ctx.find("src/auth.rs");
        let ui_pos = ctx.find("src/ui.rs");
        // auth.rs must appear, and if ui.rs appears at all it must come AFTER.
        assert!(auth_pos.is_some(), "auth.rs must be present");
        if let Some(ui) = ui_pos {
            assert!(
                auth_pos.unwrap() < ui,
                "auth.rs should be ranked before ui.rs"
            );
        }
    }

    #[test]
    fn respects_token_budget() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        // Many files all mentioning the keyword, each a few KB.
        let body = "alpha ".repeat(800); // ~4.8 KB each
        for i in 0..50 {
            fs::write(
                dir.path().join(format!("src/file_{i}_alpha.rs")),
                format!("// alpha module {i}\n{body}\n"),
            )
            .unwrap();
        }
        // Tiny budget: 500 tokens => ~2000 bytes. Output must stay bounded and
        // cannot contain all 50 files.
        let ctx = assemble_repo_context(dir.path(), "alpha refactor", 500);
        assert!(!ctx.is_empty());
        let included = ctx.matches("### src/file_").count();
        assert!(
            included < 50,
            "budget must bound the number of included files (got {included})"
        );
        // Bounded byte size: header + ~one file + truncation slack.
        assert!(
            ctx.len() < 64 * 1024,
            "assembled context must stay bounded (was {} bytes)",
            ctx.len()
        );
    }

    #[test]
    fn skips_excluded_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::write(
            dir.path().join("src/auth.rs"),
            "pub fn login() {}\n// auth\n",
        )
        .unwrap();
        // A file under target/ that is HIGHLY relevant by keywords — must still
        // be excluded because target/ is skipped.
        fs::write(
            dir.path().join("target/auth.rs"),
            "pub fn login() {} // auth auth auth login login login\n",
        )
        .unwrap();
        let ctx = assemble_repo_context(dir.path(), "fix auth login", DEFAULT_TOKEN_BUDGET);
        assert!(ctx.contains("src/auth.rs"));
        assert!(
            !ctx.contains("target/auth.rs"),
            "files under target/ must never be included"
        );
    }

    #[test]
    fn skips_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        // A 300 KB file (> MAX_FILE_BYTES) full of the keyword: must be skipped.
        let big = "alpha\n".repeat(60_000); // ~360 KB
        fs::write(dir.path().join("src/big_alpha.rs"), big).unwrap();
        fs::write(
            dir.path().join("src/small_alpha.rs"),
            "// alpha\nfn alpha() {}\n",
        )
        .unwrap();
        let ctx = assemble_repo_context(dir.path(), "alpha work", DEFAULT_TOKEN_BUDGET);
        assert!(!ctx.contains("big_alpha.rs"), "oversized file must be skipped");
        assert!(ctx.contains("small_alpha.rs"));
    }

    #[test]
    fn empty_when_nothing_relevant() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/ui.rs"), "fn render() {}\n").unwrap();
        // Ticket keywords share nothing with the file path/content.
        let ctx = assemble_repo_context(dir.path(), "database migration schema", DEFAULT_TOKEN_BUDGET);
        assert!(ctx.is_empty(), "no relevant files => empty context");
    }

    #[test]
    fn empty_when_ticket_has_no_keywords() {
        let dir = auth_repo();
        // Only stopwords / short tokens => no keywords => empty.
        let ctx = assemble_repo_context(dir.path(), "the and a to", DEFAULT_TOKEN_BUDGET);
        assert!(ctx.is_empty());
    }

    #[test]
    fn grounded_objective_contains_ticket_context_and_instruction() {
        let dir = auth_repo();
        let ticket = "fix auth login bug";
        let obj = build_grounded_objective(dir.path(), ticket, DEFAULT_TOKEN_BUDGET);
        // 1. ticket text
        assert!(obj.contains("fix auth login bug"));
        // 2. repository context block
        assert!(obj.contains("## Repository context"));
        assert!(obj.contains("src/auth.rs"));
        // 3. edit-format instruction
        assert!(obj.contains("You are modifying THIS repository"));
        assert!(obj.contains("```<lang>:<relative/path>"));
        assert!(obj.contains("<edit path="));
    }

    #[test]
    fn grounded_objective_without_context_still_has_instruction() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/ui.rs"), "fn render() {}\n").unwrap();
        // No relevant files => no context block, but ticket + instruction remain.
        let obj = build_grounded_objective(dir.path(), "database schema migration", DEFAULT_TOKEN_BUDGET);
        assert!(obj.contains("database schema migration"));
        assert!(!obj.contains("## Repository context"));
        assert!(obj.contains("You are modifying THIS repository"));
    }

    #[test]
    fn never_panics_on_missing_repo() {
        // A path that does not exist must yield empty context, not a panic.
        let ctx = assemble_repo_context(
            Path::new("/nonexistent/path/for/hive/test"),
            "anything relevant",
            DEFAULT_TOKEN_BUDGET,
        );
        assert!(ctx.is_empty());
    }
}
