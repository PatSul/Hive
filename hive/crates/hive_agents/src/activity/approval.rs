use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::OperationType;
use super::rules::ApprovalRule;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub agent_id: String,
    pub timestamp: DateTime<Utc>,
    pub operation: OperationType,
    pub context: String,
    pub matched_rule: String,
    pub estimated_cost: Option<f64>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalDecision {
    Approved,
    Denied { reason: Option<String> },
    Timeout,
}

pub struct ApprovalGate {
    rules: Vec<ApprovalRule>,
    pending: Mutex<HashMap<String, ApprovalRequest>>,
    response_channels: Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

impl std::fmt::Debug for ApprovalGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pending_count = self.pending.lock().map(|p| p.len()).unwrap_or(0);
        f.debug_struct("ApprovalGate")
            .field("rules", &self.rules.len())
            .field("pending", &pending_count)
            .finish()
    }
}

impl ApprovalGate {
    pub fn new(mut rules: Vec<ApprovalRule>) -> Self {
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        Self {
            rules,
            pending: Mutex::new(HashMap::new()),
            response_channels: Mutex::new(HashMap::new()),
        }
    }

    pub fn check_with_channel(
        &self,
        agent_id: &str,
        operation: &OperationType,
    ) -> Option<(ApprovalRequest, oneshot::Receiver<ApprovalDecision>)> {
        let matched_rule = self.rules.iter().find(|r| r.matches(operation))?;

        let request = ApprovalRequest {
            id: Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            timestamp: Utc::now(),
            operation: operation.clone(),
            context: format!("{operation:?}"),
            matched_rule: matched_rule.name.clone(),
            estimated_cost: match operation {
                OperationType::AiCall { estimated_cost, .. } => Some(*estimated_cost),
                _ => None,
            },
            timeout_secs: Some(300),
        };

        let (tx, rx) = oneshot::channel();

        self.pending
            .lock()
            .unwrap()
            .insert(request.id.clone(), request.clone());
        self.response_channels
            .lock()
            .unwrap()
            .insert(request.id.clone(), tx);

        Some((request, rx))
    }

    pub fn check_sync(&self, agent_id: &str, operation: &OperationType) -> Option<ApprovalRequest> {
        self.check_with_channel(agent_id, operation)
            .map(|(req, _rx)| req)
    }

    pub fn respond(&self, request_id: &str, decision: ApprovalDecision) {
        self.pending.lock().unwrap().remove(request_id);
        if let Some(tx) = self.response_channels.lock().unwrap().remove(request_id) {
            let _ = tx.send(decision);
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    pub fn pending_requests(&self) -> Vec<ApprovalRequest> {
        self.pending.lock().unwrap().values().cloned().collect()
    }

    pub fn rules(&self) -> &[ApprovalRule] {
        &self.rules
    }
}
