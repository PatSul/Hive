# Codex Security Fixes Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 6 security/correctness issues identified by Codex audit — 3 high, 1 medium, 2 other gaps.

**Architecture:** Surgical fixes to existing files. No new crates or modules. SecurityGateway's existing `check_url()` is reused for SSRF fix. Channel name sanitization added inline. Config export/import gets password parameter plumbing. Tick driver gets workflow execution bridge. CI expanded to cover security-critical crates.

**Tech Stack:** Rust, GPUI, hive_core::SecurityGateway, hive_agents::AutomationService, hive_assistant::tick_driver

---

### Task 1: CallApi SSRF Fix — Route through SecurityGateway

**Files:**
- Modify: `hive/crates/hive_agents/src/automation.rs:848-865` (execute_call_api)
- Modify: `hive/crates/hive_agents/src/automation.rs:1063-1070` (validate_workflow_template CallApi validation)
- Modify: `hive/crates/hive_agents/src/automation.rs:1168+` (tests)

- [ ] **Step 1: Add SecurityGateway URL check in execute_call_api**

In `automation.rs`, add `use hive_core::SecurityGateway;` to the imports (line 1 area), then modify `execute_call_api` to validate the URL before sending:

```rust
async fn execute_call_api(url: &str, method: &str) -> std::result::Result<(), String> {
    // Validate URL through SecurityGateway before making the request
    let gateway = SecurityGateway::new();
    gateway.check_url(url).map_err(|e| format!("URL blocked by security policy: {e}"))?;

    let client = reqwest::Client::new();
    // ... rest unchanged
```

- [ ] **Step 2: Add SecurityGateway URL check in validate_workflow_template**

In the `CallApi` arm of `validate_workflow_template` (~line 1063), add URL validation at template load time:

```rust
ActionType::CallApi { url, method } => {
    if url.trim().is_empty() {
        bail!("step '{}' has an empty URL", step.name);
    }
    // Validate URL against security policy at load time
    let gateway = SecurityGateway::new();
    if let Err(e) = gateway.check_url(url) {
        bail!("step '{}' has a blocked URL: {e}", step.name);
    }
    let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"];
    // ... rest unchanged
```

- [ ] **Step 3: Add tests for SSRF blocking**

Add to the `tests` module:

```rust
#[test]
fn call_api_rejects_private_ip() {
    let mut svc = AutomationService::new();
    let wf = svc.create_workflow("SSRF Test", "test", TriggerType::ManualTrigger);
    let result = svc.add_step(
        &wf.id,
        "Evil",
        ActionType::CallApi {
            url: "http://169.254.169.254/metadata".into(),
            method: "GET".into(),
        },
    );
    // Step adds fine (validation is on activate)
    assert!(result.is_ok());
    // But activation with validation should fail
    let validated = AutomationService::validate_workflow_template(
        &serde_json::from_str::<WorkflowTemplate>(
            &serde_json::to_string(&WorkflowTemplate {
                name: "SSRF".into(),
                description: "test".into(),
                trigger: TriggerType::ManualTrigger,
                steps: vec![WorkflowStepTemplate {
                    name: "Evil".into(),
                    action: ActionType::CallApi {
                        url: "http://169.254.169.254/metadata".into(),
                        method: "GET".into(),
                    },
                    conditions: vec![],
                    timeout_secs: None,
                    retry_count: 0,
                }],
            }).unwrap()
        ).unwrap()
    );
    assert!(validated.is_err());
}

#[test]
fn call_api_rejects_localhost() {
    let gateway = hive_core::SecurityGateway::new();
    assert!(gateway.check_url("https://localhost/admin").is_err());
    assert!(gateway.check_url("https://127.0.0.1/secret").is_err());
    assert!(gateway.check_url("http://10.0.0.1/internal").is_err());
}
```

- [ ] **Step 4: Verify tests pass**

Run: `cd hive && cargo test -p hive_agents -- call_api_rejects`

---

### Task 2: SendMessage Path Traversal Fix

**Files:**
- Modify: `hive/crates/hive_agents/src/automation.rs:787-844` (execute_send_message)
- Modify: `hive/crates/hive_agents/src/automation.rs:1055-1061` (validate_workflow_template SendMessage arm)
- Modify: `hive/crates/hive_agents/src/automation.rs:1168+` (tests)

- [ ] **Step 1: Add channel name sanitization helper**

Add a private helper function near the action handlers section (~line 784):

```rust
/// Sanitize a channel name to prevent path traversal.
/// Only allows alphanumeric characters, hyphens, and underscores.
fn sanitize_channel_name(channel: &str) -> std::result::Result<String, String> {
    let sanitized: String = channel.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if sanitized.is_empty() {
        return Err("Channel name is empty after sanitization".into());
    }
    if sanitized != channel {
        return Err(format!(
            "Channel name '{}' contains invalid characters (only alphanumeric, hyphens, underscores allowed)",
            channel
        ));
    }
    Ok(sanitized)
}
```

- [ ] **Step 2: Use sanitization in execute_send_message**

In `execute_send_message` (~line 801), replace the raw channel interpolation:

```rust
// Before:
let channel_file = channels_dir.join(format!("{channel}.json"));

// After:
let safe_channel = Self::sanitize_channel_name(channel)?;
let channel_file = channels_dir.join(format!("{safe_channel}.json"));
```

Also fix the pending file path at ~line 838:
```rust
// Before:
let pending_file = pending_dir.join(format!("{channel}_{}.json", Uuid::new_v4()));

// After:
let pending_file = pending_dir.join(format!("{safe_channel}_{}.json", Uuid::new_v4()));
```

- [ ] **Step 3: Add validation in validate_workflow_template**

In the `SendMessage` arm (~line 1055), add channel name validation:

```rust
ActionType::SendMessage { channel, content } => {
    if channel.trim().is_empty() {
        bail!("step '{}' has an empty channel", step.name);
    }
    if !channel.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        bail!(
            "step '{}' has invalid channel name '{}' (only alphanumeric, hyphens, underscores allowed)",
            step.name, channel
        );
    }
    if content.trim().is_empty() {
        bail!("step '{}' has empty message content", step.name);
    }
}
```

- [ ] **Step 4: Add tests for path traversal blocking**

```rust
#[test]
fn send_message_rejects_path_traversal() {
    assert!(AutomationService::sanitize_channel_name("../../etc/cron").is_err());
    assert!(AutomationService::sanitize_channel_name("..\\windows\\system32").is_err());
    assert!(AutomationService::sanitize_channel_name("general/../../evil").is_err());
}

#[test]
fn send_message_allows_valid_channel_names() {
    assert_eq!(
        AutomationService::sanitize_channel_name("general").unwrap(),
        "general"
    );
    assert_eq!(
        AutomationService::sanitize_channel_name("my-channel_01").unwrap(),
        "my-channel_01"
    );
}
```

- [ ] **Step 5: Verify tests pass**

Run: `cd hive && cargo test -p hive_agents -- send_message`

---

### Task 3: Config Export/Import — Remove Hardcoded Password

**Files:**
- Modify: `hive/crates/hive_ui/src/workspace.rs:10244-10310` (handle_export_config)
- Modify: `hive/crates/hive_ui/src/workspace.rs:10311-10420` (handle_import_config)

- [ ] **Step 1: Replace hardcoded password in export with generated random passphrase**

Replace the hardcoded password block in `handle_export_config` (~line 10255-10257):

```rust
// Before:
// Password dialog will be wired in a future iteration; for now we use
// a fixed passphrase so exports are portable between machines.
let password = "hive-export-default";

// After:
// Generate a random 24-character passphrase and show it to the user.
// They must save this to import on another machine.
let password: String = {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    (0..24).map(|_| {
        let idx = rng.gen_range(0..CHARSET.len());
        CHARSET[idx] as char
    }).collect()
};
```

- [ ] **Step 2: Show the passphrase in the success notification**

Update the success notification (~line 10292-10295) to include the passphrase:

```rust
// After:
NotificationType::Success,
"Config Export",
format!(
    "Exported to {}\n\nPassphrase (save this — required for import):\n{}",
    export_path.display(),
    password
),
```

- [ ] **Step 3: Replace hardcoded password in import with a placeholder that warns**

In `handle_import_config` (~line 10391-10392), replace the hardcoded password with a notification that explains the feature now requires a passphrase:

```rust
// Before:
// Must match the password used during export.
let password = "hive-export-default";

// After:
// For now, attempt with empty password — the UI password input dialog
// is needed to complete this flow. Show an error directing users.
self.push_notification(
    cx,
    NotificationType::Warning,
    "Config Import",
    "Import requires the passphrase shown during export. \
     Password input dialog coming in a future update.".to_string(),
);
return;
```

Note: This intentionally disables import until the password input dialog is wired. Leaving import functional with a hardcoded password defeats the purpose of the fix. The export side generates and shows the password, but import needs a UI dialog to accept user input.

- [ ] **Step 4: Verify build passes**

Run: `cd hive && cargo check -p hive_ui`

---

### Task 4: Tick Driver — Wire Scheduled Workflow Execution

**Files:**
- Modify: `hive/crates/hive_assistant/src/tick_driver.rs:51-130`
- Modify: `hive/crates/hive_app/src/main.rs:530-650` (pass automation to tick driver)

- [ ] **Step 1: Add workflow execution callback to TickDriverConfig**

In `tick_driver.rs`, extend `TickDriverConfig` to accept a workflow callback:

```rust
pub struct TickDriverConfig {
    pub interval: Duration,
    pub assistant_db_path: String,
    pub reminder_tx: Option<std::sync::mpsc::Sender<Vec<crate::reminders::TriggeredReminder>>>,
    /// Optional callback to execute when a scheduled job fires.
    /// Receives the job ID string.
    pub job_executor: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}
```

Update the `Default` impl to include `job_executor: None`.

- [ ] **Step 2: Execute due jobs in the tick loop**

In `start_tick_driver`, after the logging loop for due jobs (~line 119-121), add execution:

```rust
for job_id in &due_jobs {
    info!("Tick driver: scheduled job due: {job_id}");
    if let Some(ref executor) = job_executor {
        executor(job_id);
    }
}
```

Where `job_executor` is extracted from config at the top of the closure (alongside `reminder_tx`):

```rust
let job_executor = config.job_executor;
```

- [ ] **Step 3: Wire AutomationService into tick driver in main.rs**

In `main.rs` after the `AutomationService` is set as global (~line 539), before the tick driver setup (~line 644), create the job executor closure that looks up workflows by trigger:

```rust
// Build a job executor closure that bridges scheduler → automation
let job_executor: Option<Arc<dyn Fn(&str) + Send + Sync>> = {
    // Note: In the current architecture, AutomationService lives in
    // GPUI global state and isn't Send. We log a TODO for full wiring
    // that requires moving execution to the UI thread via cx.spawn.
    // For now, log that the job fired — actual execution requires
    // dispatching an action to the workspace.
    Some(Arc::new(|job_id: &str| {
        tracing::info!("Scheduled job ready for execution: {job_id} — dispatch to workspace");
    }))
};
```

Then pass it to the tick config:

```rust
let tick_config = hive_assistant::tick_driver::TickDriverConfig {
    interval: Duration::from_secs(60),
    assistant_db_path: assistant_db_str.clone(),
    reminder_tx: Some(reminder_tx),
    job_executor,
};
```

- [ ] **Step 4: Verify build passes**

Run: `cd hive && cargo check -p hive_assistant -p hive_app`

---

### Task 5: Remove Dead ExecuteSkill Pending Writer

**Files:**
- Modify: `hive/crates/hive_agents/src/automation.rs:927-960` (execute_skill)

- [ ] **Step 1: Replace fire-and-forget write with a warning log**

Since there's no consumer for `pending_skills/`, the current code silently dead-letters. Replace the body of `execute_skill` to make this explicit:

```rust
fn execute_skill(skill_trigger: &str, input: &str) -> std::result::Result<(), String> {
    // ExecuteSkill is not yet wired end-to-end. The pending_skills/ directory
    // had no consumer. Log a warning instead of silently dead-lettering.
    warn!(
        skill_trigger,
        input_len = input.len(),
        "ExecuteSkill action is not yet implemented — no consumer for pending skills"
    );
    Err("ExecuteSkill is not yet wired — skill execution from workflows is not supported".into())
}
```

- [ ] **Step 2: Verify build passes**

Run: `cd hive && cargo check -p hive_agents`

---

### Task 6: Expand CI to Cover Security-Critical Crates

**Files:**
- Modify: `hive/verify.bat`
- Modify: `hive/verify.sh`

- [ ] **Step 1: Add hive_agents and hive_core tests to verify.bat**

Add a new step between the existing steps:

```bat
echo [2/4] Running security-critical crate tests...
call "%~dp0build.bat" test -p hive_core -p hive_agents -q
if errorlevel 1 exit /b %errorlevel%
```

Renumber remaining steps to [3/4] and [4/4].

- [ ] **Step 2: Add hive_agents and hive_core tests to verify.sh**

Add a matching step:

```bash
echo "[2/4] Running security-critical crate tests..."
cargo test -p hive_core -p hive_agents -q
```

Renumber remaining steps to [3/4] and [4/4].

- [ ] **Step 3: Verify locally**

Run: `cd hive && cargo test -p hive_core -p hive_agents -q`

---

### Task 7: Final Verification

- [ ] **Step 1: Full workspace check**

Run: `cd hive && cargo check -p hive_agents -p hive_core -p hive_ui -p hive_assistant -p hive_app`

- [ ] **Step 2: Run all affected tests**

Run: `cd hive && cargo test -p hive_agents -p hive_core -q`
