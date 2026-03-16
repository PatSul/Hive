use serde::{Deserialize, Serialize};

/// Result of evaluating a single question against a skill output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub question_id: String,
    pub passed: bool,
    pub reasoning: String,
}

/// Aggregated result of running all eval questions across N samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunResult {
    /// Weighted pass rate from 0.0 to 1.0.
    pub pass_rate: f64,
    /// Individual question results (from the last sample).
    pub results: Vec<EvalResult>,
    /// The actual skill outputs that were evaluated.
    pub sample_outputs: Vec<String>,
}

/// Result of a single iteration in the autoresearch loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationResult {
    pub iteration: u32,
    pub prompt_text: String,
    pub eval_result: EvalRunResult,
    pub is_new_best: bool,
    pub improvement_over_baseline: f64,
}

/// Complete report from an autoresearch run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoResearchReport {
    pub skill_name: String,
    pub iterations_run: u32,
    pub baseline_pass_rate: f64,
    pub final_pass_rate: f64,
    pub best_prompt_version: u32,
    pub improvement: f64,
    pub stopped_reason: AutoResearchStopReason,
    pub iteration_history: Vec<IterationResult>,
    pub total_cost: f64,
    pub duration_ms: u64,
}

/// Why the autoresearch loop stopped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoResearchStopReason {
    MaxIterationsReached,
    PerfectScore,
    NoImprovementPlateau { consecutive_failures: u32 },
    BudgetExhausted { spent: f64, budget: f64 },
    UserCancelled,
    EmptyEvalSuite,
    NoBaselinePrompt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_result_serde_roundtrip() {
        let result = EvalResult {
            question_id: "valid_rust".into(),
            passed: true,
            reasoning: "Output contains valid Rust syntax".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: EvalResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.question_id, "valid_rust");
        assert!(parsed.passed);
    }

    #[test]
    fn test_eval_run_result_pass_rate() {
        let run = EvalRunResult {
            pass_rate: 0.75,
            results: vec![
                EvalResult { question_id: "a".into(), passed: true, reasoning: "ok".into() },
                EvalResult { question_id: "b".into(), passed: true, reasoning: "ok".into() },
                EvalResult { question_id: "c".into(), passed: true, reasoning: "ok".into() },
                EvalResult { question_id: "d".into(), passed: false, reasoning: "bad".into() },
            ],
            sample_outputs: vec!["output1".into()],
        };
        assert!((run.pass_rate - 0.75).abs() < f64::EPSILON);
        assert_eq!(run.results.len(), 4);
    }

    #[test]
    fn test_iteration_result_serde() {
        let iter = IterationResult {
            iteration: 1,
            prompt_text: "You are a coder.".into(),
            eval_result: EvalRunResult {
                pass_rate: 0.8,
                results: vec![],
                sample_outputs: vec![],
            },
            is_new_best: true,
            improvement_over_baseline: 0.15,
        };
        let json = serde_json::to_string(&iter).unwrap();
        let parsed: IterationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.iteration, 1);
        assert!(parsed.is_new_best);
    }

    #[test]
    fn test_stop_reason_serde_variants() {
        let reasons = vec![
            AutoResearchStopReason::MaxIterationsReached,
            AutoResearchStopReason::PerfectScore,
            AutoResearchStopReason::NoImprovementPlateau { consecutive_failures: 3 },
            AutoResearchStopReason::BudgetExhausted { spent: 0.50, budget: 0.45 },
            AutoResearchStopReason::UserCancelled,
            AutoResearchStopReason::EmptyEvalSuite,
            AutoResearchStopReason::NoBaselinePrompt,
        ];
        for reason in &reasons {
            let json = serde_json::to_string(reason).unwrap();
            let parsed: AutoResearchStopReason = serde_json::from_str(&json).unwrap();
            let _ = serde_json::to_string(&parsed).unwrap();
        }
    }

    #[test]
    fn test_report_serde_roundtrip() {
        let report = AutoResearchReport {
            skill_name: "test-skill".into(),
            iterations_run: 5,
            baseline_pass_rate: 0.4,
            final_pass_rate: 0.85,
            best_prompt_version: 3,
            improvement: 0.45,
            stopped_reason: AutoResearchStopReason::MaxIterationsReached,
            iteration_history: vec![],
            total_cost: 0.02,
            duration_ms: 5000,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: AutoResearchReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.skill_name, "test-skill");
        assert_eq!(parsed.iterations_run, 5);
        assert!((parsed.improvement - 0.45).abs() < f64::EPSILON);
    }
}
