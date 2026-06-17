//! GitHub Issues REST API client.
//!
//! Wraps the GitHub REST API at `https://api.github.com` using `reqwest` for
//! HTTP and Bearer (personal access token) authentication. Implements the
//! [`ProjectManagementProvider`] trait so GitHub Issues can be used as a ticket
//! source alongside Jira and Linear.
//!
//! GitHub issues are scoped to a repository (`owner/repo`). The trait's
//! `project_id` parameter is therefore interpreted as an `"owner/repo"` slug.
//! When a method receives a bare issue number (no `owner/repo` prefix), the
//! optional default repository configured on the client is used.
//!
//! Note: the GitHub "issues" endpoint also returns pull requests. Those are
//! filtered out here (a PR object carries a `pull_request` field) so callers
//! only ever see real issues.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use tracing::{debug, warn};

use super::{
    Comment, CreateIssueRequest, Issue, IssueFilters, IssuePriority, IssueStatus, IssueUpdate,
    PMPlatform, Project, ProjectManagementProvider, Sprint,
};

const DEFAULT_BASE_URL: &str = "https://api.github.com";

// ── GitHub API response types ────────────────────────────────────

// These structs map to GitHub's issues JSON schema. Some fields are kept for
// completeness even when they are not directly read in Rust code.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubIssue {
    /// GitHub's internal numeric database id (globally unique).
    id: u64,
    /// The per-repository issue number (what users see, e.g. `#42`).
    number: u64,
    title: String,
    body: Option<String>,
    /// `open` or `closed`.
    state: Option<String>,
    /// Reason for a closed state, e.g. `completed` or `not_planned`.
    state_reason: Option<String>,
    html_url: Option<String>,
    assignee: Option<GitHubUser>,
    #[serde(default)]
    labels: Vec<GitHubLabel>,
    milestone: Option<GitHubMilestone>,
    created_at: Option<String>,
    updated_at: Option<String>,
    /// Present only when the entry is actually a pull request, not an issue.
    pull_request: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
    id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubMilestone {
    title: Option<String>,
    number: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GitHubComment {
    id: u64,
    body: Option<String>,
    user: Option<GitHubUser>,
    created_at: Option<String>,
}

// ── Client ─────────────────────────────────────────────────────────

/// GitHub Issues REST API client.
pub struct GitHubIssuesClient {
    base_url: String,
    client: Client,
    /// Optional default `owner/repo` used when a call omits the repository.
    default_repo: Option<(String, String)>,
}

impl GitHubIssuesClient {
    /// Create a new GitHub Issues client with the given personal access token.
    pub fn new(token: &str) -> Result<Self> {
        Self::with_base_url(token, DEFAULT_BASE_URL, None)
    }

    /// Create a client with a default `owner/repo` target used when a call
    /// supplies only a bare issue number.
    pub fn with_default_repo(token: &str, default_repo: Option<&str>) -> Result<Self> {
        Self::with_base_url(token, DEFAULT_BASE_URL, default_repo)
    }

    /// Create a client pointing at a custom base URL (useful for GitHub
    /// Enterprise or testing against a mock server).
    pub fn with_base_url(
        token: &str,
        base_url: &str,
        default_repo: Option<&str>,
    ) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("invalid characters in GitHub token")?;
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(USER_AGENT, HeaderValue::from_static("Hive/1.0"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for GitHub")?;

        let default_repo = default_repo
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(Self::parse_repo_slug);

        Ok(Self {
            base_url,
            client,
            default_repo,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Split an `"owner/repo"` slug into its components.
    fn parse_repo_slug(slug: &str) -> Option<(String, String)> {
        let slug = slug.trim().trim_start_matches('/');
        let mut parts = slug.splitn(2, '/');
        let owner = parts.next()?.trim();
        let repo = parts.next()?.trim();
        if owner.is_empty() || repo.is_empty() {
            return None;
        }
        Some((owner.to_string(), repo.to_string()))
    }

    /// Resolve a `project_id` slug to `(owner, repo)`, falling back to the
    /// configured default repository when the slug is empty.
    fn resolve_repo(&self, project_id: &str) -> Result<(String, String)> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return self
                .default_repo
                .clone()
                .context("no repository specified and no default GitHub repo configured");
        }
        Self::parse_repo_slug(project_id).context(format!(
            "invalid GitHub repository '{project_id}'; expected 'owner/repo'"
        ))
    }

    /// Resolve an `issue_id` to `(owner, repo, number)`.
    ///
    /// Accepts `"owner/repo#42"`, `"owner/repo/42"`, or a bare `"42"` (which
    /// uses the configured default repository).
    fn resolve_issue_ref(&self, issue_id: &str) -> Result<(String, String, u64)> {
        let issue_id = issue_id.trim();

        // Form: owner/repo#number
        if let Some((repo_part, num_part)) = issue_id.rsplit_once('#') {
            let (owner, repo) = self.resolve_repo(repo_part)?;
            let number = Self::parse_issue_number(num_part)?;
            return Ok((owner, repo, number));
        }

        // Form: bare number -> default repo
        if let Ok(number) = issue_id.parse::<u64>() {
            let (owner, repo) = self
                .default_repo
                .clone()
                .context("bare issue number given but no default GitHub repo configured")?;
            return Ok((owner, repo, number));
        }

        // Form: owner/repo/number
        let segments: Vec<&str> = issue_id.split('/').collect();
        if segments.len() == 3 {
            let owner = segments[0].trim();
            let repo = segments[1].trim();
            let number = Self::parse_issue_number(segments[2])?;
            if owner.is_empty() || repo.is_empty() {
                anyhow::bail!("invalid GitHub issue reference '{issue_id}'");
            }
            return Ok((owner.to_string(), repo.to_string(), number));
        }

        anyhow::bail!(
            "invalid GitHub issue reference '{issue_id}'; expected 'owner/repo#number', 'owner/repo/number', or a bare number"
        )
    }

    /// Parse an issue number, tolerating a leading `#`.
    fn parse_issue_number(s: &str) -> Result<u64> {
        s.trim()
            .trim_start_matches('#')
            .parse::<u64>()
            .context(format!("invalid GitHub issue number '{s}'"))
    }

    /// Perform an authenticated GET request and parse the JSON response.
    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        debug!(url = %url, "GitHub GET request");

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("GitHub GET request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error ({}): {}", status, body);
        }

        resp.json::<T>()
            .await
            .context("failed to parse GitHub response")
    }

    /// Perform an authenticated POST request with a JSON body.
    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<T> {
        debug!(url = %url, "GitHub POST request");

        let resp = self
            .client
            .post(url)
            .json(payload)
            .send()
            .await
            .context("GitHub POST request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error ({}): {}", status, body);
        }

        resp.json::<T>()
            .await
            .context("failed to parse GitHub response")
    }

    /// Perform an authenticated PATCH request with a JSON body.
    async fn patch<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<T> {
        debug!(url = %url, "GitHub PATCH request");

        let resp = self
            .client
            .patch(url)
            .json(payload)
            .send()
            .await
            .context("GitHub PATCH request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error ({}): {}", status, body);
        }

        resp.json::<T>()
            .await
            .context("failed to parse GitHub response")
    }

    /// Convert a GitHub issue to our common Issue type.
    ///
    /// `owner`/`repo` are needed to construct the stable `owner/repo#number`
    /// id, since the GitHub payload exposes only the per-repo `number`.
    fn convert_issue(owner: &str, repo: &str, gh: &GitHubIssue) -> Issue {
        let status = Self::map_status(gh.state.as_deref(), gh.state_reason.as_deref());
        let priority = Self::map_priority(&gh.labels);
        let assignee = gh.assignee.as_ref().map(|u| u.login.clone());
        let labels: Vec<String> = gh.labels.iter().map(|l| l.name.clone()).collect();
        let sprint = gh.milestone.as_ref().and_then(|m| m.title.clone());

        Issue {
            id: format!("{owner}/{repo}#{}", gh.number),
            key: Some(format!("#{}", gh.number)),
            title: gh.title.clone(),
            description: gh.body.clone(),
            status,
            priority,
            assignee,
            labels,
            sprint,
            created_at: gh.created_at.as_deref().and_then(Self::parse_datetime),
            updated_at: gh.updated_at.as_deref().and_then(Self::parse_datetime),
            platform: PMPlatform::GitHubProjects,
            url: gh.html_url.clone(),
        }
    }

    /// Map GitHub's `state` (+ `state_reason`) to our IssueStatus enum.
    ///
    /// GitHub issues only have `open`/`closed`; we additionally use the close
    /// reason (`not_planned` → cancelled) and `wontfix`-style labels are left
    /// to the priority/label mapping.
    fn map_status(state: Option<&str>, state_reason: Option<&str>) -> IssueStatus {
        match state {
            Some("closed") => match state_reason {
                Some("not_planned") => IssueStatus::Cancelled,
                _ => IssueStatus::Done,
            },
            _ => IssueStatus::Todo,
        }
    }

    /// Best-effort priority derivation from issue labels.
    ///
    /// GitHub issues have no native priority, so we look for common
    /// `priority:high`, `p1`, `critical`, etc. label conventions.
    fn map_priority(labels: &[GitHubLabel]) -> IssuePriority {
        for label in labels {
            let name = label.name.to_lowercase();
            let name = name.trim();
            match name {
                "priority: critical" | "priority:critical" | "critical" | "p0" | "blocker" => {
                    return IssuePriority::Critical;
                }
                "priority: high" | "priority:high" | "high" | "p1" => {
                    return IssuePriority::High;
                }
                "priority: medium" | "priority:medium" | "medium" | "p2" => {
                    return IssuePriority::Medium;
                }
                "priority: low" | "priority:low" | "low" | "p3" => {
                    return IssuePriority::Low;
                }
                _ => {}
            }
        }
        IssuePriority::None
    }

    /// Map our IssueStatus to a GitHub issue `state` value for updates.
    fn status_to_github_state(status: IssueStatus) -> &'static str {
        match status {
            IssueStatus::Done | IssueStatus::Cancelled => "closed",
            _ => "open",
        }
    }

    /// Parse an ISO 8601 datetime string to `DateTime<Utc>`.
    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Map an [`IssueFilters`] status to GitHub's `state` query parameter.
    fn filter_state_param(filters: &IssueFilters) -> &'static str {
        match filters.status {
            Some(IssueStatus::Done) | Some(IssueStatus::Cancelled) => "closed",
            Some(_) => "open",
            None => "all",
        }
    }
}

#[async_trait]
impl ProjectManagementProvider for GitHubIssuesClient {
    fn platform(&self) -> PMPlatform {
        PMPlatform::GitHubProjects
    }

    async fn list_projects(&self) -> Result<Vec<Project>> {
        // GitHub issues are repository-scoped; there is no general "list
        // projects" REST endpoint that maps cleanly here. Surface the
        // configured default repo (if any) so hub aggregation works.
        match &self.default_repo {
            Some((owner, repo)) => Ok(vec![Project {
                id: format!("{owner}/{repo}"),
                name: format!("{owner}/{repo}"),
                key: Some(repo.clone()),
                description: None,
                platform: PMPlatform::GitHubProjects,
            }]),
            None => Ok(vec![]),
        }
    }

    async fn list_issues(&self, project_id: &str, filters: &IssueFilters) -> Result<Vec<Issue>> {
        let (owner, repo) = self.resolve_repo(project_id)?;

        let state = Self::filter_state_param(filters);
        let mut url = format!(
            "{}/repos/{owner}/{repo}/issues?state={state}&per_page=50",
            self.base_url
        );

        // GitHub supports server-side label filtering via a comma-separated list.
        if !filters.labels.is_empty() {
            url.push_str("&labels=");
            url.push_str(&filters.labels.join(","));
        }

        if let Some(ref assignee) = filters.assignee {
            url.push_str("&assignee=");
            url.push_str(assignee);
        }

        let issues: Vec<GitHubIssue> = self.get(&url).await?;

        Ok(issues
            .iter()
            // The issues endpoint also returns pull requests; drop them.
            .filter(|i| i.pull_request.is_none())
            .map(|i| Self::convert_issue(&owner, &repo, i))
            .collect())
    }

    async fn get_issue(&self, issue_id: &str) -> Result<Issue> {
        let (owner, repo, number) = self.resolve_issue_ref(issue_id)?;
        let url = format!("{}/repos/{owner}/{repo}/issues/{number}", self.base_url);

        let gh: GitHubIssue = self.get(&url).await?;
        Ok(Self::convert_issue(&owner, &repo, &gh))
    }

    async fn create_issue(&self, request: &CreateIssueRequest) -> Result<Issue> {
        let (owner, repo) = self.resolve_repo(&request.project_id)?;
        let url = format!("{}/repos/{owner}/{repo}/issues", self.base_url);

        let mut payload = serde_json::json!({ "title": request.title });

        if let Some(ref desc) = request.description {
            payload["body"] = serde_json::json!(desc);
        }
        if !request.labels.is_empty() {
            payload["labels"] = serde_json::json!(request.labels);
        }
        if let Some(ref assignee) = request.assignee {
            payload["assignees"] = serde_json::json!([assignee]);
        }

        let gh: GitHubIssue = self.post(&url, &payload).await?;
        Ok(Self::convert_issue(&owner, &repo, &gh))
    }

    async fn update_issue(&self, issue_id: &str, update: &IssueUpdate) -> Result<Issue> {
        let (owner, repo, number) = self.resolve_issue_ref(issue_id)?;
        let url = format!("{}/repos/{owner}/{repo}/issues/{number}", self.base_url);

        let mut payload = serde_json::Map::new();

        if let Some(ref title) = update.title {
            payload.insert("title".into(), serde_json::json!(title));
        }
        if let Some(ref desc) = update.description {
            payload.insert("body".into(), serde_json::json!(desc));
        }
        if let Some(ref labels) = update.labels {
            payload.insert("labels".into(), serde_json::json!(labels));
        }
        if let Some(ref assignee) = update.assignee {
            payload.insert("assignees".into(), serde_json::json!([assignee]));
        }
        if let Some(status) = update.status {
            payload.insert(
                "state".into(),
                serde_json::json!(Self::status_to_github_state(status)),
            );
        }

        let gh: GitHubIssue = self
            .patch(&url, &serde_json::Value::Object(payload))
            .await?;
        Ok(Self::convert_issue(&owner, &repo, &gh))
    }

    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<Comment> {
        let (owner, repo, number) = self.resolve_issue_ref(issue_id)?;
        let url = format!(
            "{}/repos/{owner}/{repo}/issues/{number}/comments",
            self.base_url
        );

        let payload = serde_json::json!({ "body": body });
        let gh: GitHubComment = self.post(&url, &payload).await?;

        Ok(Comment {
            id: gh.id.to_string(),
            author: gh.user.map(|u| u.login),
            body: gh.body.unwrap_or_default(),
            created_at: gh.created_at.as_deref().and_then(Self::parse_datetime),
        })
    }

    async fn transition_issue(&self, issue_id: &str, status: IssueStatus) -> Result<Issue> {
        let (owner, repo, number) = self.resolve_issue_ref(issue_id)?;
        let url = format!("{}/repos/{owner}/{repo}/issues/{number}", self.base_url);

        let mut payload = serde_json::json!({
            "state": Self::status_to_github_state(status),
        });
        // Distinguish "cancelled" from "done" via the close reason.
        if status == IssueStatus::Cancelled {
            payload["state_reason"] = serde_json::json!("not_planned");
        } else if status == IssueStatus::Done {
            payload["state_reason"] = serde_json::json!("completed");
        }

        let gh: GitHubIssue = self.patch(&url, &payload).await?;
        Ok(Self::convert_issue(&owner, &repo, &gh))
    }

    async fn search_issues(&self, query: &str, limit: u32) -> Result<Vec<Issue>> {
        // Use GitHub's search API, restricted to issues (not PRs).
        let full_query = format!("{query} type:issue");
        let encoded = encode_query_component(&full_query);
        let url = format!(
            "{}/search/issues?q={encoded}&per_page={limit}",
            self.base_url
        );

        #[derive(Debug, Deserialize)]
        struct SearchResponse {
            #[serde(default)]
            items: Vec<GitHubIssue>,
        }

        let resp: SearchResponse = self.get(&url).await?;

        Ok(resp
            .items
            .iter()
            .filter(|i| i.pull_request.is_none())
            .map(|i| {
                // The search payload exposes the repository via html_url; derive
                // owner/repo from it so the synthesized id remains stable.
                let (owner, repo) = i
                    .html_url
                    .as_deref()
                    .and_then(owner_repo_from_html_url)
                    .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
                Self::convert_issue(&owner, &repo, i)
            })
            .collect())
    }

    async fn get_sprints(&self, project_id: &str) -> Result<Vec<Sprint>> {
        // GitHub has no sprints; milestones are the closest analog. Fetch open
        // milestones for the repo and surface them as sprints. Failures are
        // logged and yield an empty list rather than erroring the caller.
        let (owner, repo) = match self.resolve_repo(project_id) {
            Ok(pair) => pair,
            Err(e) => {
                warn!(project_id = %project_id, error = %e, "cannot resolve repo for milestones");
                return Ok(vec![]);
            }
        };

        let url = format!(
            "{}/repos/{owner}/{repo}/milestones?state=all&per_page=50",
            self.base_url
        );

        #[derive(Debug, Deserialize)]
        struct ApiMilestone {
            number: u64,
            title: String,
            state: Option<String>,
            due_on: Option<String>,
        }

        let milestones: Vec<ApiMilestone> = match self.get(&url).await {
            Ok(m) => m,
            Err(e) => {
                warn!(owner = %owner, repo = %repo, error = %e, "failed to fetch GitHub milestones");
                return Ok(vec![]);
            }
        };

        Ok(milestones
            .into_iter()
            .map(|m| Sprint {
                id: m.number.to_string(),
                name: m.title,
                state: m.state,
                start_date: None,
                end_date: m.due_on.as_deref().and_then(Self::parse_datetime),
            })
            .collect())
    }
}

/// Extract `(owner, repo)` from a GitHub issue `html_url` such as
/// `https://github.com/owner/repo/issues/42`.
fn owner_repo_from_html_url(html_url: &str) -> Option<(String, String)> {
    let after_host = html_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split_once('/')
        .map(|(_, rest)| rest)?;
    let mut parts = after_host.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// Minimal percent-encoding for a URL query component value.
fn encode_query_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn make_client() -> GitHubIssuesClient {
        GitHubIssuesClient::with_base_url(
            "ghp_test_token_123",
            "https://api.github.com",
            Some("hive-org/hive"),
        )
        .unwrap()
    }

    fn sample_issue() -> GitHubIssue {
        GitHubIssue {
            id: 1001,
            number: 42,
            title: "Fix login bug".into(),
            body: Some("Users cannot log in".into()),
            state: Some("open".into()),
            state_reason: None,
            html_url: Some("https://github.com/hive-org/hive/issues/42".into()),
            assignee: Some(GitHubUser {
                login: "alice".into(),
                id: Some(7),
            }),
            labels: vec![
                GitHubLabel { name: "bug".into() },
                GitHubLabel {
                    name: "priority: high".into(),
                },
            ],
            milestone: Some(GitHubMilestone {
                title: Some("v1.0".into()),
                number: Some(3),
            }),
            created_at: Some("2024-01-15T10:30:00Z".into()),
            updated_at: Some("2024-01-16T14:00:00Z".into()),
            pull_request: None,
        }
    }

    #[test]
    fn test_new_sets_default_base_url() {
        let client = GitHubIssuesClient::new("ghp_tok").unwrap();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_custom_base_url_strips_trailing_slash() {
        let client =
            GitHubIssuesClient::with_base_url("tok", "https://github.example.com/api/v3/", None)
                .unwrap();
        assert_eq!(client.base_url(), "https://github.example.com/api/v3");
    }

    #[test]
    fn test_platform() {
        let client = make_client();
        assert_eq!(client.platform(), PMPlatform::GitHubProjects);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = GitHubIssuesClient::new("tok\nen");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_repo_slug() {
        assert_eq!(
            GitHubIssuesClient::parse_repo_slug("owner/repo"),
            Some(("owner".into(), "repo".into()))
        );
        assert_eq!(
            GitHubIssuesClient::parse_repo_slug("/owner/repo"),
            Some(("owner".into(), "repo".into()))
        );
        assert_eq!(GitHubIssuesClient::parse_repo_slug("noseparator"), None);
        assert_eq!(GitHubIssuesClient::parse_repo_slug("owner/"), None);
        assert_eq!(GitHubIssuesClient::parse_repo_slug("/repo"), None);
    }

    #[test]
    fn test_resolve_repo_explicit() {
        let client = make_client();
        let (owner, repo) = client.resolve_repo("acme/widget").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "widget");
    }

    #[test]
    fn test_resolve_repo_falls_back_to_default() {
        let client = make_client();
        let (owner, repo) = client.resolve_repo("").unwrap();
        assert_eq!(owner, "hive-org");
        assert_eq!(repo, "hive");
    }

    #[test]
    fn test_resolve_repo_no_default_errors() {
        let client = GitHubIssuesClient::new("tok").unwrap();
        assert!(client.resolve_repo("").is_err());
    }

    #[test]
    fn test_resolve_issue_ref_full() {
        let client = make_client();
        let (owner, repo, number) = client.resolve_issue_ref("acme/widget#7").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "widget");
        assert_eq!(number, 7);
    }

    #[test]
    fn test_resolve_issue_ref_slash_form() {
        let client = make_client();
        let (owner, repo, number) = client.resolve_issue_ref("acme/widget/9").unwrap();
        assert_eq!(owner, "acme");
        assert_eq!(repo, "widget");
        assert_eq!(number, 9);
    }

    #[test]
    fn test_resolve_issue_ref_bare_number_uses_default() {
        let client = make_client();
        let (owner, repo, number) = client.resolve_issue_ref("42").unwrap();
        assert_eq!(owner, "hive-org");
        assert_eq!(repo, "hive");
        assert_eq!(number, 42);
    }

    #[test]
    fn test_resolve_issue_ref_bare_number_no_default_errors() {
        let client = GitHubIssuesClient::new("tok").unwrap();
        assert!(client.resolve_issue_ref("42").is_err());
    }

    #[test]
    fn test_resolve_issue_ref_invalid() {
        let client = make_client();
        assert!(client.resolve_issue_ref("not-a-ref").is_err());
    }

    #[test]
    fn test_map_status_open() {
        assert_eq!(
            GitHubIssuesClient::map_status(Some("open"), None),
            IssueStatus::Todo
        );
    }

    #[test]
    fn test_map_status_closed_completed() {
        assert_eq!(
            GitHubIssuesClient::map_status(Some("closed"), Some("completed")),
            IssueStatus::Done
        );
        assert_eq!(
            GitHubIssuesClient::map_status(Some("closed"), None),
            IssueStatus::Done
        );
    }

    #[test]
    fn test_map_status_closed_not_planned() {
        assert_eq!(
            GitHubIssuesClient::map_status(Some("closed"), Some("not_planned")),
            IssueStatus::Cancelled
        );
    }

    #[test]
    fn test_map_priority_from_labels() {
        let labels = vec![GitHubLabel {
            name: "priority: high".into(),
        }];
        assert_eq!(
            GitHubIssuesClient::map_priority(&labels),
            IssuePriority::High
        );

        let labels = vec![GitHubLabel { name: "P0".into() }];
        assert_eq!(
            GitHubIssuesClient::map_priority(&labels),
            IssuePriority::Critical
        );

        let labels = vec![GitHubLabel { name: "bug".into() }];
        assert_eq!(
            GitHubIssuesClient::map_priority(&labels),
            IssuePriority::None
        );
    }

    #[test]
    fn test_status_to_github_state() {
        assert_eq!(
            GitHubIssuesClient::status_to_github_state(IssueStatus::Todo),
            "open"
        );
        assert_eq!(
            GitHubIssuesClient::status_to_github_state(IssueStatus::InProgress),
            "open"
        );
        assert_eq!(
            GitHubIssuesClient::status_to_github_state(IssueStatus::Done),
            "closed"
        );
        assert_eq!(
            GitHubIssuesClient::status_to_github_state(IssueStatus::Cancelled),
            "closed"
        );
    }

    #[test]
    fn test_filter_state_param() {
        let f = IssueFilters::default();
        assert_eq!(GitHubIssuesClient::filter_state_param(&f), "all");

        let f = IssueFilters {
            status: Some(IssueStatus::InProgress),
            ..Default::default()
        };
        assert_eq!(GitHubIssuesClient::filter_state_param(&f), "open");

        let f = IssueFilters {
            status: Some(IssueStatus::Done),
            ..Default::default()
        };
        assert_eq!(GitHubIssuesClient::filter_state_param(&f), "closed");
    }

    #[test]
    fn test_convert_issue() {
        let gh = sample_issue();
        let issue = GitHubIssuesClient::convert_issue("hive-org", "hive", &gh);

        assert_eq!(issue.id, "hive-org/hive#42");
        assert_eq!(issue.key.as_deref(), Some("#42"));
        assert_eq!(issue.title, "Fix login bug");
        assert_eq!(issue.description.as_deref(), Some("Users cannot log in"));
        assert_eq!(issue.status, IssueStatus::Todo);
        assert_eq!(issue.priority, IssuePriority::High);
        assert_eq!(issue.assignee.as_deref(), Some("alice"));
        assert_eq!(issue.labels, vec!["bug", "priority: high"]);
        assert_eq!(issue.sprint.as_deref(), Some("v1.0"));
        assert_eq!(issue.platform, PMPlatform::GitHubProjects);
        assert_eq!(
            issue.url.as_deref(),
            Some("https://github.com/hive-org/hive/issues/42")
        );
        assert!(issue.created_at.is_some());
        assert!(issue.updated_at.is_some());
    }

    #[test]
    fn test_convert_issue_closed_cancelled() {
        let mut gh = sample_issue();
        gh.state = Some("closed".into());
        gh.state_reason = Some("not_planned".into());
        let issue = GitHubIssuesClient::convert_issue("hive-org", "hive", &gh);
        assert_eq!(issue.status, IssueStatus::Cancelled);
    }

    #[test]
    fn test_github_issue_deserialization_filters_pull_request() {
        let json = r#"{
            "id": 1,
            "number": 5,
            "title": "A PR not an issue",
            "state": "open",
            "pull_request": { "url": "https://api.github.com/repos/o/r/pulls/5" }
        }"#;
        let gh: GitHubIssue = serde_json::from_str(json).unwrap();
        assert!(gh.pull_request.is_some());
    }

    #[test]
    fn test_github_issue_deserialization_real_issue() {
        let json = r#"{
            "id": 1001,
            "number": 42,
            "title": "Fix login bug",
            "body": "Users cannot log in",
            "state": "open",
            "html_url": "https://github.com/hive-org/hive/issues/42",
            "labels": [{ "name": "bug" }],
            "created_at": "2024-01-15T10:30:00Z",
            "updated_at": "2024-01-16T14:00:00Z"
        }"#;
        let gh: GitHubIssue = serde_json::from_str(json).unwrap();
        assert!(gh.pull_request.is_none());
        let issue = GitHubIssuesClient::convert_issue("hive-org", "hive", &gh);
        assert_eq!(issue.title, "Fix login bug");
        assert_eq!(issue.id, "hive-org/hive#42");
        assert_eq!(issue.labels, vec!["bug"]);
    }

    #[test]
    fn test_owner_repo_from_html_url() {
        assert_eq!(
            owner_repo_from_html_url("https://github.com/octocat/hello-world/issues/3"),
            Some(("octocat".into(), "hello-world".into()))
        );
        assert_eq!(owner_repo_from_html_url("https://github.com/"), None);
    }

    #[test]
    fn test_encode_query_component() {
        assert_eq!(encode_query_component("hello world"), "hello%20world");
        assert_eq!(encode_query_component("type:issue"), "type%3Aissue");
        assert_eq!(encode_query_component("safe-1.2_x~"), "safe-1.2_x~");
    }

    #[test]
    fn test_parse_datetime_valid() {
        let dt = GitHubIssuesClient::parse_datetime("2024-01-15T10:30:00Z");
        assert!(dt.is_some());
        assert_eq!(dt.unwrap().year(), 2024);
    }

    #[test]
    fn test_parse_datetime_invalid() {
        assert!(GitHubIssuesClient::parse_datetime("not-a-date").is_none());
    }

    #[test]
    fn test_list_projects_with_default_repo() {
        // Synchronous check of the default-repo projection logic via a runtime.
        let client = make_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let projects = rt.block_on(client.list_projects()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "hive-org/hive");
        assert_eq!(projects[0].platform, PMPlatform::GitHubProjects);
    }

    #[test]
    fn test_list_projects_without_default_repo() {
        let client = GitHubIssuesClient::new("tok").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let projects = rt.block_on(client.list_projects()).unwrap();
        assert!(projects.is_empty());
    }
}
