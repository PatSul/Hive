use serde::{Deserialize, Serialize};

/// Reason an agent was paused.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PauseReason {
    BudgetExhausted,
    UserRequested,
    ApprovalTimeout,
    Error(String),
}

/// File operation type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileOp {
    Created,
    Modified,
    Deleted,
    Renamed { from: String },
}

/// Operation that may require approval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    ShellCommand(String),
    FileDelete(String),
    FileModify { path: String, scope: String },
    GitPush { remote: String, branch: String },
    AiCall { model: String, estimated_cost: f64 },
    Custom(String),
}

/// Every observable agent action in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityEvent {
    // Agent lifecycle
    AgentStarted {
        agent_id: String,
        role: String,
        task_id: Option<String>,
    },
    AgentCompleted {
        agent_id: String,
        duration_ms: u64,
        cost: f64,
    },
    AgentFailed {
        agent_id: String,
        error: String,
    },
    AgentPaused {
        agent_id: String,
        reason: PauseReason,
    },

    // Task lifecycle
    TaskClaimed {
        task_id: String,
        agent_id: String,
    },
    TaskProgress {
        task_id: String,
        progress: f64,
        message: String,
    },
    TaskCompleted {
        task_id: String,
        agent_id: String,
        cost: f64,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },

    // Tool/action execution
    ToolCalled {
        agent_id: String,
        tool_name: String,
        args_summary: String,
    },
    FileModified {
        agent_id: String,
        path: String,
        op: FileOp,
    },
    ShellExecuted {
        agent_id: String,
        command: String,
        exit_code: i32,
    },

    // Cost events
    CostIncurred {
        agent_id: String,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
    },
    BudgetWarning {
        agent_id: String,
        usage_pct: f64,
        limit_usd: f64,
    },
    BudgetExhausted {
        agent_id: String,
        limit_usd: f64,
    },

    // Approval events
    ApprovalRequested {
        request_id: String,
        agent_id: String,
        operation: String,
        context: String,
        rule: String,
    },
    ApprovalGranted {
        request_id: String,
    },
    ApprovalDenied {
        request_id: String,
        reason: Option<String>,
    },

    // Heartbeat events
    HeartbeatFired {
        agent_id: String,
        task_id: String,
    },
    HeartbeatScheduled {
        agent_id: String,
        interval_secs: u64,
    },
    HeartbeatCancelled {
        agent_id: String,
    },
}

impl ActivityEvent {
    /// Human-readable event type string for storage.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::AgentStarted { .. } => "agent_started",
            Self::AgentCompleted { .. } => "agent_completed",
            Self::AgentFailed { .. } => "agent_failed",
            Self::AgentPaused { .. } => "agent_paused",
            Self::TaskClaimed { .. } => "task_claimed",
            Self::TaskProgress { .. } => "task_progress",
            Self::TaskCompleted { .. } => "task_completed",
            Self::TaskFailed { .. } => "task_failed",
            Self::ToolCalled { .. } => "tool_called",
            Self::FileModified { .. } => "file_modified",
            Self::ShellExecuted { .. } => "shell_executed",
            Self::CostIncurred { .. } => "cost_incurred",
            Self::BudgetWarning { .. } => "budget_warning",
            Self::BudgetExhausted { .. } => "budget_exhausted",
            Self::ApprovalRequested { .. } => "approval_requested",
            Self::ApprovalGranted { .. } => "approval_granted",
            Self::ApprovalDenied { .. } => "approval_denied",
            Self::HeartbeatFired { .. } => "heartbeat_fired",
            Self::HeartbeatScheduled { .. } => "heartbeat_scheduled",
            Self::HeartbeatCancelled { .. } => "heartbeat_cancelled",
        }
    }

    /// Category for filtering in UI.
    pub fn category(&self) -> &'static str {
        match self {
            Self::AgentStarted { .. }
            | Self::AgentCompleted { .. }
            | Self::AgentFailed { .. }
            | Self::AgentPaused { .. } => "agent",
            Self::TaskClaimed { .. }
            | Self::TaskProgress { .. }
            | Self::TaskCompleted { .. }
            | Self::TaskFailed { .. } => "task",
            Self::ToolCalled { .. } | Self::FileModified { .. } | Self::ShellExecuted { .. } => {
                "tool"
            }
            Self::CostIncurred { .. }
            | Self::BudgetWarning { .. }
            | Self::BudgetExhausted { .. } => "cost",
            Self::ApprovalRequested { .. }
            | Self::ApprovalGranted { .. }
            | Self::ApprovalDenied { .. } => "approval",
            Self::HeartbeatFired { .. }
            | Self::HeartbeatScheduled { .. }
            | Self::HeartbeatCancelled { .. } => "heartbeat",
        }
    }

    /// Human-readable summary for display.
    pub fn summary(&self) -> String {
        match self {
            Self::AgentStarted {
                agent_id,
                role,
                task_id,
            } => {
                let task = task_id.as_deref().unwrap_or("no task");
                format!("{role} agent '{agent_id}' started (task: {task})")
            }
            Self::AgentCompleted {
                agent_id,
                duration_ms,
                cost,
            } => {
                format!("Agent '{agent_id}' completed in {duration_ms}ms (${cost:.4})")
            }
            Self::AgentFailed { agent_id, error } => {
                format!("Agent '{agent_id}' failed: {error}")
            }
            Self::AgentPaused { agent_id, reason } => {
                format!("Agent '{agent_id}' paused: {reason:?}")
            }
            Self::TaskClaimed { task_id, agent_id } => {
                format!("Agent '{agent_id}' claimed task '{task_id}'")
            }
            Self::TaskProgress {
                task_id,
                progress,
                message,
            } => {
                format!("Task '{task_id}': {:.0}% — {message}", progress * 100.0)
            }
            Self::TaskCompleted {
                task_id,
                agent_id,
                cost,
            } => {
                format!("Task '{task_id}' completed by '{agent_id}' (${cost:.4})")
            }
            Self::TaskFailed { task_id, error } => {
                format!("Task '{task_id}' failed: {error}")
            }
            Self::ToolCalled {
                agent_id,
                tool_name,
                ..
            } => {
                format!("Agent '{agent_id}' called tool '{tool_name}'")
            }
            Self::FileModified { agent_id, path, op } => {
                format!("Agent '{agent_id}' {op:?} '{path}'")
            }
            Self::ShellExecuted {
                agent_id,
                command,
                exit_code,
            } => {
                let status = if *exit_code == 0 { "ok" } else { "FAIL" };
                format!("Agent '{agent_id}' ran `{command}` [{status}]")
            }
            Self::CostIncurred {
                agent_id,
                model,
                cost_usd,
                input_tokens,
                output_tokens,
            } => {
                let total_tok = input_tokens + output_tokens;
                format!("Agent '{agent_id}' spent ${cost_usd:.4} on {model} ({total_tok} tokens)")
            }
            Self::BudgetWarning {
                agent_id,
                usage_pct,
                limit_usd,
            } => {
                format!(
                    "Budget warning: '{agent_id}' at {:.0}% of ${limit_usd:.2}",
                    usage_pct * 100.0
                )
            }
            Self::BudgetExhausted {
                agent_id,
                limit_usd,
            } => {
                format!("Budget exhausted: '{agent_id}' hit ${limit_usd:.2} limit")
            }
            Self::ApprovalRequested {
                agent_id,
                operation,
                ..
            } => {
                format!("Agent '{agent_id}' requests approval: {operation}")
            }
            Self::ApprovalGranted { request_id } => {
                format!("Approval granted: {request_id}")
            }
            Self::ApprovalDenied { request_id, reason } => {
                let r = reason.as_deref().unwrap_or("no reason");
                format!("Approval denied: {request_id} ({r})")
            }
            Self::HeartbeatFired { agent_id, task_id } => {
                format!("Heartbeat fired for '{agent_id}' on task '{task_id}'")
            }
            Self::HeartbeatScheduled {
                agent_id,
                interval_secs,
            } => {
                format!("Heartbeat scheduled for '{agent_id}' every {interval_secs}s")
            }
            Self::HeartbeatCancelled { agent_id } => {
                format!("Heartbeat cancelled for '{agent_id}'")
            }
        }
    }

    /// Extract agent_id if present.
    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::AgentStarted { agent_id, .. }
            | Self::AgentCompleted { agent_id, .. }
            | Self::AgentFailed { agent_id, .. }
            | Self::AgentPaused { agent_id, .. }
            | Self::TaskClaimed { agent_id, .. }
            | Self::TaskCompleted { agent_id, .. }
            | Self::ToolCalled { agent_id, .. }
            | Self::FileModified { agent_id, .. }
            | Self::ShellExecuted { agent_id, .. }
            | Self::CostIncurred { agent_id, .. }
            | Self::BudgetWarning { agent_id, .. }
            | Self::BudgetExhausted { agent_id, .. }
            | Self::ApprovalRequested { agent_id, .. }
            | Self::HeartbeatFired { agent_id, .. }
            | Self::HeartbeatScheduled { agent_id, .. }
            | Self::HeartbeatCancelled { agent_id, .. } => Some(agent_id),
            Self::TaskProgress { .. }
            | Self::TaskFailed { .. }
            | Self::ApprovalGranted { .. }
            | Self::ApprovalDenied { .. } => None,
        }
    }

    /// Extract cost_usd if this is a cost-bearing event.
    pub fn cost_usd(&self) -> f64 {
        match self {
            Self::CostIncurred { cost_usd, .. } => *cost_usd,
            Self::AgentCompleted { cost, .. } => *cost,
            Self::TaskCompleted { cost, .. } => *cost,
            _ => 0.0,
        }
    }
}
