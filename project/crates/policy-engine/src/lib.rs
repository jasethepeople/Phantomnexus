//! # Policy Engine
//!
//! The `policy-engine` crate provides rule-based and ML-driven policy matching
//! for the YouTube Sentinel video analysis pipeline.
//!
//! ## Key Components
//!
//! - **PolicyEngine** — Main engine that evaluates analysis results against rules.
//! - **RuleSet** — Collection of policy rules with condition evaluation.
//! - **MlRiskScorer** — Machine-learning risk scorer with Platt scaling calibration.
//! - **CountryPolicyRegistry** — 203 country-specific policies across 10 regions.
//! - **Guidelines** — YouTube Help Center references and remediation advice.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use policy_engine::{PolicyEngine, PolicyConfig, StrictnessLevel};
//! use sentinel_core::proto::AnalysisResult;
//!
//! let config = PolicyConfig::new(StrictnessLevel::Balanced)
//!     .with_country("US");
//! let engine = PolicyEngine::new(config).unwrap();
//! // let violations = engine.evaluate(&analysis_result).unwrap();
//! ```

pub mod country_policies;
pub mod engine;
pub mod guidelines;
pub mod ml_scorer;
pub mod rules;

// Re-export the main types for convenience.
pub use engine::{PolicyConfig, PolicyEngine, StrictnessLevel};
pub use ml_scorer::RiskScores;
pub use rules::{PolicyRule, RuleAction, RuleCondition, RuleMatch, RuleSet};

// Re-export Violation from sentinel-core for API consistency.
pub use sentinel_core::proto::Violation;

use thiserror::Error;

// ---------------------------------------------------------------------------
// PolicyEngineError
// ---------------------------------------------------------------------------

/// Errors that can occur in the policy engine.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PolicyEngineError {
    /// Failed to parse JSON configuration or rules.
    #[error("JSON parse error: {0}")]
    JsonParse(String),

    /// A referenced rule was not found.
    #[error("Rule not found: {0}")]
    RuleNotFound(String),

    /// An invalid configuration value was provided.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Country code is not recognized.
    #[error("Unknown country code: {0}")]
    UnknownCountry(String),

    /// Evaluation was aborted due to internal error.
    #[error("Evaluation failed: {0}")]
    EvaluationFailed(String),
}

impl From<serde_json::Error> for PolicyEngineError {
    fn from(err: serde_json::Error) -> Self {
        PolicyEngineError::JsonParse(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_json_parse() {
        let e = PolicyEngineError::JsonParse("bad json".into());
        assert!(format!("{e}").contains("JSON parse error"));
        assert!(format!("{e}").contains("bad json"));
    }

    #[test]
    fn error_display_rule_not_found() {
        let e = PolicyEngineError::RuleNotFound("rule-123".into());
        assert!(format!("{e}").contains("Rule not found"));
        assert!(format!("{e}").contains("rule-123"));
    }

    #[test]
    fn error_display_invalid_config() {
        let e = PolicyEngineError::InvalidConfig("bad threshold".into());
        assert!(format!("{e}").contains("Invalid configuration"));
    }

    #[test]
    fn error_display_unknown_country() {
        let e = PolicyEngineError::UnknownCountry("XX".into());
        assert!(format!("{e}").contains("Unknown country code"));
    }

    #[test]
    fn error_display_evaluation_failed() {
        let e = PolicyEngineError::EvaluationFailed("timeout".into());
        assert!(format!("{e}").contains("Evaluation failed"));
    }

    #[test]
    fn error_from_serde_json() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let engine_err: PolicyEngineError = serde_err.into();
        assert!(matches!(engine_err, PolicyEngineError::JsonParse(_)));
    }

    #[test]
    fn reexports_available() {
        // These should compile if re-exports are correct
        let _: PolicyConfig = PolicyConfig::default();
        let _ = StrictnessLevel::Balanced;
    }

    #[test]
    fn strictness_level_equality() {
        assert_eq!(StrictnessLevel::Balanced, StrictnessLevel::Balanced);
        assert_ne!(StrictnessLevel::Permissive, StrictnessLevel::Strict);
    }

    #[test]
    fn policy_config_default_values() {
        let config = PolicyConfig::default();
        assert_eq!(config.strictness, StrictnessLevel::Balanced);
        assert!(config.enable_ml_scoring);
        assert_eq!(config.max_violations, 50);
        assert!(config.include_remediation);
    }

    #[test]
    fn policy_config_builder_chain() {
        let config = PolicyConfig::new(StrictnessLevel::Enterprise)
            .with_country("GB")
            .with_ml_scoring(true)
            .with_auto_escalate(true)
            .with_max_violations(100)
            .with_country_overrides(true);

        assert_eq!(config.strictness, StrictnessLevel::Enterprise);
        assert_eq!(config.country_code, Some("GB".into()));
        assert!(config.enable_ml_scoring);
        assert!(config.auto_escalate);
        assert_eq!(config.max_violations, 100);
        assert!(config.enable_country_overrides);
    }

    #[test]
    fn violation_reexport() {
        let v = Violation::new("test violation");
        assert_eq!(v.description, "test violation");
    }
}
