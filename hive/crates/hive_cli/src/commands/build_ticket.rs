//! `hive build-ticket <source> <id>` — turn a ticket into a built branch and,
//! optionally, a draft pull request.
//!
//! This command wires the headless [`hive_agents::build_from_ticket`]
//! orchestration core end-to-end from the terminal:
//!
//! 1. Load config (`HiveConfig::load`).
//! 2. Build a [`ProjectManagementHub`] and register Jira / Linear / GitHub
//!    providers from config (mirrors `hive_app::main`).
//! 3. Resolve the provider for `<source>` and fetch the ticket. The objective
//!    is `"{title}\n\n{body}"`.
//! 4. Construct a routing AI executor over [`hive_ai::AiService::routing_handle`]
//!    so each swarm request — including the `"auto"` sentinel the swarm emits
//!    (auto_routing is on by default) — is routed to the correct provider by the
//!    policy/cost router and scrubbed by the egress redactor.
//! 5. When `--open-pr` is set, construct a GitHub draft-PR opener and run with
//!    `require_approval = false`. Otherwise (the default) run approval-gated:
//!    no PR is opened and the branch is left for human review.
//!
//! The whole flow is defensive — every failure surfaces as a clear error and a
//! non-zero exit code; nothing panics.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use hive_agents::{build_from_ticket, BuildOpts, BuildOutcome, PrOpener};
use hive_ai::{AiService, AiServiceConfig};
use hive_core::HiveConfig;
use hive_integrations::github::GitHubClient;
use hive_integrations::project_management::{PMPlatform, ProjectManagementHub};

/// Run the `build-ticket` subcommand.
///
/// * `source` — one of `jira`, `linear`, `github`.
/// * `id` — the ticket reference (e.g. `PROJ-123`, `42`).
/// * `open_pr` — when `true`, run unattended and open a draft PR. When `false`
///   (the default) run approval-gated: build the branch but open no PR.
/// * `repo` — local git repo to build in. Defaults to the current directory.
/// * `base` — base branch the PR should target. Defaults to `main`.
pub async fn run(
    source: &str,
    id: &str,
    open_pr: bool,
    repo: Option<PathBuf>,
    base: String,
) -> Result<()> {
    let platform = parse_source(source)?;

    let config = HiveConfig::load().context("failed to load Hive config")?;

    // --- Project management hub: register configured providers --------------
    let hub = build_pm_hub(&config);
    if !hub.has_provider(platform) {
        return Err(anyhow!(
            "no '{source}' provider is configured. Configure its credentials \
             (see `hive config`) before building from a {source} ticket."
        ));
    }

    // --- Fetch the ticket ---------------------------------------------------
    let issue = hub.get_issue(platform, id).await.with_context(|| {
        format!("failed to fetch {source} ticket '{id}' (not found or not accessible)")
    })?;

    let title = issue.title.trim();
    let body = issue.description.as_deref().unwrap_or("").trim();
    let objective = if body.is_empty() {
        title.to_string()
    } else {
        format!("{title}\n\n{body}")
    };
    if objective.trim().is_empty() {
        return Err(anyhow!(
            "ticket '{id}' has no title or description to build from"
        ));
    }

    println!("Ticket {id} ({source}): {title}");
    if let Some(url) = issue.url.as_deref() {
        println!("  {url}");
    }

    // --- Routing AI executor -----------------------------------------------
    // The swarm emits the sentinel model "auto" (auto_routing is on by default)
    // and expects each request routed to the cheapest capable model per the
    // policy. So the executor must ROUTE every request via AiService's routing
    // handle rather than pin one provider — this resolves "auto" + concrete
    // models to the correct provider AND applies egress secret-redaction. This
    // mirrors the desktop swarm executor exactly.
    let ai = AiService::new(ai_service_config(&config));
    if ai.first_provider().is_none() {
        return Err(anyhow!(
            "no AI provider is configured. Set an API key (e.g. anthropic/openai) \
             or a local provider URL (ollama/lmstudio) before building."
        ));
    }
    let executor = Arc::new(RoutingExecutor {
        handle: ai.routing_handle(),
    });

    // --- Resolve the repo to build in --------------------------------------
    let repo_path = match repo {
        Some(p) => p,
        None => std::env::current_dir().context("failed to resolve current directory")?,
    };

    // --- PR opener (only needed when --open-pr) ----------------------------
    // Built eagerly so a misconfiguration fails fast before the swarm runs.
    let pr_opener: Box<dyn PrOpener> = if open_pr {
        let token = config
            .github_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "--open-pr requires a GitHub token. Configure `github_token` \
                     (see `hive config`) first."
                )
            })?;
        let (owner, repo_name) = resolve_owner_repo(&repo_path, &config).context(
            "could not determine the GitHub owner/repo to open the PR against. Set \
             `github_default_repo` to `owner/repo`, or run inside a repo whose `origin` \
             remote points at GitHub.",
        )?;
        let client = GitHubClient::new(token).context("failed to build GitHub client")?;
        Box::new(GitHubDraftPrOpener {
            client,
            owner,
            repo: repo_name,
            base: base.clone(),
        })
    } else {
        // Not used when require_approval = true, but the core needs a value.
        Box::new(NoopPrOpener)
    };

    // --- Build --------------------------------------------------------------
    let opts = BuildOpts {
        require_approval: !open_pr,
        repo_path: Some(repo_path),
        base_branch: base,
        ..Default::default()
    };

    println!(
        "Building from ticket {id} (open-pr: {}, base: {})...",
        open_pr, opts.base_branch
    );

    let outcome = build_from_ticket(&objective, id, executor, pr_opener.as_ref(), opts).await;
    report_outcome(&outcome);

    // Surface a non-zero exit on failure so scripts can detect it.
    if outcome.is_error() {
        std::process::exit(1);
    }
    Ok(())
}

/// Map the CLI `<source>` to a [`PMPlatform`].
fn parse_source(source: &str) -> Result<PMPlatform> {
    match source.to_ascii_lowercase().as_str() {
        "jira" => Ok(PMPlatform::Jira),
        "linear" => Ok(PMPlatform::Linear),
        "github" => Ok(PMPlatform::GitHubProjects),
        other => Err(anyhow!(
            "unknown source '{other}'. Expected one of: jira, linear, github"
        )),
    }
}

/// Build a [`ProjectManagementHub`] and register Jira / Linear / GitHub
/// providers from config. Mirrors the registration block in `hive_app::main`.
/// Missing or incomplete credentials are skipped silently (the caller checks
/// `has_provider`).
fn build_pm_hub(config: &HiveConfig) -> ProjectManagementHub {
    use hive_integrations::project_management::{GitHubIssuesClient, JiraClient, LinearClient};

    let mut hub = ProjectManagementHub::new();

    // Jira — needs base URL + email + API token, all non-empty.
    if let (Some(base_url), Some(email), Some(token)) = (
        config.jira_base_url.as_deref(),
        config.jira_email.as_deref(),
        config.jira_api_token.as_deref(),
    ) {
        let base_url = base_url.trim();
        let email = email.trim();
        let token = token.trim();
        if !base_url.is_empty() && !email.is_empty() && !token.is_empty() {
            let domain = jira_domain_from_base_url(base_url);
            let rest_base = format!("{}/rest/api/3", base_url.trim_end_matches('/'));
            if let Ok(client) = JiraClient::with_base_url(&domain, email, token, &rest_base) {
                hub.register_provider(Box::new(client));
            }
        }
    }

    // Linear — needs a non-empty API key.
    if let Some(api_key) = config.linear_api_key.as_deref() {
        let api_key = api_key.trim();
        if !api_key.is_empty() {
            if let Ok(client) = LinearClient::new(api_key) {
                hub.register_provider(Box::new(client));
            }
        }
    }

    // GitHub Issues — needs a non-empty token; default repo is optional.
    if let Some(token) = config.github_token.as_deref() {
        let token = token.trim();
        if !token.is_empty() {
            let default_repo = config
                .github_default_repo
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            if let Ok(client) = GitHubIssuesClient::with_default_repo(token, default_repo) {
                hub.register_provider(Box::new(client));
            }
        }
    }

    hub
}

/// Derive the Atlassian subdomain from a configured Jira base URL.
/// Mirrors `hive_app::jira_domain_from_base_url`.
fn jira_domain_from_base_url(base_url: &str) -> String {
    let host = base_url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim();

    host.strip_suffix(".atlassian.net")
        .map(|s| s.to_string())
        .unwrap_or_else(|| host.to_string())
}

/// Build an [`AiServiceConfig`] from a [`HiveConfig`]. Mirrors the conversion in
/// `hive_app::main`.
fn ai_service_config(config: &HiveConfig) -> AiServiceConfig {
    AiServiceConfig {
        anthropic_api_key: config.anthropic_api_key.clone(),
        openai_api_key: config.openai_api_key.clone(),
        openrouter_api_key: config.openrouter_api_key.clone(),
        google_api_key: config.google_api_key.clone(),
        groq_api_key: config.groq_api_key.clone(),
        huggingface_api_key: config.huggingface_api_key.clone(),
        xai_api_key: config.xai_api_key.clone(),
        mistral_api_key: config.mistral_api_key.clone(),
        venice_api_key: config.venice_api_key.clone(),
        zai_api_key: config.zai_api_key.clone(),
        litellm_url: config.litellm_url.clone(),
        litellm_api_key: config.litellm_api_key.clone(),
        ollama_url: config.ollama_url.clone(),
        lmstudio_url: config.lmstudio_url.clone(),
        local_provider_url: config.local_provider_url.clone(),
        kilo_url: Some(config.kilo_url.clone()),
        kilo_password: config.kilo_password.clone(),
        privacy_mode: config.privacy_mode,
        default_model: config.default_model.clone(),
        auto_routing: config.auto_routing,
        routing_policy: config.routing_policy.clone(),
    }
}

/// Resolve `(owner, repo)` for the PR target. Tries the `origin` remote of the
/// local repo first, then falls back to `config.github_default_repo`.
fn resolve_owner_repo(repo_path: &PathBuf, config: &HiveConfig) -> Result<(String, String)> {
    if let Some(remote) = git_origin_url(repo_path) {
        if let Some(parsed) = parse_github_owner_repo(&remote) {
            return Ok(parsed);
        }
    }

    if let Some(default_repo) = config.github_default_repo.as_deref() {
        let default_repo = default_repo.trim();
        if let Some((owner, repo)) = default_repo.split_once('/') {
            if !owner.is_empty() && !repo.is_empty() {
                return Ok((owner.to_string(), repo.to_string()));
            }
        }
    }

    Err(anyhow!("no GitHub origin remote and no github_default_repo"))
}

/// Best-effort `git -C <repo> remote get-url origin`. Returns `None` on any
/// failure.
fn git_origin_url(repo_path: &PathBuf) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Parse a GitHub remote URL into `(owner, repo)`. Mirrors the helper in
/// `hive_ui::workspace`.
fn parse_github_owner_repo(url: &str) -> Option<(String, String)> {
    // HTTPS: https://github.com/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.trim_end_matches(".git").splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.trim_end_matches(".git").splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    None
}

/// Print a [`BuildOutcome`] in a clear, human-readable form.
fn report_outcome(outcome: &BuildOutcome) {
    println!();
    match outcome {
        BuildOutcome::PrOpened {
            branch,
            pr_url,
            summary,
        } => {
            println!("Draft PR opened.");
            println!("  Branch:  {branch}");
            println!("  PR URL:  {pr_url}");
            println!("  Summary: {summary}");
        }
        BuildOutcome::AwaitingApproval {
            branch,
            summary,
            reason,
        } => {
            println!("Build complete — awaiting human approval (no PR opened).");
            println!("  Branch:  {branch}");
            println!("  Reason:  {reason}");
            println!("  Summary: {summary}");
            println!();
            println!("Review the branch, then re-run with --open-pr to open a draft PR.");
        }
        BuildOutcome::Error { branch, message } => {
            eprintln!("Build failed.");
            if !branch.is_empty() {
                eprintln!("  Branch:  {branch}");
            }
            eprintln!("  Error:   {message}");
        }
    }
}

/// An [`hive_agents::AiExecutor`] that routes each request through the policy/
/// cost router and dispatches to the resolved (egress-redacting) provider —
/// mirrors the desktop swarm executor. Resolves the swarm's `"auto"` sentinel
/// and concrete model ids alike, so the headless build uses the same routing
/// (and redaction) as the app.
struct RoutingExecutor {
    handle: hive_ai::AiRoutingHandle,
}

impl hive_agents::AiExecutor for RoutingExecutor {
    async fn execute(
        &self,
        request: &hive_ai::types::ChatRequest,
    ) -> std::result::Result<hive_ai::types::ChatResponse, String> {
        let (provider, resolved) = self
            .handle
            .route(&request.messages, &request.model)
            .ok_or_else(|| "no AI provider available for the requested model".to_string())?;
        let mut req = request.clone();
        req.model = resolved;
        provider.chat(&req).await.map_err(|e| e.to_string())
    }
}

/// A [`PrOpener`] that opens a GitHub **draft** PR via [`GitHubClient`].
struct GitHubDraftPrOpener {
    client: GitHubClient,
    owner: String,
    repo: String,
    base: String,
}

#[async_trait]
impl PrOpener for GitHubDraftPrOpener {
    async fn open_draft_pr(&self, branch: &str, title: &str, body: &str) -> Result<String, String> {
        let value = self
            .client
            .create_draft_pull(&self.owner, &self.repo, title, body, branch, &self.base)
            .await
            .map_err(|e| e.to_string())?;
        // The GitHub API returns the PR's web URL under `html_url`.
        let url = value
            .get("html_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("PR opened on {}/{} from {branch}", self.owner, self.repo));
        Ok(url)
    }
}

/// A [`PrOpener`] that is never invoked (used when approval is required, so no
/// PR is ever opened). Returns an error if somehow called, to be safe.
struct NoopPrOpener;

#[async_trait]
impl PrOpener for NoopPrOpener {
    async fn open_draft_pr(
        &self,
        _branch: &str,
        _title: &str,
        _body: &str,
    ) -> Result<String, String> {
        Err("internal error: PR opener invoked while approval was required".to_string())
    }
}
