//! SSE streaming helpers for A2A status updates.
//!
//! Converts Hive internal [`TaskEvent`]s from the Coordinator into
//! SSE-ready JSON strings suitable for Server-Sent Events responses.

use hive_agents::coordinator::TaskEvent;

use crate::bridge;

/// Convert a Coordinator [`TaskEvent`] into an SSE-ready JSON string.
///
/// Returns `None` if the event cannot be mapped to an A2A status update
/// (currently all variants are mappable, but this keeps the API
/// future-proof).
///
/// The returned string is a JSON-serialised [`a2a_rs::TaskStatusUpdateEvent`]
/// ready to be written as an SSE `data:` line.
pub fn coordinator_event_to_sse(
    a2a_task_id: &str,
    context_id: &str,
    event: &TaskEvent,
) -> Option<String> {
    let update = bridge::task_event_to_status_update(a2a_task_id, context_id, event)?;
    serde_json::to_string(&update).ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_event_to_sse_data() {
        let event = TaskEvent::TaskStarted {
            task_id: "t1".into(),
            description: "test".into(),
            persona: "implement".into(),
        };
        let sse = coordinator_event_to_sse("a2a-1", "ctx-1", &event);
        assert!(sse.is_some());
        let json_str = sse.unwrap();
        assert!(json_str.contains("a2a-1"));
    }

    #[test]
    fn test_coordinator_event_to_sse_contains_context_id() {
        let event = TaskEvent::TaskProgress {
            task_id: "t2".into(),
            progress: 0.75,
            message: "Almost there".into(),
        };
        let sse = coordinator_event_to_sse("a2a-2", "ctx-42", &event);
        assert!(sse.is_some());
        let json_str = sse.unwrap();
        assert!(json_str.contains("ctx-42"));
        assert!(json_str.contains("a2a-2"));
    }

    #[test]
    fn test_coordinator_event_to_sse_all_complete() {
        let event = TaskEvent::AllComplete {
            total_cost: 0.10,
            total_duration_ms: 5000,
            success_count: 3,
            failure_count: 0,
        };
        let sse = coordinator_event_to_sse("a2a-3", "ctx-3", &event);
        assert!(sse.is_some());
        let json_str = sse.unwrap();

        // Should contain "final" field set to true
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["final"], true);
        assert_eq!(parsed["taskId"], "a2a-3");
    }

    #[test]
    fn test_coordinator_event_to_sse_plan_created() {
        let event = TaskEvent::PlanCreated {
            plan_id: "plan-1".into(),
            tasks: vec![],
        };
        let sse = coordinator_event_to_sse("a2a-4", "ctx-4", &event);
        assert!(sse.is_some());
        let json_str = sse.unwrap();
        assert!(json_str.contains("a2a-4"));
        assert!(json_str.contains("ctx-4"));
    }

    #[test]
    fn test_coordinator_event_to_sse_task_failed() {
        let event = TaskEvent::TaskFailed {
            task_id: "t-fail".into(),
            error: "Timeout".into(),
        };
        let sse = coordinator_event_to_sse("a2a-5", "ctx-5", &event);
        assert!(sse.is_some());
        let json_str = sse.unwrap();
        assert!(json_str.contains("a2a-5"));
    }

    #[test]
    fn test_coordinator_event_to_sse_task_completed() {
        let event = TaskEvent::TaskCompleted {
            task_id: "t-done".into(),
            duration_ms: 500,
            cost: 0.01,
            output_preview: "Done".into(),
        };
        let sse = coordinator_event_to_sse("a2a-6", "ctx-6", &event);
        assert!(sse.is_some());
        let json_str = sse.unwrap();

        // Non-final: individual task completion is not the overall final event
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["final"], false);
    }
}
