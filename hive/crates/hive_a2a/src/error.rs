//! A2A error types.

/// Errors that can occur in the A2A protocol layer.
#[derive(Debug, thiserror::Error)]
pub enum A2aError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Unsupported skill: {0}")]
    UnsupportedSkill(String),

    #[error("Budget exceeded: limit ${limit:.2}, spent ${spent:.2}")]
    BudgetExceeded { limit: f64, spent: f64 },

    #[error("Task timed out after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("AI provider error: {0}")]
    Provider(String),

    #[error("Type bridge error: {0}")]
    Bridge(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Security validation failed: {0}")]
    Security(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = A2aError::Config("bad config".into());
        assert!(err.to_string().contains("bad config"));
        assert!(err.to_string().contains("Configuration error"));
    }

    #[test]
    fn test_all_variants_constructible() {
        let variants: Vec<A2aError> = vec![
            A2aError::Config("cfg".into()),
            A2aError::Auth("auth".into()),
            A2aError::TaskNotFound("task-123".into()),
            A2aError::UnsupportedSkill("unknown".into()),
            A2aError::BudgetExceeded {
                limit: 1.0,
                spent: 2.5,
            },
            A2aError::Timeout { seconds: 300 },
            A2aError::Provider("openai down".into()),
            A2aError::Bridge("type mismatch".into()),
            A2aError::Network("connection refused".into()),
            A2aError::RateLimited,
            A2aError::Security("blocked path".into()),
        ];
        // All 11 variants must be constructible
        assert_eq!(variants.len(), 11);
    }

    #[test]
    fn test_budget_exceeded_formatting() {
        let err = A2aError::BudgetExceeded {
            limit: 1.0,
            spent: 2.50,
        };
        assert_eq!(err.to_string(), "Budget exceeded: limit $1.00, spent $2.50");
    }

    #[test]
    fn test_timeout_formatting() {
        let err = A2aError::Timeout { seconds: 300 };
        assert_eq!(err.to_string(), "Task timed out after 300s");
    }

    #[test]
    fn test_rate_limited_display() {
        let err = A2aError::RateLimited;
        assert_eq!(err.to_string(), "Rate limited");
    }
}
