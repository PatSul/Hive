use serde::{Deserialize, Serialize};

use super::OperationType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub name: String,
    pub enabled: bool,
    pub trigger: RuleTrigger,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleTrigger {
    SecurityGatewayBlock,
    CostExceeds { usd: f64 },
    PathMatches { glob: String },
    FilesExceed { count: usize },
    CommandMatches { pattern: String },
    Always,
}

impl ApprovalRule {
    pub fn matches(&self, op: &OperationType) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.trigger {
            RuleTrigger::SecurityGatewayBlock => false,
            RuleTrigger::CostExceeds { usd } => {
                if let OperationType::AiCall { estimated_cost, .. } = op {
                    *estimated_cost > *usd
                } else {
                    false
                }
            }
            RuleTrigger::PathMatches { glob: pattern } => {
                let path = match op {
                    OperationType::FileModify { path, .. } => Some(path.as_str()),
                    OperationType::FileDelete(path) => Some(path.as_str()),
                    _ => None,
                };
                if let Some(path) = path {
                    glob_match(pattern, path)
                } else {
                    false
                }
            }
            RuleTrigger::FilesExceed { count } => {
                if let OperationType::FileModify { scope, .. } = op {
                    scope.split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<usize>().ok())
                        .map(|n| n > *count)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            RuleTrigger::CommandMatches { pattern } => {
                if let OperationType::ShellCommand(cmd) = op {
                    glob_match(pattern, cmd)
                } else {
                    false
                }
            }
            RuleTrigger::Always => true,
        }
    }

    pub fn defaults() -> Vec<ApprovalRule> {
        vec![
            ApprovalRule {
                name: "security-gateway".into(),
                enabled: true,
                trigger: RuleTrigger::SecurityGatewayBlock,
                priority: 100,
            },
            ApprovalRule {
                name: "expensive-operations".into(),
                enabled: true,
                trigger: RuleTrigger::CostExceeds { usd: 5.0 },
                priority: 90,
            },
            ApprovalRule {
                name: "git-push".into(),
                enabled: true,
                trigger: RuleTrigger::CommandMatches { pattern: "git push*".into() },
                priority: 80,
            },
            ApprovalRule {
                name: "bulk-modify".into(),
                enabled: true,
                trigger: RuleTrigger::FilesExceed { count: 10 },
                priority: 70,
            },
        ]
    }
}

/// Simple glob matching: `*` matches any sequence of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 {
                    return false;
                }
                pos += idx + part.len();
            }
            None => return false,
        }
    }
    if !pattern.ends_with('*') && pos != text.len() {
        return false;
    }
    true
}
