use std::sync::Arc;

use sentinel_core::proto::{AnalysisResult, Severity, Violation, ViolationCategory};

use crate::country_policies::{CountryPolicy, CountryPolicyRegistry};
use crate::guidelines;
use crate::ml_scorer::{MlRiskScorer, RiskScores};
use crate::rules::{PolicyRule, RuleMatch, RuleSet};

// ---------------------------------------------------------------------------
// StrictnessLevel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum StrictnessLevel {
    /// Low enforcement: only flag severe violations.
    Permissive,
    /// Standard enforcement: flag moderate and severe violations.
    Balanced,
    /// High enforcement: flag all violations above low severity.
    Strict,
    /// Maximum enforcement: block on any match, escalate everything.
    Enterprise,
}

impl StrictnessLevel {
    /// Minimum severity to trigger a violation at this strictness.
    pub fn min_severity(&self) -> Severity {
        match self {
            StrictnessLevel::Permissive => Severity::High,
            StrictnessLevel::Balanced => Severity::Medium,
            StrictnessLevel::Strict => Severity::Low,
            StrictnessLevel::Enterprise => Severity::None,
        }
    }

    /// Label for the strictness level.
    pub fn label(&self) -> &'static str {
        match self {
            StrictnessLevel::Permissive => "permissive",
            StrictnessLevel::Balanced => "balanced",
            StrictnessLevel::Strict => "strict",
            StrictnessLevel::Enterprise => "enterprise",
        }
    }

    /// Confidence threshold below which matches are discarded.
    pub fn confidence_threshold(&self) -> f64 {
        match self {
            StrictnessLevel::Permissive => 0.7,
            StrictnessLevel::Balanced => 0.5,
            StrictnessLevel::Strict => 0.3,
            StrictnessLevel::Enterprise => 0.1,
        }
    }

    /// Risk score threshold for escalation.
    pub fn escalation_threshold(&self) -> f64 {
        match self {
            StrictnessLevel::Permissive => 0.8,
            StrictnessLevel::Balanced => 0.6,
            StrictnessLevel::Strict => 0.4,
            StrictnessLevel::Enterprise => 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// PolicyConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyConfig {
    /// Strictness level of the policy engine.
    pub strictness: StrictnessLevel,
    /// ISO-3166 country code for region-specific policies.
    pub country_code: Option<String>,
    /// Whether to enable ML-based risk scoring.
    pub enable_ml_scoring: bool,
    /// Whether to apply country-specific severity overrides.
    pub enable_country_overrides: bool,
    /// Whether to auto-escalate high-risk content.
    pub auto_escalate: bool,
    /// Maximum number of violations to report.
    pub max_violations: usize,
    /// Whether to include remediation advice in violations.
    pub include_remediation: bool,
    /// Minimum confidence threshold (overrides strictness if set).
    pub custom_confidence_threshold: Option<f64>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            strictness: StrictnessLevel::Balanced,
            country_code: None,
            enable_ml_scoring: true,
            enable_country_overrides: true,
            auto_escalate: false,
            max_violations: 50,
            include_remediation: true,
            custom_confidence_threshold: None,
        }
    }
}

impl PolicyConfig {
    pub fn new(strictness: StrictnessLevel) -> Self {
        PolicyConfig {
            strictness,
            ..Default::default()
        }
    }

    pub fn with_country<S: Into<String>>(mut self, code: S) -> Self {
        self.country_code = Some(code.into());
        self
    }

    pub fn with_ml_scoring(mut self, enable: bool) -> Self {
        self.enable_ml_scoring = enable;
        self
    }

    pub fn with_auto_escalate(mut self, enable: bool) -> Self {
        self.auto_escalate = enable;
        self
    }

    pub fn with_max_violations(mut self, max: usize) -> Self {
        self.max_violations = max.max(1);
        self
    }

    pub fn with_country_overrides(mut self, enable: bool) -> Self {
        self.enable_country_overrides = enable;
        self
    }

    /// Get the effective confidence threshold.
    pub fn effective_confidence_threshold(&self) -> f64 {
        self.custom_confidence_threshold
            .unwrap_or_else(|| self.strictness.confidence_threshold())
    }
}

// ---------------------------------------------------------------------------
// PolicyEngine
// ---------------------------------------------------------------------------

pub struct PolicyEngine {
    config: PolicyConfig,
    rule_set: RuleSet,
    country_registry: CountryPolicyRegistry,
    active_country_policy: Option<CountryPolicy>,
    ml_scorer: MlRiskScorer,
    custom_rules: Vec<PolicyRule>,
}

impl PolicyEngine {
    /// Create a new PolicyEngine with the given configuration.
    /// Loads default rules automatically.
    pub fn new(config: PolicyConfig) -> Result<Self, crate::PolicyEngineError> {
        let mut rule_set = RuleSet::new();
        rule_set.load_defaults();

        let country_registry = CountryPolicyRegistry::new();
        let active_country_policy = config
            .country_code
            .as_ref()
            .and_then(|code| country_registry.get(code).cloned());

        let ml_scorer = MlRiskScorer::new();

        tracing::info!(
            "PolicyEngine initialized: strictness={}, rules_loaded={}",
            config.strictness.label(),
            rule_set.len()
        );

        Ok(PolicyEngine {
            config,
            rule_set,
            country_registry,
            active_country_policy,
            ml_scorer,
            custom_rules: Vec::new(),
        })
    }

    /// Evaluate an analysis result against all rules and return violations.
    pub fn evaluate(
        &self,
        analysis: &AnalysisResult,
    ) -> Result<Vec<Violation>, crate::PolicyEngineError> {
        let rule_matches = self.rule_set.evaluate(analysis);
        let confidence_threshold = self.config.effective_confidence_threshold();
        let min_severity = self.config.strictness.min_severity();
        let mut violations = Vec::new();

        for m in &rule_matches {
            // Filter by confidence threshold
            if m.confidence < confidence_threshold {
                continue;
            }

            // Filter by minimum severity
            if !severity_meets_minimum(&m.severity, &min_severity) {
                continue;
            }

            // Apply country-specific overrides if active
            let mut violation = m.to_violation();

            if self.config.enable_country_overrides {
                if let Some(ref country) = self.active_country_policy {
                    // Check if category is blocked
                    if country.is_category_blocked(&m.category) {
                        violation.severity = Severity::Critical;
                        violation.confidence = 1.0;
                    }
                    // Apply severity multiplier
                    let adjusted = country.apply_multiplier(&m.category, violation.confidence);
                    violation.confidence = adjusted;
                }
            }

            // Add guideline reference and remediation
            if self.config.include_remediation {
                let guideline_url = guidelines::help_center_url(&m.category);
                violation.guideline_reference = guideline_url;
            }

            violations.push(violation);
        }

        // Limit number of violations
        if violations.len() > self.config.max_violations {
            // Sort by severity then confidence, keep the most severe
            violations.sort_by(|a, b| {
                let sev_cmp = severity_rank(&b.severity).cmp(&severity_rank(&a.severity));
                if sev_cmp != std::cmp::Ordering::Equal {
                    return sev_cmp;
                }
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            violations.truncate(self.config.max_violations);
        }

        // Auto-escalate if configured
        if self.config.auto_escalate {
            for v in &mut violations {
                let risk_scores = if self.config.enable_ml_scoring {
                    Some(self.ml_scorer.score(analysis))
                } else {
                    None
                };
                if let Some(ref scores) = risk_scores {
                    if scores.overall_risk >= self.config.strictness.escalation_threshold() {
                        v.severity = escalate_severity(&v.severity);
                    }
                }
            }
        }

        tracing::info!(
            "Policy evaluation complete: {} violations from {} rule matches",
            violations.len(),
            rule_matches.len()
        );

        Ok(violations)
    }

    /// Compute calibrated risk probabilities using the ML scorer.
    pub fn score_risk(&self, analysis: &AnalysisResult) -> RiskScores {
        if !self.config.enable_ml_scoring {
            return RiskScores::default();
        }
        self.ml_scorer.score(analysis)
    }

    /// Load region-specific policy overrides for a country.
    pub fn load_country_policy(&mut self, country_code: &str) {
        if let Some(policy) = self.country_registry.get(country_code) {
            tracing::info!(
                "Loading country policy for {} ({})",
                policy.country_name,
                country_code
            );
            self.active_country_policy = Some(policy.clone());
            self.config.country_code = Some(country_code.to_uppercase());
        } else {
            tracing::warn!("No country policy found for code: {}", country_code);
            self.active_country_policy = None;
        }
    }

    /// Add a custom enterprise rule.
    pub fn add_custom_rule(&mut self, rule: PolicyRule) {
        tracing::info!(
            "Adding custom rule '{}' (id={})",
            rule.name,
            rule.id
        );
        self.custom_rules.push(rule.clone());
        self.rule_set.add_rule(rule);
    }

    /// Get the current configuration.
    pub fn config(&self) -> &PolicyConfig {
        &self.config
    }

    /// Get the number of loaded rules.
    pub fn rule_count(&self) -> usize {
        self.rule_set.len()
    }

    /// Get the active country policy name if any.
    pub fn active_country_name(&self) -> Option<String> {
        self.active_country_policy
            .as_ref()
            .map(|p| p.country_name.clone())
    }

    /// Check if a specific country policy is loaded.
    pub fn has_country_policy(&self, country_code: &str) -> bool {
        self.country_registry.has_policy(country_code)
    }

    /// Get a reference to the underlying rule set (for inspection).
    pub fn rule_set(&self) -> &RuleSet {
        &self.rule_set
    }

    /// Enable or disable a rule by ID.
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) -> bool {
        self.rule_set.set_rule_enabled(rule_id, enabled)
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&mut self, rule_id: &str) -> Option<PolicyRule> {
        self.rule_set.remove_rule(rule_id)
    }

    /// Load additional rules from a JSON string.
    pub fn load_rules_from_json(
        &mut self,
        json: &str,
    ) -> Result<(), crate::PolicyEngineError> {
        self.rule_set.load_from_json(json)
    }

    /// Serialize current rules to JSON.
    pub fn rules_to_json(&self) -> Result<String, crate::PolicyEngineError> {
        self.rule_set.to_json()
    }

    /// Get current ML scorer configuration.
    pub fn ml_scorer(&self) -> &MlRiskScorer {
        &self.ml_scorer
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn severity_meets_minimum(severity: &Severity, minimum: &Severity) -> bool {
    severity_rank(severity) >= severity_rank(minimum)
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::None => 0,
        Severity::Low => 1,
        Severity::Medium => 2,
        Severity::High => 3,
        Severity::Critical => 4,
    }
}

fn escalate_severity(severity: &Severity) -> Severity {
    match severity {
        Severity::None => Severity::Low,
        Severity::Low => Severity::Medium,
        Severity::Medium => Severity::High,
        Severity::High => Severity::Critical,
        Severity::Critical => Severity::Critical,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::proto::{
        AudioAnalysis, BoundingBox, ContextType, DetectedObject, FlashingSegment, MetadataAnalysis,
        Timestamp, VideoSource, VisualAnalysis,
    };

    fn make_test_analysis() -> AnalysisResult {
        AnalysisResult {
            job_id: uuid::Uuid::new_v4(),
            source: VideoSource::new("test", "/tmp/test.mp4"),
            visual: Some(VisualAnalysis {
                objects: vec![
                    DetectedObject::new("weapon", 0.92)
                        .with_category(ViolationCategory::Violence)
                        .with_context(ContextType::Gratuitous)
                        .at_timestamp(Timestamp::new(10.0, 300))
                        .with_bbox(BoundingBox::new(0.2, 0.2, 0.8, 0.8)),
                ],
                texts: vec![
                    sentinel_core::proto::DetectedText::new("free money scam", 0.88)
                        .at_timestamp(Timestamp::new(5.0, 150)),
                ],
                transitions: vec![],
                flashing: vec![FlashingSegment {
                    start: Timestamp::new(20.0, 600),
                    end: Timestamp::new(22.0, 660),
                    frequency_hz: 12.0,
                    severity: Severity::High,
                }],
                overall_risk_score: 55.0,
            }),
            audio: Some(AudioAnalysis {
                transcript: vec![
                    sentinel_core::proto::TranscriptSegment {
                        start: Timestamp::new(0.0, 0),
                        end: Timestamp::new(5.0, 150),
                        text: "This is hate speech content".into(),
                        confidence: 0.9,
                        sentiment: -0.7,
                        is_sarcasm: false,
                        is_background_speech: false,
                    },
                ],
                events: vec![
                    sentinel_core::proto::AudioEvent::new("gunshot", 0.88, Timestamp::new(10.5, 315)),
                ],
                copyright_matches: vec![
                    sentinel_core::proto::CopyrightMatch::new("Song A", "Label B")
                        .with_confidence(0.92)
                        .with_match_type("audio_fingerprint")
                        .with_span(Timestamp::new(30.0, 900), Timestamp::new(35.0, 1050)),
                ],
                overall_risk_score: 48.0,
            }),
            metadata: Some(MetadataAnalysis {
                clickbait_score: 0.85,
                keyword_stuffing_score: 0.3,
                misleading_score: 0.7,
                flagged_keywords: vec!["scam".into(), "clickbait".into()],
                suggestions: vec![],
                overall_risk_score: 62.0,
            }),
            final_score: 56.0,
            completed_at: None,
        }
    }

    #[test]
    fn policy_engine_new() {
        let config = PolicyConfig::default();
        let engine = PolicyEngine::new(config);
        assert!(engine.is_ok());
        let engine = engine.unwrap();
        assert!(engine.rule_count() > 0);
    }

    #[test]
    fn policy_engine_evaluate() {
        let config = PolicyConfig::default();
        let engine = PolicyEngine::new(config).unwrap();
        let analysis = make_test_analysis();
        let violations = engine.evaluate(&analysis).unwrap();
        // Should find violations from the default rules matching our test data
        assert!(!violations.is_empty(), "Expected violations from test data");
    }

    #[test]
    fn policy_engine_evaluate_permissive() {
        let config = PolicyConfig::new(StrictnessLevel::Permissive);
        let engine = PolicyEngine::new(config).unwrap();
        let analysis = make_test_analysis();
        let violations = engine.evaluate(&analysis).unwrap();
        // Permissive should find fewer violations
        assert_eq!(violations.iter().all(|v| {
            matches!(v.severity, Severity::High | Severity::Critical)
        }), true);
    }

    #[test]
    fn policy_engine_evaluate_strict() {
        let config = PolicyConfig::new(StrictnessLevel::Strict);
        let engine = PolicyEngine::new(config).unwrap();
        let analysis = make_test_analysis();
        let violations = engine.evaluate(&analysis).unwrap();
        // Strict should find more violations than permissive
        assert!(!violations.is_empty());
    }

    #[test]
    fn policy_engine_score_risk() {
        let config = PolicyConfig::default().with_ml_scoring(true);
        let engine = PolicyEngine::new(config).unwrap();
        let analysis = make_test_analysis();
        let scores = engine.score_risk(&analysis);
        assert!(scores.overall_risk >= 0.0 && scores.overall_risk <= 1.0);
    }

    #[test]
    fn policy_engine_score_risk_disabled() {
        let config = PolicyConfig::default().with_ml_scoring(false);
        let engine = PolicyEngine::new(config).unwrap();
        let analysis = make_test_analysis();
        let scores = engine.score_risk(&analysis);
        assert_eq!(scores.overall_risk, 0.0);
    }

    #[test]
    fn policy_engine_load_country_policy() {
        let config = PolicyConfig::default();
        let mut engine = PolicyEngine::new(config).unwrap();
        engine.load_country_policy("DE");
        assert_eq!(engine.active_country_name(), Some("Germany".into()));
    }

    #[test]
    fn policy_engine_load_country_policy_nonexistent() {
        let config = PolicyConfig::default();
        let mut engine = PolicyEngine::new(config).unwrap();
        engine.load_country_policy("XX");
        assert!(engine.active_country_name().is_none());
    }

    #[test]
    fn policy_engine_add_custom_rule() {
        let config = PolicyConfig::default();
        let mut engine = PolicyEngine::new(config).unwrap();
        let rule_count_before = engine.rule_count();
        engine.add_custom_rule(PolicyRule {
            id: "custom-001".into(),
            name: "Custom Test".into(),
            category: ViolationCategory::Violence,
            severity: Severity::High,
            conditions: vec![],
            action: crate::rules::RuleAction::Flag,
            regions: vec![],
            enabled: true,
            description: "A custom test rule".into(),
        });
        assert_eq!(engine.rule_count(), rule_count_before + 1);
    }

    #[test]
    fn policy_engine_set_rule_enabled() {
        let config = PolicyConfig::default();
        let mut engine = PolicyEngine::new(config).unwrap();
        engine.add_custom_rule(PolicyRule {
            id: "toggle-rule".into(),
            name: "Toggle".into(),
            category: ViolationCategory::SpamDeceptive,
            severity: Severity::Medium,
            conditions: vec![],
            action: crate::rules::RuleAction::LogOnly,
            regions: vec![],
            enabled: true,
            description: "".into(),
        });
        assert!(engine.set_rule_enabled("toggle-rule", false));
    }

    #[test]
    fn policy_engine_remove_rule() {
        let config = PolicyConfig::default();
        let mut engine = PolicyEngine::new(config).unwrap();
        engine.add_custom_rule(PolicyRule {
            id: "removable".into(),
            name: "Removable".into(),
            category: ViolationCategory::SpamDeceptive,
            severity: Severity::Low,
            conditions: vec![],
            action: crate::rules::RuleAction::LogOnly,
            regions: vec![],
            enabled: true,
            description: "".into(),
        });
        let count_before = engine.rule_count();
        let removed = engine.remove_rule("removable");
        assert!(removed.is_some());
        assert_eq!(engine.rule_count(), count_before - 1);
    }

    #[test]
    fn strictness_level_confidence_threshold() {
        assert_eq!(StrictnessLevel::Permissive.confidence_threshold(), 0.7);
        assert_eq!(StrictnessLevel::Balanced.confidence_threshold(), 0.5);
        assert_eq!(StrictnessLevel::Strict.confidence_threshold(), 0.3);
        assert_eq!(StrictnessLevel::Enterprise.confidence_threshold(), 0.1);
    }

    #[test]
    fn strictness_level_min_severity() {
        assert_eq!(StrictnessLevel::Permissive.min_severity(), Severity::High);
        assert_eq!(StrictnessLevel::Balanced.min_severity(), Severity::Medium);
        assert_eq!(StrictnessLevel::Strict.min_severity(), Severity::Low);
        assert_eq!(StrictnessLevel::Enterprise.min_severity(), Severity::None);
    }

    #[test]
    fn strictness_level_labels() {
        assert_eq!(StrictnessLevel::Permissive.label(), "permissive");
        assert_eq!(StrictnessLevel::Balanced.label(), "balanced");
        assert_eq!(StrictnessLevel::Strict.label(), "strict");
        assert_eq!(StrictnessLevel::Enterprise.label(), "enterprise");
    }

    #[test]
    fn severity_rank_ordering() {
        assert!(severity_rank(&Severity::Critical) > severity_rank(&Severity::High));
        assert!(severity_rank(&Severity::High) > severity_rank(&Severity::Medium));
        assert!(severity_rank(&Severity::Medium) > severity_rank(&Severity::Low));
        assert!(severity_rank(&Severity::Low) > severity_rank(&Severity::None));
    }

    #[test]
    fn severity_meets_minimum_test() {
        assert!(severity_meets_minimum(&Severity::High, &Severity::Medium));
        assert!(severity_meets_minimum(&Severity::Medium, &Severity::Medium));
        assert!(!severity_meets_minimum(&Severity::Low, &Severity::Medium));
    }

    #[test]
    fn escalate_severity_test() {
        assert_eq!(escalate_severity(&Severity::None), Severity::Low);
        assert_eq!(escalate_severity(&Severity::Low), Severity::Medium);
        assert_eq!(escalate_severity(&Severity::Medium), Severity::High);
        assert_eq!(escalate_severity(&Severity::High), Severity::Critical);
        assert_eq!(escalate_severity(&Severity::Critical), Severity::Critical);
    }

    #[test]
    fn policy_config_builder() {
        let config = PolicyConfig::new(StrictnessLevel::Strict)
            .with_country("DE")
            .with_ml_scoring(false)
            .with_auto_escalate(true)
            .with_max_violations(10);
        assert_eq!(config.strictness, StrictnessLevel::Strict);
        assert_eq!(config.country_code, Some("DE".into()));
        assert!(!config.enable_ml_scoring);
        assert!(config.auto_escalate);
        assert_eq!(config.max_violations, 10);
    }

    #[test]
    fn policy_config_effective_threshold() {
        let config = PolicyConfig::default();
        assert!((config.effective_confidence_threshold() - 0.5).abs() < f64::EPSILON);

        let config_custom = PolicyConfig {
            custom_confidence_threshold: Some(0.25),
            ..Default::default()
        };
        assert!(
            (config_custom.effective_confidence_threshold() - 0.25).abs() < f64::EPSILON
        );
    }

    #[test]
    fn policy_config_default() {
        let config = PolicyConfig::default();
        assert_eq!(config.strictness, StrictnessLevel::Balanced);
        assert!(config.enable_ml_scoring);
        assert!(config.enable_country_overrides);
        assert!(!config.auto_escalate);
        assert_eq!(config.max_violations, 50);
        assert!(config.include_remediation);
    }

    #[test]
    fn policy_engine_has_country_policy() {
        let engine = PolicyEngine::new(PolicyConfig::default()).unwrap();
        assert!(engine.has_country_policy("US"));
        assert!(engine.has_country_policy("us"));
        assert!(!engine.has_country_policy("ZZ"));
    }

    #[test]
    fn policy_engine_max_violations_respected() {
        let config = PolicyConfig::default().with_max_violations(5);
        let engine = PolicyEngine::new(config).unwrap();
        let analysis = make_test_analysis();
        let violations = engine.evaluate(&analysis).unwrap();
        assert!(violations.len() <= 5);
    }

    #[test]
    fn policy_engine_load_rules_from_json() {
        let mut engine = PolicyEngine::new(PolicyConfig::default()).unwrap();
        let json = r#"[
            {
                "id": "test-json-rule",
                "name": "JSON Rule",
                "category": "spam_deceptive",
                "severity": "Medium",
                "conditions": [],
                "action": "log_only",
                "enabled": true
            }
        ]"#;
        let count_before = engine.rule_count();
        let result = engine.load_rules_from_json(json);
        assert!(result.is_ok());
        assert_eq!(engine.rule_count(), count_before + 1);
    }

    #[test]
    fn policy_engine_rules_to_json() {
        let engine = PolicyEngine::new(PolicyConfig::default()).unwrap();
        let json_result = engine.rules_to_json();
        assert!(json_result.is_ok());
        let json = json_result.unwrap();
        assert!(json.contains("[") || !json.is_empty());
    }

    #[test]
    fn policy_engine_with_country_overrides() {
        let config = PolicyConfig::default()
            .with_country("DE")
            .with_country_overrides(true);
        let mut engine = PolicyEngine::new(config).unwrap();
        // Germany has strict hate speech rules
        engine.load_country_policy("DE");
        assert!(engine.active_country_policy.is_some());
    }
}
