use super::*;

impl HiveWorkspace {
    pub(super) fn run_checked_git_command(
        &self,
        cx: &Context<Self>,
        args: &[&str],
        security_check: &str,
    ) -> Result<std::process::Output, String> {
        if cx.has_global::<AppSecurity>() {
            cx.global::<AppSecurity>().0.check_command(security_check)?;
        }

        std::process::Command::new("git")
            .args(args)
            .current_dir(&self.current_project_root)
            .output()
            .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))
    }

    pub(super) fn handle_review_stage_all(
        &mut self,
        _action: &ReviewStageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: stage all");
        match self.run_checked_git_command(cx, &["add", "-A"], "git add -A") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git add -A failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    pub(super) fn handle_review_unstage_all(
        &mut self,
        _action: &ReviewUnstageAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: unstage all");
        match self.run_checked_git_command(cx, &["reset", "HEAD"], "git reset HEAD") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git reset HEAD failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    pub(super) fn handle_review_commit(
        &mut self,
        _action: &ReviewCommit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: commit");
        let staged = self.review_data.staged_count;
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = if staged > 0 {
            format!("chore(review): apply {staged} staged change(s) ({timestamp})")
        } else {
            format!("chore(review): snapshot commit ({timestamp})")
        };

        match self.run_checked_git_command(cx, &["commit", "-m", &message], "git commit -m") {
            Ok(output) if output.status.success() => {
                let commit_hash = self
                    .run_checked_git_command(
                        cx,
                        &["rev-parse", "--short", "HEAD"],
                        "git rev-parse HEAD",
                    )
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "unknown".to_string());

                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Review",
                    format!("Created commit {commit_hash}"),
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let msg = if !stderr.trim().is_empty() {
                    stderr.trim().to_string()
                } else if !stdout.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    "git commit failed".to_string()
                };
                self.push_notification(cx, NotificationType::Warning, "Review", msg);
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    pub(super) fn handle_review_discard_all(
        &mut self,
        _action: &ReviewDiscardAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        info!("Review: discard all");
        match self.run_checked_git_command(cx, &["checkout", "--", "."], "git checkout -- .") {
            Ok(output) if output.status.success() => {
                self.review_data = ReviewData::from_git(&self.current_project_root);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Review",
                    format!("git checkout -- . failed: {}", stderr.trim()),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Review", e);
            }
        }
        cx.notify();
    }

    pub(super) fn handle_review_switch_tab(
        &mut self,
        action: &ReviewSwitchTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = GitOpsTab::parse_tab(&action.tab);
        self.review_data.active_tab = tab;
        match tab {
            GitOpsTab::Push => self.refresh_push_data(cx),
            GitOpsTab::Branches => self.refresh_branches_data(cx),
            GitOpsTab::Lfs => self.refresh_lfs_data(cx),
            GitOpsTab::Gitflow => self.refresh_gitflow_data(cx),
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn handle_review_ai_commit_message(
        &mut self,
        _action: &ReviewAiCommitMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let diff =
            match self.run_checked_git_command(cx, &["diff", "--cached"], "git diff --cached") {
                Ok(output) if output.status.success() => {
                    String::from_utf8_lossy(&output.stdout).to_string()
                }
                _ => {
                    self.push_notification(
                        cx,
                        NotificationType::Warning,
                        "Git Ops",
                        "Failed to get staged diff",
                    );
                    return;
                }
            };

        if diff.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "No staged changes to generate message for",
            );
            return;
        }

        self.review_data.ai_commit.generating = true;
        cx.notify();

        let truncated_diff = if diff.len() > 32000 {
            format!(
                "{}...\n[truncated — {} total chars]",
                &diff[..32000],
                diff.len()
            )
        } else {
            diff
        };

        let system_prompt = "You are a git commit message generator. Given the following diff of staged changes, write a clear, concise commit message following conventional commit format (type(scope): description). Keep the first line under 72 characters. Add a body paragraph if the changes are complex. Only output the commit message, nothing else.".to_string();

        let messages = vec![hive_ai::types::ChatMessage {
            role: hive_ai::types::MessageRole::User,
            content: format!(
                "Generate a commit message for this diff:\n\n{}",
                truncated_diff
            ),
            timestamp: chrono::Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        }];

        let model = self.status_bar.current_model.clone();

        let stream_setup = if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>()
                .0
                .prepare_stream(messages, &model, Some(system_prompt), None)
        } else {
            None
        };

        let Some((provider, request)) = stream_setup else {
            self.review_data.ai_commit.generating = false;
            self.push_notification(
                cx,
                NotificationType::Error,
                "Git Ops",
                "No AI provider available",
            );
            cx.notify();
            return;
        };

        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => return Err(format!("Runtime error: {e}")),
                    };
                    rt.block_on(async {
                        match provider.stream_chat(&request).await {
                            Ok(mut rx) => {
                                let mut accumulated = String::new();
                                while let Some(chunk) = rx.recv().await {
                                    accumulated.push_str(&chunk.content);
                                }
                                Ok(accumulated)
                            }
                            Err(e) => Err(format!("AI error: {e}")),
                        }
                    })
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.ai_commit.generating = false;
                    match result {
                        Ok(msg) => {
                            let msg = msg.trim().to_string();
                            workspace.review_data.ai_commit.generated_message = Some(msg.clone());
                            workspace.review_data.ai_commit.user_edited_message = msg;
                            workspace.push_notification(
                                cx,
                                NotificationType::Success,
                                "Git Ops",
                                "Commit message generated",
                            );
                        }
                        Err(e) => {
                            workspace.push_notification(
                                cx,
                                NotificationType::Error,
                                "Git Ops",
                                format!("AI generation failed: {e}"),
                            );
                        }
                    }
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_set_commit_message(
        &mut self,
        action: &ReviewSetCommitMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.ai_commit.user_edited_message = action.message.clone();
        cx.notify();
    }

    pub(super) fn handle_review_commit_with_message(
        &mut self,
        _action: &ReviewCommitWithMessage,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = self.review_data.ai_commit.user_edited_message.clone();
        if message.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "Commit message is empty",
            );
            return;
        }

        match self.run_checked_git_command(cx, &["commit", "-m", &message], "git commit") {
            Ok(output) if output.status.success() => {
                let commit_hash = self
                    .run_checked_git_command(cx, &["rev-parse", "--short", "HEAD"], "git rev-parse")
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.review_data.ai_commit = AiCommitState::default();
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Commit {commit_hash} created"),
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Commit failed: {stderr}"),
                );
            }
            Err(e) => {
                self.push_notification(cx, NotificationType::Error, "Git Ops", e);
            }
        }
        cx.notify();
    }

    pub(super) fn handle_review_push(
        &mut self,
        _action: &ReviewPush,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.push_data.push_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let output = std::process::Command::new("git")
                        .args(["push"])
                        .current_dir(&work_dir)
                        .output();
                    match output {
                        Ok(o) if o.status.success() => {
                            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                            Ok(format!("{stdout}{stderr}").trim().to_string())
                        }
                        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => Err(format!("Failed to push: {e}")),
                    }
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.push_data.push_in_progress = false;
                    match &result {
                        Ok(msg) => {
                            workspace.push_notification(
                                cx,
                                NotificationType::Success,
                                "Git Ops",
                                format!("Push successful: {msg}"),
                            );
                        }
                        Err(e) => {
                            workspace.push_notification(
                                cx,
                                NotificationType::Error,
                                "Git Ops",
                                format!("Push failed: {e}"),
                            );
                        }
                    }
                    workspace.review_data.push_data.last_push_result = Some(result);
                    workspace.refresh_push_data(cx);
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_push_set_upstream(
        &mut self,
        _action: &ReviewPushSetUpstream,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.push_data.push_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let branch = self.review_data.branch.clone();
        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let output = std::process::Command::new("git")
                        .args(["push", "--set-upstream", "origin", &branch])
                        .current_dir(&work_dir)
                        .output();
                    match output {
                        Ok(o) if o.status.success() => {
                            let combined = format!(
                                "{}{}",
                                String::from_utf8_lossy(&o.stdout),
                                String::from_utf8_lossy(&o.stderr)
                            );
                            Ok(combined.trim().to_string())
                        }
                        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => Err(format!("Failed to push: {e}")),
                    }
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.push_data.push_in_progress = false;
                    match &result {
                        Ok(msg) => workspace.push_notification(
                            cx,
                            NotificationType::Success,
                            "Git Ops",
                            format!("Push successful: {msg}"),
                        ),
                        Err(e) => workspace.push_notification(
                            cx,
                            NotificationType::Error,
                            "Git Ops",
                            format!("Push failed: {e}"),
                        ),
                    }
                    workspace.review_data.push_data.last_push_result = Some(result);
                    workspace.refresh_push_data(cx);
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_branch_refresh(
        &mut self,
        _action: &ReviewBranchRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_branches_data(cx);
        cx.notify();
    }

    pub(super) fn handle_review_branch_create(
        &mut self,
        _action: &ReviewBranchCreate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.review_data.branches_data.new_branch_name.clone();
        if name.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "Branch name is empty",
            );
            return;
        }
        match self.run_checked_git_command(cx, &["checkout", "-b", &name], "git checkout -b") {
            Ok(output) if output.status.success() => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Created and switched to branch: {name}"),
                );
                self.review_data.branches_data.new_branch_name.clear();
                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.refresh_branches_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Failed: {stderr}"),
                );
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    pub(super) fn handle_review_branch_switch(
        &mut self,
        action: &ReviewBranchSwitch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = &action.branch_name;
        match self.run_checked_git_command(cx, &["checkout", name], "git checkout") {
            Ok(output) if output.status.success() => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Switched to branch: {name}"),
                );
                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.refresh_branches_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Failed: {stderr}"),
                );
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    pub(super) fn handle_review_branch_delete_named(
        &mut self,
        action: &ReviewBranchDeleteNamed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = &action.branch_name;
        match self.run_checked_git_command(cx, &["branch", "-d", name], "git branch -d") {
            Ok(output) if output.status.success() => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Deleted branch: {name}"),
                );
                self.refresh_branches_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Failed: {stderr}"),
                );
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    pub(super) fn handle_review_branch_set_name(
        &mut self,
        action: &ReviewBranchSetName,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.branches_data.new_branch_name = action.name.clone();
        cx.notify();
    }

    pub(super) fn handle_review_pr_refresh(
        &mut self,
        _action: &ReviewPrRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_pr_data(cx);
    }

    pub(super) fn handle_review_pr_ai_generate(
        &mut self,
        _action: &ReviewPrAiGenerate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base = self.review_data.pr_data.pr_form.base_branch.clone();

        let commits = match self.run_checked_git_command(
            cx,
            &["log", &format!("{base}..HEAD"), "--oneline"],
            "git log",
        ) {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };

        let diff = match self.run_checked_git_command(
            cx,
            &["diff", &format!("{base}...HEAD")],
            "git diff",
        ) {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => String::new(),
        };

        if commits.trim().is_empty() && diff.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "No changes found between HEAD and base branch",
            );
            return;
        }

        self.review_data.pr_data.pr_form.ai_generating = true;
        cx.notify();

        let truncated_diff = if diff.len() > 32000 {
            format!("{}...\n[truncated]", &diff[..32000])
        } else {
            diff
        };

        let system_prompt = "You are a pull request description generator. Given commits and a diff, generate a PR title and markdown body.\n\nOutput format:\nTITLE: <title under 72 chars>\nBODY:\n## Summary\n<2-3 bullets>\n\n## Changes\n<list of key changes>\n\n## Testing\n<how to test>\n\nOnly output in this format, nothing else.".to_string();

        let messages = vec![hive_ai::types::ChatMessage {
            role: hive_ai::types::MessageRole::User,
            content: format!("Commits:\n{}\n\nDiff:\n{}", commits, truncated_diff),
            timestamp: chrono::Utc::now(),
            tool_call_id: None,
            tool_calls: None,
        }];

        let model = self.status_bar.current_model.clone();
        let stream_setup = if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>()
                .0
                .prepare_stream(messages, &model, Some(system_prompt), None)
        } else {
            None
        };

        let Some((provider, request)) = stream_setup else {
            self.review_data.pr_data.pr_form.ai_generating = false;
            self.push_notification(
                cx,
                NotificationType::Error,
                "Git Ops",
                "No AI provider available",
            );
            cx.notify();
            return;
        };

        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => return Err(format!("Runtime error: {e}")),
                    };
                    rt.block_on(async {
                        match provider.stream_chat(&request).await {
                            Ok(mut rx) => {
                                let mut accumulated = String::new();
                                while let Some(chunk) = rx.recv().await {
                                    accumulated.push_str(&chunk.content);
                                }
                                Ok(accumulated)
                            }
                            Err(e) => Err(format!("AI error: {e}")),
                        }
                    })
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.pr_data.pr_form.ai_generating = false;
                    match result {
                        Ok(text) => {
                            let text = text.trim();
                            if let Some(title_start) = text.find("TITLE:") {
                                let after_title = &text[title_start + 6..];
                                if let Some(body_start) = after_title.find("BODY:") {
                                    let title = after_title[..body_start].trim().to_string();
                                    let body = after_title[body_start + 5..].trim().to_string();
                                    workspace.review_data.pr_data.pr_form.title = title;
                                    workspace.review_data.pr_data.pr_form.body = body;
                                } else {
                                    workspace.review_data.pr_data.pr_form.title =
                                        after_title.lines().next().unwrap_or("").trim().to_string();
                                }
                            } else {
                                let lines: Vec<&str> = text.lines().collect();
                                workspace.review_data.pr_data.pr_form.title =
                                    lines.first().unwrap_or(&"").to_string();
                                workspace.review_data.pr_data.pr_form.body =
                                    lines[1..].join("\n").trim().to_string();
                            }
                            workspace.push_notification(
                                cx,
                                NotificationType::Success,
                                "Git Ops",
                                "PR description generated",
                            );
                        }
                        Err(e) => {
                            workspace.push_notification(
                                cx,
                                NotificationType::Error,
                                "Git Ops",
                                format!("AI generation failed: {e}"),
                            );
                        }
                    }
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_pr_create(
        &mut self,
        _action: &ReviewPrCreate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let token = match self.get_github_token(cx) {
            Some(t) => t,
            None => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    "GitHub not connected. Connect via Settings.",
                );
                return;
            }
        };

        let (owner, repo) = match self.parse_github_remote(cx) {
            Some(pair) => pair,
            None => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    "Could not parse GitHub owner/repo from remote",
                );
                return;
            }
        };

        let title = self.review_data.pr_data.pr_form.title.clone();
        let body = self.review_data.pr_data.pr_form.body.clone();
        let head = self.review_data.branch.clone();
        let base = self.review_data.pr_data.pr_form.base_branch.clone();

        if title.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "PR title is empty",
            );
            return;
        }

        self.review_data.pr_data.loading = true;
        cx.notify();

        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let rt =
                        tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {e}"))?;
                    rt.block_on(async {
                        let client = hive_integrations::GitHubClient::new(&token)
                            .map_err(|e| format!("GitHub client error: {e}"))?;
                        client
                            .create_pull(&owner, &repo, &title, &body, &head, &base)
                            .await
                            .map_err(|e| format!("GitHub API error: {e}"))
                    })
                })
                .join()
                .unwrap_or(Err("Thread panicked".into()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.pr_data.loading = false;
                    match result {
                        Ok(value) => {
                            let url = value
                                .get("html_url")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let number = value.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
                            workspace.push_notification(
                                cx,
                                NotificationType::Success,
                                "Git Ops",
                                format!("PR #{number} created: {url}"),
                            );
                            workspace.review_data.pr_data.pr_form = PrForm::default();
                        }
                        Err(e) => {
                            workspace.push_notification(cx, NotificationType::Error, "Git Ops", e);
                        }
                    }
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_pr_set_title(
        &mut self,
        action: &ReviewPrSetTitle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.pr_data.pr_form.title = action.title.clone();
        cx.notify();
    }

    pub(super) fn handle_review_pr_set_body(
        &mut self,
        action: &ReviewPrSetBody,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.pr_data.pr_form.body = action.body.clone();
        cx.notify();
    }

    pub(super) fn handle_review_pr_set_base(
        &mut self,
        action: &ReviewPrSetBase,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.pr_data.pr_form.base_branch = action.base.clone();
        cx.notify();
    }

    pub(super) fn handle_review_lfs_refresh(
        &mut self,
        _action: &ReviewLfsRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_lfs_data(cx);
        cx.notify();
    }

    pub(super) fn handle_review_lfs_track(
        &mut self,
        _action: &ReviewLfsTrack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pattern = self.review_data.lfs_data.new_pattern.clone();
        if pattern.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "LFS pattern is empty",
            );
            return;
        }
        match self.run_checked_git_command(cx, &["lfs", "track", &pattern], "git lfs track") {
            Ok(output) if output.status.success() => {
                let _ = self.run_checked_git_command(
                    cx,
                    &["add", ".gitattributes"],
                    "git add .gitattributes",
                );
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Now tracking: {pattern}"),
                );
                self.review_data.lfs_data.new_pattern.clear();
                self.refresh_lfs_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("LFS track failed: {stderr}"),
                );
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    pub(super) fn handle_review_lfs_untrack(
        &mut self,
        _action: &ReviewLfsUntrack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pattern = self.review_data.lfs_data.new_pattern.clone();
        if pattern.trim().is_empty() {
            self.push_notification(
                cx,
                NotificationType::Warning,
                "Git Ops",
                "LFS pattern is empty",
            );
            return;
        }
        match self.run_checked_git_command(cx, &["lfs", "untrack", &pattern], "git lfs untrack") {
            Ok(output) if output.status.success() => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Untracked: {pattern}"),
                );
                self.review_data.lfs_data.new_pattern.clear();
                self.refresh_lfs_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("LFS untrack failed: {stderr}"),
                );
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    pub(super) fn handle_review_lfs_set_pattern(
        &mut self,
        action: &ReviewLfsSetPattern,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.lfs_data.new_pattern = action.pattern.clone();
        cx.notify();
    }

    pub(super) fn handle_review_lfs_pull(
        &mut self,
        _action: &ReviewLfsPull,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.lfs_data.lfs_pull_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let output = std::process::Command::new("git")
                        .args(["lfs", "pull"])
                        .current_dir(&work_dir)
                        .output();
                    match output {
                        Ok(o) if o.status.success() => {
                            Ok(String::from_utf8_lossy(&o.stdout).to_string())
                        }
                        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => Err(format!("LFS pull failed: {e}")),
                    }
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.lfs_data.lfs_pull_in_progress = false;
                    match result {
                        Ok(_) => workspace.push_notification(
                            cx,
                            NotificationType::Success,
                            "Git Ops",
                            "LFS pull complete",
                        ),
                        Err(e) => workspace.push_notification(
                            cx,
                            NotificationType::Error,
                            "Git Ops",
                            format!("LFS pull failed: {e}"),
                        ),
                    }
                    workspace.refresh_lfs_data(cx);
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_lfs_push(
        &mut self,
        _action: &ReviewLfsPush,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.lfs_data.lfs_push_in_progress = true;
        cx.notify();

        let work_dir = self.current_project_root.clone();
        let branch = self.review_data.branch.clone();
        let task = cx.spawn(
            async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                let result = std::thread::spawn(move || {
                    let output = std::process::Command::new("git")
                        .args(["lfs", "push", "origin", &branch])
                        .current_dir(&work_dir)
                        .output();
                    match output {
                        Ok(o) if o.status.success() => {
                            Ok(String::from_utf8_lossy(&o.stdout).to_string())
                        }
                        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
                        Err(e) => Err(format!("LFS push failed: {e}")),
                    }
                })
                .join()
                .unwrap_or(Err("Thread panicked".to_string()));

                let _ = this.update(app, |workspace, cx| {
                    workspace.review_data.lfs_data.lfs_push_in_progress = false;
                    match result {
                        Ok(_) => workspace.push_notification(
                            cx,
                            NotificationType::Success,
                            "Git Ops",
                            "LFS push complete",
                        ),
                        Err(e) => workspace.push_notification(
                            cx,
                            NotificationType::Error,
                            "Git Ops",
                            format!("LFS push failed: {e}"),
                        ),
                    }
                    workspace.refresh_lfs_data(cx);
                    cx.notify();
                });
            },
        );
        self._stream_task = Some(task);
    }

    pub(super) fn handle_review_gitflow_init(
        &mut self,
        _action: &ReviewGitflowInit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let commands: [(&str, &[&str]); 5] = [
            ("config", &["config", "gitflow.branch.master", "main"]),
            ("config", &["config", "gitflow.branch.develop", "develop"]),
            ("config", &["config", "gitflow.prefix.feature", "feature/"]),
            ("config", &["config", "gitflow.prefix.release", "release/"]),
            ("config", &["config", "gitflow.prefix.hotfix", "hotfix/"]),
        ];

        for (label, args) in &commands {
            if let Err(e) = self.run_checked_git_command(cx, args, &format!("git {label}")) {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Gitflow init failed: {e}"),
                );
                cx.notify();
                return;
            }
        }

        let branch_exists = self
            .run_checked_git_command(cx, &["rev-parse", "--verify", "develop"], "git rev-parse")
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !branch_exists {
            let _ = self.run_checked_git_command(cx, &["branch", "develop"], "git branch develop");
        }

        self.push_notification(
            cx,
            NotificationType::Success,
            "Git Ops",
            "Gitflow initialized",
        );
        self.refresh_gitflow_data(cx);
        cx.notify();
    }

    pub(super) fn handle_review_gitflow_start(
        &mut self,
        action: &ReviewGitflowStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = &action.name;
        if name.trim().is_empty() {
            self.push_notification(cx, NotificationType::Warning, "Git Ops", "Name is empty");
            return;
        }

        let gf = &self.review_data.gitflow_data;
        let (branch_name, base) = match action.kind.as_str() {
            "feature" => (
                format!("{}{}", gf.feature_prefix, name),
                gf.develop_branch.clone(),
            ),
            "release" => (
                format!("{}{}", gf.release_prefix, name),
                gf.develop_branch.clone(),
            ),
            "hotfix" => (
                format!("{}{}", gf.hotfix_prefix, name),
                gf.main_branch.clone(),
            ),
            _ => {
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Unknown gitflow kind: {}", action.kind),
                );
                return;
            }
        };

        match self.run_checked_git_command(
            cx,
            &["checkout", "-b", &branch_name, &base],
            "git checkout -b",
        ) {
            Ok(output) if output.status.success() => {
                self.push_notification(
                    cx,
                    NotificationType::Success,
                    "Git Ops",
                    format!("Started {} {}", action.kind, name),
                );
                self.review_data.gitflow_data.new_name.clear();
                self.review_data = ReviewData::from_git(&self.current_project_root);
                self.refresh_gitflow_data(cx);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                self.push_notification(
                    cx,
                    NotificationType::Error,
                    "Git Ops",
                    format!("Failed: {stderr}"),
                );
            }
            Err(e) => self.push_notification(cx, NotificationType::Error, "Git Ops", e),
        }
        cx.notify();
    }

    pub(super) fn handle_review_gitflow_finish_named(
        &mut self,
        action: &ReviewGitflowFinishNamed,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let gf = &self.review_data.gitflow_data;
        let branch_name = match action.kind.as_str() {
            "feature" => format!("{}{}", gf.feature_prefix, action.name),
            "release" => format!("{}{}", gf.release_prefix, action.name),
            "hotfix" => format!("{}{}", gf.hotfix_prefix, action.name),
            _ => return,
        };

        let main = gf.main_branch.clone();
        let develop = gf.develop_branch.clone();

        let run = |this: &mut Self, cx: &mut Context<Self>, args: &[&str]| -> bool {
            match this.run_checked_git_command(cx, args, &format!("git {}", args.join(" "))) {
                Ok(o) if o.status.success() => true,
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    this.push_notification(
                        cx,
                        NotificationType::Error,
                        "Git Ops",
                        format!("Failed: {stderr}"),
                    );
                    false
                }
                Err(e) => {
                    this.push_notification(cx, NotificationType::Error, "Git Ops", e);
                    false
                }
            }
        };

        match action.kind.as_str() {
            "feature" => {
                if !run(self, cx, &["checkout", &develop]) {
                    return;
                }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) {
                    return;
                }
                run(self, cx, &["branch", "-d", &branch_name]);
            }
            "release" => {
                if !run(self, cx, &["checkout", &main]) {
                    return;
                }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) {
                    return;
                }
                run(
                    self,
                    cx,
                    &[
                        "tag",
                        "-a",
                        &action.name,
                        "-m",
                        &format!("Release {}", action.name),
                    ],
                );
                if !run(self, cx, &["checkout", &develop]) {
                    return;
                }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) {
                    return;
                }
                run(self, cx, &["branch", "-d", &branch_name]);
            }
            "hotfix" => {
                if !run(self, cx, &["checkout", &main]) {
                    return;
                }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) {
                    return;
                }
                run(
                    self,
                    cx,
                    &[
                        "tag",
                        "-a",
                        &action.name,
                        "-m",
                        &format!("Hotfix {}", action.name),
                    ],
                );
                if !run(self, cx, &["checkout", &develop]) {
                    return;
                }
                if !run(self, cx, &["merge", "--no-ff", &branch_name]) {
                    return;
                }
                run(self, cx, &["branch", "-d", &branch_name]);
            }
            _ => return,
        }

        self.push_notification(
            cx,
            NotificationType::Success,
            "Git Ops",
            format!("Finished {} {}", action.kind, action.name),
        );
        self.review_data = ReviewData::from_git(&self.current_project_root);
        self.refresh_gitflow_data(cx);
        cx.notify();
    }

    pub(super) fn handle_review_gitflow_set_name(
        &mut self,
        action: &ReviewGitflowSetName,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review_data.gitflow_data.new_name = action.name.clone();
        cx.notify();
    }

    fn get_github_token(&self, cx: &Context<Self>) -> Option<String> {
        use hive_core::config::AccountPlatform;
        if !cx.has_global::<AppConfig>() {
            return None;
        }
        cx.global::<AppConfig>()
            .0
            .get_oauth_token(AccountPlatform::GitHub)
            .map(|t| t.access_token.clone())
    }

    fn parse_github_remote(&self, cx: &Context<Self>) -> Option<(String, String)> {
        let output = self
            .run_checked_git_command(
                cx,
                &["remote", "get-url", "origin"],
                "git remote get-url origin",
            )
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        parse_github_owner_repo(&url)
    }

    fn refresh_push_data(&mut self, cx: &Context<Self>) {
        if let Ok(output) = self.run_checked_git_command(
            cx,
            &["remote", "get-url", "origin"],
            "git remote get-url",
        ) && output.status.success()
        {
            self.review_data.push_data.remote_url =
                String::from_utf8_lossy(&output.stdout).trim().to_string();
        }

        if let Ok(output) = self.run_checked_git_command(
            cx,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            "git rev-parse",
        ) {
            if output.status.success() {
                self.review_data.push_data.tracking_branch =
                    Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
            } else {
                self.review_data.push_data.tracking_branch = None;
            }
        }

        if self.review_data.push_data.tracking_branch.is_some() {
            if let Ok(output) = self.run_checked_git_command(
                cx,
                &["rev-list", "--count", "@{u}..HEAD"],
                "git rev-list",
            ) && output.status.success()
            {
                self.review_data.push_data.ahead_count = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
            }
            if let Ok(output) = self.run_checked_git_command(
                cx,
                &["rev-list", "--count", "HEAD..@{u}"],
                "git rev-list",
            ) && output.status.success()
            {
                self.review_data.push_data.behind_count = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
            }
        }
    }

    fn refresh_branches_data(&mut self, cx: &Context<Self>) {
        let mut branches = Vec::new();

        let current = match self.run_checked_git_command(
            cx,
            &["rev-parse", "--abbrev-ref", "HEAD"],
            "git rev-parse",
        ) {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => String::new(),
        };
        self.review_data.branches_data.current_branch = current.clone();

        if let Ok(output) = self.run_checked_git_command(
            cx,
            &[
                "branch",
                "-a",
                "--format=%(refname:short)\t%(objectname:short)\t%(subject)",
            ],
            "git branch -a",
        ) && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(3, '\t').collect();
                if parts.is_empty() {
                    continue;
                }
                let name = parts[0].to_string();
                let is_remote = name.starts_with("origin/");
                if name.contains("HEAD") {
                    continue;
                }
                let commit_msg = parts.get(2).unwrap_or(&"").to_string();
                branches.push(BranchEntry {
                    is_current: name == current,
                    is_remote,
                    last_commit_msg: commit_msg,
                    last_commit_time: String::new(),
                    name,
                });
            }
        }

        self.review_data.branches_data.branches = branches;
    }

    fn refresh_lfs_data(&mut self, cx: &Context<Self>) {
        let lfs_installed = self
            .run_checked_git_command(cx, &["lfs", "version"], "git lfs version")
            .map(|o| o.status.success())
            .unwrap_or(false);
        self.review_data.lfs_data.is_lfs_installed = lfs_installed;

        if !lfs_installed {
            return;
        }

        let gitattributes_path = self.current_project_root.join(".gitattributes");
        let mut patterns = Vec::new();
        if let Ok(content) = std::fs::read_to_string(&gitattributes_path) {
            for line in content.lines() {
                if line.contains("filter=lfs")
                    && let Some(pattern) = line.split_whitespace().next()
                {
                    patterns.push(pattern.to_string());
                }
            }
        }
        self.review_data.lfs_data.tracked_patterns = patterns;

        let mut lfs_files = Vec::new();
        if let Ok(output) = self.run_checked_git_command(
            cx,
            &["lfs", "ls-files", "--long"],
            "git lfs ls-files",
        ) && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(3, ' ').collect();
                if parts.len() >= 3 {
                    lfs_files.push(LfsFileEntry {
                        oid: parts[0].to_string(),
                        is_pointer: parts[1] == "-",
                        path: parts[2].to_string(),
                        size: String::new(),
                    });
                }
            }
        }
        self.review_data.lfs_data.lfs_files = lfs_files;
    }

    fn refresh_gitflow_data(&mut self, cx: &Context<Self>) {
        let read_config = |this: &Self, cx: &Context<Self>, key: &str| -> Option<String> {
            this.run_checked_git_command(cx, &["config", key], &format!("git config {key}"))
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        };

        if let Some(main) = read_config(self, cx, "gitflow.branch.master") {
            self.review_data.gitflow_data.main_branch = main;
            self.review_data.gitflow_data.initialized = true;
        } else {
            self.review_data.gitflow_data.initialized = false;
            return;
        }

        if let Some(develop) = read_config(self, cx, "gitflow.branch.develop") {
            self.review_data.gitflow_data.develop_branch = develop;
        }
        if let Some(fp) = read_config(self, cx, "gitflow.prefix.feature") {
            self.review_data.gitflow_data.feature_prefix = fp;
        }
        if let Some(rp) = read_config(self, cx, "gitflow.prefix.release") {
            self.review_data.gitflow_data.release_prefix = rp;
        }
        if let Some(hp) = read_config(self, cx, "gitflow.prefix.hotfix") {
            self.review_data.gitflow_data.hotfix_prefix = hp;
        }

        let list_active = |this: &Self, cx: &Context<Self>, prefix: &str| -> Vec<String> {
            let mut active = Vec::new();
            if let Ok(output) = this.run_checked_git_command(
                cx,
                &["branch", "--list", &format!("{prefix}*")],
                "git branch --list",
            ) && output.status.success()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let name = line
                        .trim()
                        .trim_start_matches("* ")
                        .trim_start_matches(prefix);
                    if !name.is_empty() {
                        active.push(name.to_string());
                    }
                }
            }
            active
        };

        let fp = self.review_data.gitflow_data.feature_prefix.clone();
        let rp = self.review_data.gitflow_data.release_prefix.clone();
        let hp = self.review_data.gitflow_data.hotfix_prefix.clone();
        self.review_data.gitflow_data.active_features = list_active(self, cx, &fp);
        self.review_data.gitflow_data.active_releases = list_active(self, cx, &rp);
        self.review_data.gitflow_data.active_hotfixes = list_active(self, cx, &hp);
    }

    fn refresh_pr_data(&mut self, cx: &mut Context<Self>) {
        self.review_data.pr_data.loading = true;
        cx.notify();

        let _current_branch = match hive_fs::git::GitService::open(&self.current_project_root)
            .and_then(|gs| gs.current_branch())
        {
            Ok(b) => b,
            Err(e) => {
                warn!("refresh_pr_data: cannot read current branch: {e}");
                self.review_data.pr_data.loading = false;
                cx.notify();
                return;
            }
        };

        let github_token = self.get_github_token(cx);
        let github_remote = self.parse_github_remote(cx);

        if let (Some(token), Some((owner, repo))) = (github_token, github_remote) {
            self.review_data.pr_data.github_connected = true;

            let task =
                cx.spawn(
                    async move |this: WeakEntity<HiveWorkspace>, app: &mut AsyncApp| {
                        let result = std::thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new()
                                .map_err(|e| format!("Runtime error: {e}"))?;
                            rt.block_on(async {
                                let client = hive_integrations::GitHubClient::new(&token)
                                    .map_err(|e| format!("GitHub client error: {e}"))?;
                                let pulls = client
                                    .list_pulls(&owner, &repo)
                                    .await
                                    .map_err(|e| format!("GitHub API error: {e}"))?;

                                let summaries: Vec<PrSummary> = pulls
                                    .as_array()
                                    .unwrap_or(&Vec::new())
                                    .iter()
                                    .filter_map(|pr| {
                                        Some(PrSummary {
                                            number: pr.get("number")?.as_u64()?,
                                            title: pr.get("title")?.as_str()?.to_string(),
                                            author: pr
                                                .get("user")
                                                .and_then(|u| u.get("login"))
                                                .and_then(|l| l.as_str())
                                                .unwrap_or("unknown")
                                                .to_string(),
                                            head: pr
                                                .get("head")
                                                .and_then(|h| h.get("ref"))
                                                .and_then(|r| r.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            base: pr
                                                .get("base")
                                                .and_then(|b| b.get("ref"))
                                                .and_then(|r| r.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            state: pr
                                                .get("state")
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("open")
                                                .to_string(),
                                            created_at: pr
                                                .get("created_at")
                                                .and_then(|c| c.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            url: pr
                                                .get("html_url")
                                                .and_then(|u| u.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                        })
                                    })
                                    .collect();
                                Ok(summaries)
                            })
                        })
                        .join()
                        .unwrap_or(Err("Thread panicked".to_string()));

                        let _ = this.update(app, |workspace, cx| {
                            workspace.review_data.pr_data.loading = false;
                            match result {
                                Ok(prs) => {
                                    info!("PR refresh: fetched {} open PRs from GitHub", prs.len());
                                    workspace.review_data.pr_data.open_prs = prs;
                                }
                                Err(e) => {
                                    warn!(
                                        "PR refresh GitHub fetch failed, falling back to git log: {e}"
                                    );
                                    workspace.populate_pr_data_from_git_log();
                                }
                            }
                            cx.notify();
                        });
                    },
                );
            self._stream_task = Some(task);
        } else {
            self.review_data.pr_data.github_connected = false;
            self.populate_pr_data_from_git_log();
            self.review_data.pr_data.loading = false;
            cx.notify();
        }
    }

    fn populate_pr_data_from_git_log(&mut self) {
        let base = &self.review_data.pr_data.pr_form.base_branch;
        let git = match hive_fs::git::GitService::open(&self.current_project_root) {
            Ok(g) => g,
            Err(_) => return,
        };

        let branch = match git.current_branch() {
            Ok(b) => b,
            Err(_) => return,
        };

        if branch == *base {
            self.review_data.pr_data.open_prs.clear();
            return;
        }

        let commits = match git.log(20) {
            Ok(c) => c,
            Err(_) => return,
        };

        if commits.is_empty() {
            return;
        }

        let first_msg = commits
            .first()
            .map(|c| c.message.clone())
            .unwrap_or_default();
        let author = commits
            .first()
            .map(|c| c.author.clone())
            .unwrap_or_else(|| "local".to_string());

        let body_lines: Vec<String> = commits
            .iter()
            .take(10)
            .map(|c| format!("- {} ({})", c.message, &c.hash[..8.min(c.hash.len())]))
            .collect();

        self.review_data.pr_data.pr_form.title = first_msg.clone();
        self.review_data.pr_data.pr_form.body = body_lines.join("\n");

        let summary = PrSummary {
            number: 0,
            title: format!("[local] {}", first_msg),
            author,
            head: branch.clone(),
            base: base.clone(),
            state: "draft".to_string(),
            created_at: commits
                .first()
                .map(|c| {
                    chrono::DateTime::from_timestamp(c.timestamp, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            url: String::new(),
        };

        self.review_data.pr_data.open_prs = vec![summary];
    }
}
