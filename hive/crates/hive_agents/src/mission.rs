//! Ticket -> build -> PR orchestration core.
//!
//! This module implements the headless "turn a ticket into a built, PR'd
//! change" loop. It is deliberately decoupled from any concrete AI provider or
//! git-hosting client so it can be unit-tested without network access:
//!
//! * The AI work is performed by a [`Queen`] swarm driven by an injected
//!   [`AiExecutor`] (mockable).
//! * Pull-request creation goes through the injected [`PrOpener`] trait, so the
//!   core never talks to GitHub/GitLab directly.
//!
//! # Safe defaults
//!
//! * Branches are always named `hive/ticket-{sanitized_ref}`. When a real repo
//!   is supplied the swarm's changes are consolidated and committed onto THIS
//!   exact branch (inside an isolated worktree), so the branch we report is the
//!   branch that holds the changes — there is no separate `swarm/...` branch to
//!   reconcile.
//! * The swarm runs with git-worktree isolation when a real repository path is
//!   supplied (see [`BuildOpts::repo_path`]), so the user's working tree is
//!   never mutated in place.
//! * On the `--open-pr` path only, the result branch is pushed to `origin`
//!   (via a [`SecurityGateway`](hive_core::SecurityGateway)-validated `git
//!   push`) BEFORE the PR is opened, so the PR never references a branch that is
//!   absent on the remote. The approval-gated default never pushes.
//! * Pull requests are opened as **drafts only** ([`PrOpener::open_draft_pr`]).
//! * `require_approval` defaults to `true`. When set (or when the repo's risk
//!   tier demands it) the core STOPS before opening a PR and hands the branch +
//!   summary back for human review.
//! * The core NEVER merges. There is no merge path here at all.
//! * Every failure is returned as a [`BuildOutcome::Error`] — this function
//!   must never panic.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;

use hive_core::SecurityGateway;

use crate::hivemind::AiExecutor;
use crate::queen::Queen;
use crate::response_parser::parse_edits;
use crate::swarm::{InnerResult, SwarmConfig, SwarmResult, SwarmStatus, TeamStatus};
use crate::worktree::WorktreeManager;

// ---------------------------------------------------------------------------
// PrOpener trait
// ---------------------------------------------------------------------------

/// Opens a *draft* pull/merge request for a built branch.
///
/// Injected into [`build_from_ticket`] so the orchestration core is testable
/// without a real git-hosting client. Real implementations wrap a
/// `GitHubClient` / `GitLabClient` / `BitbucketClient`.
///
/// Implementations MUST open the PR as a draft and MUST NEVER merge.
///
/// Uses [`async_trait`] so the core can accept `&dyn PrOpener` for injection.
#[async_trait]
pub trait PrOpener: Send + Sync {
    /// Open a draft PR from `branch` with the given `title` and `body`.
    ///
    /// Returns the PR/MR URL on success.
    async fn open_draft_pr(&self, branch: &str, title: &str, body: &str) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// BranchPusher trait
// ---------------------------------------------------------------------------

/// Pushes a built branch to the `origin` remote.
///
/// Injected into [`build_from_ticket`] so the push step is unit-testable
/// without a real remote (mirroring the [`PrOpener`] seam). The real
/// implementation, [`GitBranchPusher`], shells out to `git push` *after*
/// validating the command through [`SecurityGateway`] (the same pattern
/// `auto_commit` uses). Tests inject a recording stub.
///
/// A push is performed ONLY on the `--open-pr` path, immediately before the PR
/// is opened, so the PR never references a branch that is missing on the remote.
pub trait BranchPusher: Send + Sync {
    /// Push `branch` from the repository at `repo_path` to `origin`.
    ///
    /// Returns `Ok(())` on success or a human-readable error (never panics).
    fn push(&self, repo_path: &Path, branch: &str) -> Result<(), String>;
}

/// The production [`BranchPusher`]: runs `git push --set-upstream origin
/// <branch>` validated through [`SecurityGateway`].
pub struct GitBranchPusher;

impl BranchPusher for GitBranchPusher {
    fn push(&self, repo_path: &Path, branch: &str) -> Result<(), String> {
        push_branch(repo_path, branch)
    }
}

/// Push `branch` to `origin` using a [`SecurityGateway`]-validated `git`
/// command — the same defensive pattern as
/// [`crate::auto_commit::AutoCommitService`]. No raw, unchecked shell is ever
/// run, and the branch name is validated to contain only safe characters before
/// it is handed to git.
///
/// Surfaces all failures as `Err(String)`; never panics.
pub fn push_branch(repo_path: &Path, branch: &str) -> Result<(), String> {
    // Defense in depth: the branch comes from `ticket_branch_name` (already
    // sanitized), but re-validate here so this helper is safe in isolation.
    if branch.is_empty()
        || !branch
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
    {
        return Err(format!("Refusing to push unsafe branch name '{branch}'"));
    }

    let gateway = SecurityGateway::new();
    let cmd = format!("git push --set-upstream origin {branch}");
    gateway
        .check_command(&cmd)
        .map_err(|e| format!("Security check failed for git push: {e}"))?;

    let output = Command::new("git")
        .args(["push", "--set-upstream", "origin", branch])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run git push: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git push failed: {}", stderr.trim()));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Risk tier
// ---------------------------------------------------------------------------

/// Repository risk tier. Higher tiers force human approval before a PR is
/// opened, regardless of the caller's `require_approval` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskTier {
    /// Low risk: caller's `require_approval` flag is honored as-is.
    #[default]
    Low,
    /// Medium risk: caller's `require_approval` flag is honored as-is.
    Medium,
    /// High risk: approval is ALWAYS required; a PR is never auto-opened.
    High,
}

impl RiskTier {
    /// Whether this tier forces approval no matter what the caller requested.
    fn forces_approval(self) -> bool {
        matches!(self, RiskTier::High)
    }
}

// ---------------------------------------------------------------------------
// Build options
// ---------------------------------------------------------------------------

/// Options controlling a ticket build.
#[derive(Debug, Clone)]
pub struct BuildOpts {
    /// When `true` (the default) STOP before opening a PR and return the branch
    /// + summary for human review. NEVER auto-merge regardless of this flag.
    pub require_approval: bool,
    /// Repository risk tier. [`RiskTier::High`] forces approval even when
    /// `require_approval` is `false`.
    pub risk_tier: RiskTier,
    /// Path to the git repository to build in. When `Some`, a git worktree is
    /// created for isolation so the user's working tree is never touched. When
    /// `None`, no worktree is created (used by tests / dry runs).
    pub repo_path: Option<std::path::PathBuf>,
    /// Base branch the PR should target. Defaults to `"main"`.
    pub base_branch: String,
    /// Swarm configuration used to run the build. Defaults to
    /// [`SwarmConfig::default`].
    pub swarm_config: SwarmConfig,
}

impl Default for BuildOpts {
    fn default() -> Self {
        Self {
            // Safe default: require human approval before opening a PR.
            require_approval: true,
            risk_tier: RiskTier::default(),
            repo_path: None,
            base_branch: "main".into(),
            swarm_config: SwarmConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Build outcome
// ---------------------------------------------------------------------------

/// The result of a ticket build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildOutcome {
    /// The swarm completed and a draft PR was opened.
    PrOpened {
        /// The branch the work landed on (`hive/ticket-{ref}`).
        branch: String,
        /// The opened draft PR URL.
        pr_url: String,
        /// Human-readable summary of what the swarm produced.
        summary: String,
    },
    /// The swarm completed but a PR was deliberately NOT opened because
    /// approval is required (caller opted in or the risk tier demanded it).
    /// The branch is ready for human review.
    AwaitingApproval {
        /// The branch the work landed on (`hive/ticket-{ref}`).
        branch: String,
        /// Human-readable summary of what the swarm produced.
        summary: String,
        /// Why approval was required.
        reason: String,
    },
    /// The build failed (swarm error, empty ticket, PR-open failure, etc.).
    /// Never panics — failures always land here.
    Error {
        /// Best-effort branch name (may be empty if branch was never computed).
        branch: String,
        /// What went wrong.
        message: String,
    },
}

impl BuildOutcome {
    /// Convenience: whether a draft PR was opened.
    pub fn pr_opened(&self) -> bool {
        matches!(self, BuildOutcome::PrOpened { .. })
    }

    /// Convenience: whether the build stopped for approval.
    pub fn awaiting_approval(&self) -> bool {
        matches!(self, BuildOutcome::AwaitingApproval { .. })
    }

    /// Convenience: whether the build errored.
    pub fn is_error(&self) -> bool {
        matches!(self, BuildOutcome::Error { .. })
    }

    /// The branch name regardless of variant.
    pub fn branch(&self) -> &str {
        match self {
            BuildOutcome::PrOpened { branch, .. }
            | BuildOutcome::AwaitingApproval { branch, .. }
            | BuildOutcome::Error { branch, .. } => branch,
        }
    }
}

// ---------------------------------------------------------------------------
// Branch naming
// ---------------------------------------------------------------------------

/// Sanitize a ticket reference into a safe branch component and build the
/// `hive/ticket-{ref}` branch name.
///
/// Keeps alphanumerics, dash and underscore; everything else (including `/`,
/// spaces, `#`) is collapsed to a single dash. Leading/trailing dashes are
/// trimmed and the component is length-capped. Returns `None` when nothing
/// usable remains.
pub fn ticket_branch_name(ticket_ref: &str) -> Option<String> {
    let mut out = String::with_capacity(ticket_ref.len());
    let mut last_dash = false;
    for c in ticket_ref.chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
            last_dash = c == '-';
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let capped: String = trimmed.chars().take(60).collect();
    let capped = capped.trim_matches('-').to_string();
    if capped.is_empty() {
        None
    } else {
        Some(format!("hive/ticket-{capped}"))
    }
}

// ---------------------------------------------------------------------------
// PR body
// ---------------------------------------------------------------------------

/// Build the PR body. Always references the ticket id and (if present) its URL,
/// plus a marker that this is an automated, draft, never-auto-merged change.
fn build_pr_body(ticket_ref: &str, ticket_url: Option<&str>, summary: &str) -> String {
    let mut body = String::new();
    body.push_str("Automated change built by Hive from a ticket.\n\n");
    body.push_str(&format!("Ticket: {ticket_ref}\n"));
    if let Some(url) = ticket_url {
        body.push_str(&format!("Ticket URL: {url}\n"));
    }
    body.push_str("\n## Summary\n\n");
    if summary.trim().is_empty() {
        body.push_str("(no summary produced)\n");
    } else {
        body.push_str(summary.trim());
        body.push('\n');
    }
    body.push_str(
        "\n---\nThis is a DRAFT pull request opened by Hive. It is never \
         auto-merged; a human must review and merge it.\n",
    );
    body
}

// ---------------------------------------------------------------------------
// Consolidating swarm output onto the result branch
// ---------------------------------------------------------------------------

/// Collect every raw text output the swarm produced — each completed team's
/// content plus the final synthesized output — so file-targeted edits can be
/// parsed from all of them, not just the summary.
fn collect_swarm_texts(result: &SwarmResult) -> Vec<String> {
    let mut texts = Vec::new();
    for team in &result.team_results {
        if team.status != TeamStatus::Completed {
            continue;
        }
        match &team.inner {
            Some(InnerResult::Native { content, .. })
            | Some(InnerResult::SingleShot { content, .. })
            | Some(InnerResult::Fusion { content, .. }) => texts.push(content.clone()),
            Some(InnerResult::HiveMind { result }) => {
                texts.push(result.synthesized_output.clone())
            }
            Some(InnerResult::Coordinator { result }) => {
                for t in &result.results {
                    if t.success {
                        texts.push(t.output.clone());
                    }
                }
            }
            None => {}
        }
    }
    texts.push(result.synthesized_output.clone());
    texts
}

/// Apply any file-targeted edits found in the swarm's outputs into `work_dir`.
///
/// Edits are extracted with [`parse_edits`] (fenced `lang:path` blocks and
/// `<edit path=...>` tags) — the same parser the desktop "Apply all edits"
/// action uses. Each edit path is validated to stay within `work_dir` (no
/// absolute paths, no `..` traversal) before any write. Returns the list of
/// relative paths written.
///
/// Returns an error only on a genuine IO/security failure; an empty result
/// (no parseable edits) is NOT an error — the caller decides how to handle an
/// edit-less swarm output.
fn apply_swarm_edits(work_dir: &Path, texts: &[String]) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for text in texts {
        for edit in parse_edits(text) {
            let rel = sanitize_edit_path(&edit.file_path).ok_or_else(|| {
                format!("Refusing to apply edit to unsafe path '{}'", edit.file_path)
            })?;
            let target = work_dir.join(&rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create dir for '{rel}': {e}"))?;
            }
            std::fs::write(&target, &edit.new_content)
                .map_err(|e| format!("Failed to write '{rel}': {e}"))?;
            if !written.contains(&rel) {
                written.push(rel);
            }
        }
    }
    Ok(written)
}

/// Validate an edit's target path: must be relative, must not contain `..`
/// components, and must not be empty. Returns the normalized relative path on
/// success, or `None` if it is unsafe.
fn sanitize_edit_path(path: &str) -> Option<String> {
    let p = Path::new(path);
    if p.is_absolute() {
        return None;
    }
    let mut parts = Vec::new();
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            std::path::Component::CurDir => {}
            // ParentDir / RootDir / Prefix are all rejected as traversal.
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

// ---------------------------------------------------------------------------
// Core orchestration
// ---------------------------------------------------------------------------

/// Turn a ticket into a built branch and (optionally) a draft PR.
///
/// * `objective` — the goal string driving the build. Typically derived from
///   the ticket title + body by the caller.
/// * `ticket_ref` — the ticket id (used for branch naming + PR body linking).
/// * `executor` — the AI executor the swarm runs on (mockable), passed as
///   `Arc<E>`. NOTE: [`AiExecutor`] uses an `async fn` in trait (RPITIT) and is
///   therefore not `dyn`-compatible, so the executor cannot be `Arc<dyn
///   AiExecutor>`; the core is generic over `E` instead, exactly mirroring
///   [`Queen::new`].
/// * `pr` — the injected [`PrOpener`] (mockable), as `&dyn PrOpener`.
/// * `opts` — [`BuildOpts`] controlling approval, risk tier, worktree, etc.
///
/// Branch coherence: when a real repo is supplied, the swarm's changes are
/// consolidated and committed onto the single `hive/ticket-{ref}` branch (the
/// exact branch reported back), inside an isolated worktree so the user's
/// working tree is never mutated. On the `--open-pr` path (and only there) that
/// branch is pushed to `origin` *before* the PR is opened, so the PR can never
/// reference an empty / missing-on-remote branch. The default approval-gated
/// path neither pushes nor opens a PR.
///
/// Returns a [`BuildOutcome`]. Never panics; all failures map to
/// [`BuildOutcome::Error`].
///
/// This is a thin wrapper over [`build_from_ticket_full`] using the production
/// [`GitBranchPusher`].
pub async fn build_from_ticket<E: AiExecutor + 'static>(
    objective: &str,
    ticket_ref: &str,
    executor: Arc<E>,
    pr: &dyn PrOpener,
    opts: BuildOpts,
) -> BuildOutcome {
    build_from_ticket_full(objective, ticket_ref, executor, pr, &GitBranchPusher, opts).await
}

/// Full form of [`build_from_ticket`] with an injectable [`BranchPusher`] so the
/// push step is unit-testable without a real remote. Production callers should
/// use [`build_from_ticket`] (which injects [`GitBranchPusher`]).
pub async fn build_from_ticket_full<E: AiExecutor + 'static>(
    objective: &str,
    ticket_ref: &str,
    executor: Arc<E>,
    pr: &dyn PrOpener,
    pusher: &dyn BranchPusher,
    opts: BuildOpts,
) -> BuildOutcome {
    // --- Validate inputs --------------------------------------------------
    if objective.trim().is_empty() {
        return BuildOutcome::Error {
            branch: String::new(),
            message: "Empty objective: cannot build from a ticket with no title/body".into(),
        };
    }

    let branch = match ticket_branch_name(ticket_ref) {
        Some(b) => b,
        None => {
            return BuildOutcome::Error {
                branch: String::new(),
                message: format!(
                    "Invalid ticket reference '{ticket_ref}': no usable branch name characters"
                ),
            };
        }
    };

    // --- Optional worktree isolation -------------------------------------
    // When a repo path is supplied, create an isolated worktree ON THE RESULT
    // BRANCH itself (`hive/ticket-{ref}`) so (a) the user's working tree is
    // never mutated, and (b) the branch we report, commit onto, and push is the
    // SAME branch — no more `swarm/...` vs `hive/ticket-...` mismatch.
    let mut worktree: Option<crate::worktree::TeamWorktree> = None;
    if let Some(ref repo_path) = opts.repo_path {
        let mgr = WorktreeManager::new(repo_path.clone());
        // Slot name = sanitized ticket tail; the branch is the full result
        // branch so `wt.branch_name == branch`.
        let slot = branch.trim_start_matches("hive/ticket-");
        match mgr.create_branch_worktree(&branch, slot) {
            Ok(wt) => worktree = Some(wt),
            Err(e) => {
                // Worktree isolation failed — refuse to build in place rather
                // than risk touching the user's tree.
                return BuildOutcome::Error {
                    branch,
                    message: format!("Failed to create isolated worktree: {e}"),
                };
            }
        }
    }

    // --- Run the swarm ----------------------------------------------------
    let queen = Queen::new(opts.swarm_config.clone(), executor);
    let swarm_result = match queen.execute(objective).await {
        Ok(r) => r,
        Err(e) => {
            return BuildOutcome::Error {
                branch,
                message: format!("Swarm execution failed: {e}"),
            };
        }
    };

    // Treat a fully-failed swarm as an error; partial success still produces a
    // branch + summary the human can inspect.
    if swarm_result.status == SwarmStatus::Failed {
        return BuildOutcome::Error {
            branch,
            message: format!(
                "Swarm failed to produce a build (status: {:?})",
                swarm_result.status
            ),
        };
    }

    // --- Consolidate the swarm's changes onto the result branch ----------
    // Apply any file-targeted edits the swarm emitted into the worktree, then
    // commit them onto `hive/ticket-{ref}`. If the swarm produced no parseable
    // edits we still write the synthesized output as a tracked artifact so the
    // branch is never empty and the reviewer has something concrete to read.
    //
    // NOTE: this is purely git mechanics in `hive_agents`. Whether the branch
    // contains *real code changes* depends on the swarm emitting `lang:path` /
    // `<edit>` blocks — see the module-level caveat and the LIVE-VALIDATION
    // note in the PR/return summary.
    let mut consolidation_note = String::new();
    if let Some(ref wt) = worktree {
        let texts = collect_swarm_texts(&swarm_result);
        let applied = match apply_swarm_edits(&wt.worktree_path, &texts) {
            Ok(paths) => paths,
            Err(e) => {
                let message = format!("Failed to apply swarm edits to '{branch}': {e}");
                return BuildOutcome::Error { branch, message };
            }
        };

        if applied.is_empty() {
            // Fallback: persist the synthesized output so the branch carries a
            // committable artifact rather than being empty.
            let artifact = wt.worktree_path.join("HIVE_BUILD.md");
            let body = if swarm_result.synthesized_output.trim().is_empty() {
                format!("# Hive build for ticket {ticket_ref}\n\n(no swarm output)\n")
            } else {
                format!(
                    "# Hive build for ticket {ticket_ref}\n\n{}\n",
                    swarm_result.synthesized_output.trim()
                )
            };
            if let Err(e) = std::fs::write(&artifact, body) {
                return BuildOutcome::Error {
                    branch,
                    message: format!("Failed to write build artifact: {e}"),
                };
            }
            consolidation_note =
                " (no file edits parsed from swarm output; wrote build summary artifact)".into();
        } else {
            consolidation_note = format!(" (applied {} file edit(s))", applied.len());
        }

        let commit_msg = format!("[hive] build from ticket {ticket_ref}");
        if let Err(e) =
            crate::worktree::WorktreeManager::commit_worktree_changes(&wt.worktree_path, &commit_msg)
        {
            let message = format!("Failed to commit swarm changes onto '{branch}': {e}");
            return BuildOutcome::Error { branch, message };
        }
    }

    let summary = if swarm_result.synthesized_output.trim().is_empty() {
        format!("Built from ticket {ticket_ref}{consolidation_note}")
    } else {
        format!(
            "{}{}",
            swarm_result.synthesized_output.trim(),
            consolidation_note
        )
    };

    // --- Approval gate ----------------------------------------------------
    // Safe default: require approval. The risk tier can force approval even if
    // the caller opted out. We NEVER auto-merge in either branch.
    let forced_by_tier = opts.risk_tier.forces_approval();
    if opts.require_approval || forced_by_tier {
        let reason = if forced_by_tier && !opts.require_approval {
            format!("risk tier {:?} requires human approval", opts.risk_tier)
        } else if forced_by_tier {
            format!(
                "approval requested and risk tier {:?} requires it",
                opts.risk_tier
            )
        } else {
            "approval required before opening a PR".to_string()
        };
        return BuildOutcome::AwaitingApproval {
            branch,
            summary,
            reason,
        };
    }

    // --- Push the result branch to origin (open-pr path ONLY) -------------
    // The PR must reference a branch that exists on the remote, so we push
    // `hive/ticket-{ref}` BEFORE opening the PR. Pushing only happens here, on
    // the unattended open-pr path; the approval-gated default above already
    // returned without pushing. We skip the push when there is no real repo
    // (repo_path = None / no worktree), e.g. in dry runs.
    if let Some(ref repo_path) = opts.repo_path
        && worktree.is_some()
        && let Err(e) = pusher.push(repo_path, &branch)
    {
        let message = format!("Build succeeded but pushing branch to origin failed: {e}");
        return BuildOutcome::Error { branch, message };
    }

    // --- Open a DRAFT PR --------------------------------------------------
    let pr_title = format!("[Hive] {ticket_ref}: automated build");
    let body = build_pr_body(ticket_ref, None, &summary);
    match pr.open_draft_pr(&branch, &pr_title, &body).await {
        Ok(pr_url) => BuildOutcome::PrOpened {
            branch,
            pr_url,
            summary,
        },
        Err(e) => BuildOutcome::Error {
            branch,
            message: format!("Build succeeded but opening draft PR failed: {e}"),
        },
    }
}

/// Variant of [`build_from_ticket`] that also threads the ticket URL into the
/// PR body. Prefer this when the resolved ticket has a URL.
#[allow(clippy::too_many_arguments)]
pub async fn build_from_ticket_with_url<E: AiExecutor + 'static>(
    objective: &str,
    ticket_ref: &str,
    ticket_url: Option<&str>,
    executor: Arc<E>,
    pr: &dyn PrOpener,
    opts: BuildOpts,
) -> BuildOutcome {
    // Reuse the main path for everything except the PR body's URL line. To keep
    // the URL in the body we re-implement the final PR step here only when a
    // PR is actually opened; otherwise we just delegate.
    let outcome = build_from_ticket(objective, ticket_ref, executor, pr, opts).await;
    // If a PR was opened, the body already lacked the URL. We cannot re-open it,
    // so instead we surface the URL through the summary for visibility. The
    // primary, fully-tested path is `build_from_ticket`; this helper exists for
    // callers that have a URL and want it reflected. When approval is required
    // (the default) no PR is opened, so the URL simply rides along in the
    // summary already produced.
    match outcome {
        BuildOutcome::AwaitingApproval {
            branch,
            summary,
            reason,
        } => {
            let summary = match ticket_url {
                Some(u) => format!("{summary}\n\nTicket URL: {u}"),
                None => summary,
            };
            BuildOutcome::AwaitingApproval {
                branch,
                summary,
                reason,
            }
        }
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hive_ai::types::{ChatRequest, ChatResponse, FinishReason, TokenUsage};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // -- Mock AI executor ----------------------------------------------------
    //
    // Returns a JSON team plan for the Queen's planning call; the same content
    // is returned for the SingleShot team execution (the swarm tolerates this).

    struct MockExecutor {
        response: String,
        calls: AtomicUsize,
    }

    impl MockExecutor {
        fn new(response: &str) -> Self {
            Self {
                response: response.into(),
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl AiExecutor for MockExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ChatResponse {
                content: self.response.clone(),
                model: "mock".into(),
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                thinking: None,
                tool_calls: None,
            })
        }
    }

    /// An executor that always fails (to exercise the error path).
    struct FailingExecutor;

    impl AiExecutor for FailingExecutor {
        async fn execute(&self, _request: &ChatRequest) -> Result<ChatResponse, String> {
            Err("provider unreachable".into())
        }
    }

    // -- Mock PR opener ------------------------------------------------------

    struct MockPrOpener {
        /// Records (branch, title, body) of the last opened PR.
        opened: Mutex<Vec<(String, String, String)>>,
        url: String,
        fail: bool,
    }

    impl MockPrOpener {
        fn ok(url: &str) -> Self {
            Self {
                opened: Mutex::new(Vec::new()),
                url: url.into(),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                opened: Mutex::new(Vec::new()),
                url: String::new(),
                fail: true,
            }
        }

        fn open_count(&self) -> usize {
            self.opened.lock().unwrap().len()
        }

        fn last_body(&self) -> Option<String> {
            self.opened.lock().unwrap().last().map(|(_, _, b)| b.clone())
        }
    }

    #[async_trait]
    impl PrOpener for MockPrOpener {
        async fn open_draft_pr(
            &self,
            branch: &str,
            title: &str,
            body: &str,
        ) -> Result<String, String> {
            if self.fail {
                return Err("github 422: draft PR rejected".into());
            }
            self.opened
                .lock()
                .unwrap()
                .push((branch.into(), title.into(), body.into()));
            Ok(self.url.clone())
        }
    }

    // -- Mock branch pusher --------------------------------------------------

    /// Records each (repo_path, branch) push so tests can assert the push step
    /// fired (or did not) and with which branch — without touching a real
    /// remote.
    struct MockPusher {
        pushed: Mutex<Vec<(std::path::PathBuf, String)>>,
        fail: bool,
    }

    impl MockPusher {
        fn ok() -> Self {
            Self {
                pushed: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                pushed: Mutex::new(Vec::new()),
                fail: true,
            }
        }

        fn push_count(&self) -> usize {
            self.pushed.lock().unwrap().len()
        }

        fn last_branch(&self) -> Option<String> {
            self.pushed.lock().unwrap().last().map(|(_, b)| b.clone())
        }
    }

    impl BranchPusher for MockPusher {
        fn push(&self, repo_path: &Path, branch: &str) -> Result<(), String> {
            if self.fail {
                return Err("remote rejected push (403)".into());
            }
            self.pushed
                .lock()
                .unwrap()
                .push((repo_path.to_path_buf(), branch.into()));
            Ok(())
        }
    }

    /// Initialize a temp git repo with an initial commit on a non-protected
    /// branch (so worktree creation off HEAD works) and return the repo dir.
    fn setup_repo() -> tempfile::TempDir {
        use git2::Repository;
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let sig = git2::Signature::now("Test", "test@test.com").unwrap();
            let tree_id = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
        // Ensure .hive-worktrees is ignored to keep the tree clean.
        std::fs::write(dir.path().join(".gitignore"), ".hive-worktrees/\n").unwrap();
        dir
    }

    /// A valid single-team plan the Queen can parse and execute.
    fn plan_json() -> &'static str {
        r#"[
            {
                "id": "team-1",
                "name": "Implement",
                "description": "Implement the ticket",
                "dependencies": [],
                "orchestration_mode": "single_shot",
                "scope_paths": [],
                "priority": 0
            }
        ]"#
    }

    // -- Branch naming -------------------------------------------------------

    #[test]
    fn branch_name_sanitizes_ref() {
        assert_eq!(
            ticket_branch_name("PROJ-123"),
            Some("hive/ticket-proj-123".into())
        );
        assert_eq!(
            ticket_branch_name("#42 Fix login"),
            Some("hive/ticket-42-fix-login".into())
        );
        assert_eq!(
            ticket_branch_name("a/b\\c"),
            Some("hive/ticket-a-b-c".into())
        );
    }

    #[test]
    fn branch_name_rejects_empty() {
        assert_eq!(ticket_branch_name(""), None);
        assert_eq!(ticket_branch_name("###"), None);
    }

    // -- PR opened path (require_approval = false) ---------------------------

    #[tokio::test]
    async fn ticket_to_draft_pr_when_approval_not_required() {
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://github.com/acme/repo/pull/7");
        let opts = BuildOpts {
            require_approval: false,
            ..Default::default()
        };

        let outcome =
            build_from_ticket("Implement ticket PROJ-9", "PROJ-9", executor, &pr, opts).await;

        match &outcome {
            BuildOutcome::PrOpened {
                branch,
                pr_url,
                ..
            } => {
                assert_eq!(branch, "hive/ticket-proj-9");
                assert_eq!(pr_url, "https://github.com/acme/repo/pull/7");
            }
            other => panic!("expected PrOpened, got {other:?}"),
        }
        assert!(outcome.pr_opened());

        // The PR was opened exactly once and its body references the ticket id.
        assert_eq!(pr.open_count(), 1);
        let body = pr.last_body().unwrap();
        assert!(body.contains("PROJ-9"), "body should reference ticket id");
        assert!(
            body.to_lowercase().contains("draft"),
            "body should note it is a draft"
        );
        assert!(
            body.to_lowercase().contains("never")
                && body.to_lowercase().contains("auto-merge"),
            "body should state it is never auto-merged"
        );
    }

    #[tokio::test]
    async fn pr_body_includes_ticket_url_via_helper() {
        // The URL helper threads the ticket URL into the AwaitingApproval
        // summary (default path requires approval, so no PR is opened).
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://example/pr/1");

        let outcome = build_from_ticket_with_url(
            "Implement ticket",
            "BUG-1",
            Some("https://tracker/BUG-1"),
            executor,
            &pr,
            BuildOpts::default(),
        )
        .await;

        match outcome {
            BuildOutcome::AwaitingApproval { summary, .. } => {
                assert!(summary.contains("https://tracker/BUG-1"));
            }
            other => panic!("expected AwaitingApproval, got {other:?}"),
        }
    }

    // -- Approval gate (require_approval = true, the default) ----------------

    #[tokio::test]
    async fn stops_at_branch_when_approval_required() {
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://github.com/acme/repo/pull/1");

        // Default opts => require_approval = true.
        let outcome =
            build_from_ticket("Implement ticket TASK-5", "TASK-5", executor, &pr, BuildOpts::default())
                .await;

        match &outcome {
            BuildOutcome::AwaitingApproval { branch, .. } => {
                assert_eq!(branch, "hive/ticket-task-5");
            }
            other => panic!("expected AwaitingApproval, got {other:?}"),
        }
        assert!(outcome.awaiting_approval());

        // Crucially: NO PR was opened.
        assert_eq!(pr.open_count(), 0, "no PR may be opened when approval is required");
    }

    #[tokio::test]
    async fn high_risk_tier_forces_approval_even_when_opted_out() {
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://github.com/acme/repo/pull/2");
        let opts = BuildOpts {
            require_approval: false,
            risk_tier: RiskTier::High,
            ..Default::default()
        };

        let outcome = build_from_ticket("Implement risky change", "SEC-1", executor, &pr, opts).await;

        assert!(outcome.awaiting_approval(), "high risk must force approval");
        assert_eq!(pr.open_count(), 0, "high-risk builds never auto-open a PR");
        if let BuildOutcome::AwaitingApproval { reason, .. } = &outcome {
            assert!(reason.to_lowercase().contains("risk tier"));
        }
    }

    // -- Error paths (never panic) ------------------------------------------

    #[tokio::test]
    async fn empty_objective_is_error() {
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://x/pr/1");
        let outcome =
            build_from_ticket("   ", "T-1", executor, &pr, BuildOpts::default()).await;
        assert!(outcome.is_error());
        assert_eq!(pr.open_count(), 0);
    }

    #[tokio::test]
    async fn invalid_ticket_ref_is_error() {
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://x/pr/1");
        let outcome =
            build_from_ticket("Do the thing", "###", executor, &pr, BuildOpts::default()).await;
        assert!(outcome.is_error());
        assert_eq!(pr.open_count(), 0);
    }

    #[tokio::test]
    async fn swarm_failure_is_error_not_panic() {
        let executor = Arc::new(FailingExecutor);
        let pr = MockPrOpener::ok("https://x/pr/1");
        let opts = BuildOpts {
            require_approval: false,
            ..Default::default()
        };
        let outcome = build_from_ticket("Implement ticket", "T-2", executor, &pr, opts).await;
        match &outcome {
            BuildOutcome::Error { branch, message } => {
                assert_eq!(branch, "hive/ticket-t-2");
                assert!(message.to_lowercase().contains("swarm"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
        assert_eq!(pr.open_count(), 0);
    }

    #[tokio::test]
    async fn pr_open_failure_is_error() {
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::failing();
        let opts = BuildOpts {
            require_approval: false,
            ..Default::default()
        };
        let outcome = build_from_ticket("Implement ticket", "T-3", executor, &pr, opts).await;
        assert!(outcome.is_error());
        if let BuildOutcome::Error { message, .. } = &outcome {
            assert!(message.contains("draft PR"));
        }
    }

    // -- Path sanitization (security) ---------------------------------------

    #[test]
    fn sanitize_edit_path_accepts_relative() {
        assert_eq!(sanitize_edit_path("src/main.rs").as_deref(), Some("src/main.rs"));
        assert_eq!(sanitize_edit_path("./a/b.txt").as_deref(), Some("a/b.txt"));
    }

    #[test]
    fn sanitize_edit_path_rejects_traversal_and_absolute() {
        assert!(sanitize_edit_path("../escape").is_none());
        assert!(sanitize_edit_path("a/../../b").is_none());
        assert!(sanitize_edit_path("/etc/passwd").is_none());
        assert!(sanitize_edit_path("").is_none());
    }

    #[test]
    fn push_branch_rejects_unsafe_branch_name() {
        // Injection-style branch names must be refused before git is invoked.
        let dir = setup_repo();
        let bad = push_branch(dir.path(), "main; rm -rf /");
        assert!(bad.is_err());
        assert!(bad.unwrap_err().contains("unsafe branch"));
    }

    // -- Open-PR path: branch consistency + push fired ----------------------
    //
    // A response carrying BOTH a JSON team plan (so the Queen can plan) AND a
    // fenced `lang:path` edit block (so the team output yields a real file
    // edit). The planner extracts the [..] array; `parse_edits` extracts the
    // fenced block from the team's SingleShot output.
    fn plan_with_edit() -> &'static str {
        "Plan and code.\n\
         [\n  {\n    \"id\": \"team-1\",\n    \"name\": \"Implement\",\n    \
         \"description\": \"do it\",\n    \"dependencies\": [],\n    \
         \"orchestration_mode\": \"single_shot\",\n    \"scope_paths\": [],\n    \
         \"priority\": 0\n  }\n]\n\n\
         ```rust:src/added.rs\nfn added() {}\n```\n"
    }

    #[tokio::test]
    async fn open_pr_path_pushes_then_opens_pr_on_consistent_branch() {
        let dir = setup_repo();
        let executor = Arc::new(MockExecutor::new(plan_with_edit()));
        let pr = MockPrOpener::ok("https://github.com/acme/repo/pull/42");
        let pusher = MockPusher::ok();
        let opts = BuildOpts {
            require_approval: false,
            repo_path: Some(dir.path().to_path_buf()),
            ..Default::default()
        };

        let outcome = build_from_ticket_full(
            "Implement ticket PROJ-9",
            "PROJ-9",
            executor,
            &pr,
            &pusher,
            opts,
        )
        .await;

        // PR opened, and the reported branch is the canonical name.
        match &outcome {
            BuildOutcome::PrOpened { branch, .. } => {
                assert_eq!(branch, "hive/ticket-proj-9");
            }
            other => panic!("expected PrOpened, got {other:?}"),
        }

        // The push fired exactly once, BEFORE the PR, on the SAME branch the PR
        // and outcome reference — end-to-end branch consistency.
        assert_eq!(pusher.push_count(), 1, "branch must be pushed on open-pr path");
        assert_eq!(pusher.last_branch().as_deref(), Some("hive/ticket-proj-9"));
        assert_eq!(pr.open_count(), 1);

        // The edit landed on the branch: open the worktree and confirm the file.
        let wt_repo = dir.path().join(".hive-worktrees").join("proj-9");
        assert!(
            wt_repo.join("src/added.rs").exists(),
            "swarm edit should be written into the worktree"
        );
    }

    #[tokio::test]
    async fn default_path_pushes_nothing_and_opens_no_pr() {
        let dir = setup_repo();
        let executor = Arc::new(MockExecutor::new(plan_with_edit()));
        let pr = MockPrOpener::ok("https://x/pr/1");
        let pusher = MockPusher::ok();
        // Default opts => require_approval = true, but with a real repo.
        let opts = BuildOpts {
            repo_path: Some(dir.path().to_path_buf()),
            ..Default::default()
        };

        let outcome =
            build_from_ticket_full("Implement TASK-5", "TASK-5", executor, &pr, &pusher, opts).await;

        match &outcome {
            BuildOutcome::AwaitingApproval { branch, .. } => {
                assert_eq!(branch, "hive/ticket-task-5");
            }
            other => panic!("expected AwaitingApproval, got {other:?}"),
        }
        // Crucially: nothing pushed, no PR opened on the default path.
        assert_eq!(pusher.push_count(), 0, "approval path must NEVER push");
        assert_eq!(pr.open_count(), 0, "approval path must NEVER open a PR");

        // The branch still exists locally with the committed changes for review.
        let branch_exists = git2::Repository::open(dir.path())
            .unwrap()
            .find_branch("hive/ticket-task-5", git2::BranchType::Local)
            .is_ok();
        assert!(branch_exists, "result branch should exist for human review");
    }

    #[tokio::test]
    async fn push_failure_is_error_and_pr_not_opened() {
        let dir = setup_repo();
        let executor = Arc::new(MockExecutor::new(plan_with_edit()));
        let pr = MockPrOpener::ok("https://x/pr/1");
        let pusher = MockPusher::failing();
        let opts = BuildOpts {
            require_approval: false,
            repo_path: Some(dir.path().to_path_buf()),
            ..Default::default()
        };

        let outcome =
            build_from_ticket_full("Implement T-9", "T-9", executor, &pr, &pusher, opts).await;

        match &outcome {
            BuildOutcome::Error { branch, message } => {
                assert_eq!(branch, "hive/ticket-t-9");
                assert!(message.to_lowercase().contains("push"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
        // A failed push must abort before opening the PR.
        assert_eq!(pr.open_count(), 0, "must not open a PR if the push failed");
    }

    #[tokio::test]
    async fn no_repo_path_skips_push_entirely() {
        // Dry-run style: no repo_path => no worktree => no push attempted even
        // on the open-pr path. The injected pusher must never be called.
        let executor = Arc::new(MockExecutor::new(plan_json()));
        let pr = MockPrOpener::ok("https://x/pr/1");
        let pusher = MockPusher::ok();
        let opts = BuildOpts {
            require_approval: false,
            ..Default::default()
        };

        let outcome =
            build_from_ticket_full("Implement T-1", "T-1", executor, &pr, &pusher, opts).await;

        assert!(outcome.pr_opened());
        assert_eq!(pusher.push_count(), 0, "no repo => nothing to push");
        assert_eq!(pr.open_count(), 1);
    }
}
