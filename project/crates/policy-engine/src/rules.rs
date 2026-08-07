use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use sentinel_core::proto::{
    AnalysisResult, AudioEvent, CopyrightMatch, DetectedObject, DetectedText, FlashingSegment,
    MetadataAnalysis, Severity, Timestamp, TranscriptSegment, ViolationCategory, VisualAnalysis,
};

// ---------------------------------------------------------------------------
// RuleAction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Flag the content for manual review.
    Flag,
    /// Automatically apply a fix.
    AutoFix { fix_type: String },
    /// Block the content from being published.
    Block,
    /// Escalate to a human moderator.
    Escalate,
    /// Log the match but take no action.
    LogOnly,
}

impl RuleAction {
    pub fn label(&self) -> String {
        match self {
            RuleAction::Flag => "flag".into(),
            RuleAction::AutoFix { fix_type } => format!("auto_fix:{fix_type}"),
            RuleAction::Block => "block".into(),
            RuleAction::Escalate => "escalate".into(),
            RuleAction::LogOnly => "log_only".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// RuleCondition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RuleCondition {
    /// Match a detected visual object.
    VisualObject {
        label: String,
        confidence_threshold: f64,
        #[serde(default)]
        context_excludes: Vec<String>,
    },
    /// Match OCR text via a regex pattern.
    VisualText {
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Match an acoustic audio event.
    AudioEvent {
        event_type: String,
        #[serde(default)]
        min_confidence: f64,
    },
    /// Match phrases in the transcript.
    TranscriptPhrase {
        phrases: Vec<String>,
        #[serde(default)]
        match_any: bool,
    },
    /// Match keywords in metadata (title, description, tags).
    MetadataKeyword {
        keywords: Vec<String>,
    },
    /// Match flashing segments that exceed a frequency threshold.
    Flashing {
        frequency_threshold: f64,
    },
    /// Match copyright matches above a confidence threshold.
    CopyrightMatch {
        confidence_threshold: f64,
    },
    /// Match volume spikes above a dB threshold.
    VolumeSpike {
        db_threshold: f64,
    },
    /// Composite: combine multiple conditions with AND/OR.
    Composite {
        conditions: Vec<RuleCondition>,
        operator: CompositeOperator,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CompositeOperator {
    And,
    Or,
}

impl RuleCondition {
    /// Evaluate this single condition against an analysis result.
    pub fn evaluate(&self, analysis: &AnalysisResult) -> Option<RuleEvidence> {
        match self {
            RuleCondition::VisualObject {
                label,
                confidence_threshold,
                context_excludes,
            } => Self::match_visual_object(
                analysis,
                label,
                *confidence_threshold,
                context_excludes,
            ),
            RuleCondition::VisualText {
                pattern,
                case_sensitive,
            } => Self::match_visual_text(analysis, pattern, *case_sensitive),
            RuleCondition::AudioEvent {
                event_type,
                min_confidence,
            } => Self::match_audio_event(analysis, event_type, *min_confidence),
            RuleCondition::TranscriptPhrase { phrases, match_any } => {
                Self::match_transcript_phrase(analysis, phrases, *match_any)
            }
            RuleCondition::MetadataKeyword { keywords } => {
                Self::match_metadata_keyword(analysis, keywords)
            }
            RuleCondition::Flashing {
                frequency_threshold,
            } => Self::match_flashing(analysis, *frequency_threshold),
            RuleCondition::CopyrightMatch {
                confidence_threshold,
            } => Self::match_copyright(analysis, *confidence_threshold),
            RuleCondition::VolumeSpike { db_threshold } => {
                Self::match_volume_spike(analysis, *db_threshold)
            }
            RuleCondition::Composite {
                conditions,
                operator,
            } => Self::match_composite(analysis, conditions, operator),
        }
    }

    // -----------------------------------------------------------------------
    // Private condition evaluators
    // -----------------------------------------------------------------------

    fn match_visual_object(
        analysis: &AnalysisResult,
        label: &str,
        threshold: f64,
        context_excludes: &[String],
    ) -> Option<RuleEvidence> {
        let visual = analysis.visual.as_ref()?;
        let mut matches = Vec::new();
        for obj in &visual.objects {
            if obj.label.eq_ignore_ascii_case(label) && obj.confidence >= threshold {
                let excluded = context_excludes.iter().any(|ex| {
                    let ex_lower = ex.to_lowercase();
                    obj.context.label().to_lowercase() == ex_lower
                });
                if !excluded {
                    matches.push(format!(
                        "{} @ {:.2}s (conf={:.2}, ctx={})",
                        obj.label,
                        obj.timestamp.seconds,
                        obj.confidence,
                        obj.context.label()
                    ));
                }
            }
        }
        if matches.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: visual.objects.first().map(|o| o.timestamp.clone()),
                confidence: visual
                    .objects
                    .iter()
                    .filter(|o| o.label.eq_ignore_ascii_case(label))
                    .map(|o| o.confidence)
                    .fold(0.0, f64::max),
            })
        }
    }

    fn match_visual_text(
        analysis: &AnalysisResult,
        pattern: &str,
        case_sensitive: bool,
    ) -> Option<RuleEvidence> {
        let visual = analysis.visual.as_ref()?;
        let re = if case_sensitive {
            Regex::new(pattern).ok()?
        } else {
            Regex::new(&format!("(?i){}", regex::escape(pattern))).ok()?
        };
        let mut matches = Vec::new();
        let mut max_confidence = 0.0;
        for txt in &visual.texts {
            let text_to_check = if case_sensitive {
                txt.text.clone()
            } else {
                txt.text.to_lowercase()
            };
            let search_pattern = if case_sensitive {
                pattern.to_string()
            } else {
                pattern.to_lowercase()
            };
            if text_to_check.contains(&search_pattern) || re.is_match(&txt.text) {
                matches.push(format!(
                    "text '{}' @ {:.2}s",
                    txt.text, txt.timestamp.seconds
                ));
                max_confidence = max_confidence.max(txt.confidence);
            }
        }
        if matches.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: visual.texts.first().map(|t| t.timestamp.clone()),
                confidence: max_confidence,
            })
        }
    }

    fn match_audio_event(
        analysis: &AnalysisResult,
        event_type: &str,
        min_confidence: f64,
    ) -> Option<RuleEvidence> {
        let audio = analysis.audio.as_ref()?;
        let mut matches = Vec::new();
        let mut max_confidence = 0.0;
        let mut first_ts: Option<Timestamp> = None;
        for ev in &audio.events {
            if ev.event_type.eq_ignore_ascii_case(event_type) && ev.confidence >= min_confidence {
                matches.push(format!(
                    "{} @ {:.2}s (conf={:.2}, vol={:.1}dB)",
                    ev.event_type, ev.timestamp.seconds, ev.confidence, ev.volume_db
                ));
                max_confidence = max_confidence.max(ev.confidence);
                if first_ts.is_none() {
                    first_ts = Some(ev.timestamp.clone());
                }
            }
        }
        if matches.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: first_ts,
                confidence: max_confidence,
            })
        }
    }

    fn match_transcript_phrase(
        analysis: &AnalysisResult,
        phrases: &[String],
        match_any: bool,
    ) -> Option<RuleEvidence> {
        let audio = analysis.audio.as_ref()?;
        let mut matched_phrases = Vec::new();
        let mut max_confidence = 0.0;
        let mut first_ts: Option<Timestamp> = None;
        for seg in &audio.transcript {
            let seg_lower = seg.text.to_lowercase();
            for phrase in phrases {
                if seg_lower.contains(&phrase.to_lowercase()) {
                    matched_phrases.push(format!(
                        "phrase '{}' in '{}' @ {:.2}s",
                        phrase, seg.text, seg.start.seconds
                    ));
                    max_confidence = max_confidence.max(seg.confidence);
                    if first_ts.is_none() {
                        first_ts = Some(seg.start.clone());
                    }
                    if match_any {
                        return Some(RuleEvidence {
                            matched: true,
                            details: matched_phrases.join("; "),
                            timestamp: first_ts,
                            confidence: max_confidence,
                        });
                    }
                }
            }
        }
        if matched_phrases.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matched_phrases.join("; "),
                timestamp: first_ts,
                confidence: max_confidence,
            })
        }
    }

    fn match_metadata_keyword(
        analysis: &AnalysisResult,
        keywords: &[String],
    ) -> Option<RuleEvidence> {
        let meta = analysis.metadata.as_ref()?;
        let mut matches = Vec::new();
        for kw in keywords {
            let kw_lower = kw.to_lowercase();
            for flagged in &meta.flagged_keywords {
                if flagged.to_lowercase().contains(&kw_lower) {
                    matches.push(format!("keyword '{}' found in metadata", kw));
                }
            }
        }
        if meta.clickbait_score > 0.7 {
            matches.push(format!("high clickbait score: {:.2}", meta.clickbait_score));
        }
        if meta.misleading_score > 0.7 {
            matches.push(format!("high misleading score: {:.2}", meta.misleading_score));
        }
        if meta.keyword_stuffing_score > 0.8 {
            matches.push(format!(
                "keyword stuffing detected: {:.2}",
                meta.keyword_stuffing_score
            ));
        }
        if matches.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: None,
                confidence: meta.overall_risk_score / 100.0,
            })
        }
    }

    fn match_flashing(
        analysis: &AnalysisResult,
        frequency_threshold: f64,
    ) -> Option<RuleEvidence> {
        let visual = analysis.visual.as_ref()?;
        let mut matches = Vec::new();
        let mut max_freq = 0.0;
        let mut first_ts: Option<Timestamp> = None;
        for flash in &visual.flashing {
            if flash.frequency_hz >= *frequency_threshold {
                matches.push(format!(
                    "flashing {:.1}Hz (severity={}) @ {:.2}s..{:.2}s",
                    flash.frequency_hz,
                    flash.severity.label(),
                    flash.start.seconds,
                    flash.end.seconds
                ));
                max_freq = max_freq.max(flash.frequency_hz);
                if first_ts.is_none() {
                    first_ts = Some(flash.start.clone());
                }
            }
        }
        if matches.is_empty() {
            None
        } else {
            let severity_weight = visual
                .flashing
                .iter()
                .map(|f| f.severity.weight())
                .fold(0.0, f64::max);
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: first_ts,
                confidence: (max_freq / 30.0).min(1.0) * (severity_weight / 10.0),
            })
        }
    }

    fn match_copyright(
        analysis: &AnalysisResult,
        confidence_threshold: f64,
    ) -> Option<RuleEvidence> {
        let audio = analysis.audio.as_ref()?;
        let mut matches = Vec::new();
        let mut max_confidence = 0.0;
        let mut first_ts: Option<Timestamp> = None;
        for cm in &audio.copyright_matches {
            if cm.match_confidence >= *confidence_threshold {
                matches.push(format!(
                    "copyright match: '{}' by '{}' (conf={:.2}, type={})",
                    cm.matched_work, cm.copyright_holder, cm.match_confidence, cm.match_type
                ));
                max_confidence = max_confidence.max(cm.match_confidence);
                if first_ts.is_none() {
                    first_ts = Some(cm.start.clone());
                }
            }
        }
        if matches.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: first_ts,
                confidence: max_confidence,
            })
        }
    }

    fn match_volume_spike(
        analysis: &AnalysisResult,
        db_threshold: f64,
    ) -> Option<RuleEvidence> {
        let audio = analysis.audio.as_ref()?;
        let mut matches = Vec::new();
        let mut max_db = 0.0;
        let mut first_ts: Option<Timestamp> = None;
        for ev in &audio.events {
            if ev.volume_db >= *db_threshold {
                matches.push(format!(
                    "volume spike: {:.1}dB (type={}) @ {:.2}s",
                    ev.volume_db, ev.event_type, ev.timestamp.seconds
                ));
                max_db = max_db.max(ev.volume_db);
                if first_ts.is_none() {
                    first_ts = Some(ev.timestamp.clone());
                }
            }
        }
        if matches.is_empty() {
            None
        } else {
            Some(RuleEvidence {
                matched: true,
                details: matches.join("; "),
                timestamp: first_ts,
                confidence: (max_db / 120.0).min(1.0),
            })
        }
    }

    fn match_composite(
        analysis: &AnalysisResult,
        conditions: &[RuleCondition],
        operator: &CompositeOperator,
    ) -> Option<RuleEvidence> {
        match operator {
            CompositeOperator::And => {
                let mut all_evidence = Vec::new();
                let mut all_confidence = 1.0;
                for cond in conditions {
                    match cond.evaluate(analysis) {
                        Some(ev) => {
                            all_confidence *= ev.confidence;
                            all_evidence.push(ev.details);
                        }
                        None => return None,
                    }
                }
                Some(RuleEvidence {
                    matched: true,
                    details: all_evidence.join(" AND "),
                    timestamp: None,
                    confidence: all_confidence,
                })
            }
            CompositeOperator::Or => {
                let mut any_evidence = Vec::new();
                let mut max_confidence = 0.0;
                let mut any_ts: Option<Timestamp> = None;
                for cond in conditions {
                    if let Some(ev) = cond.evaluate(analysis) {
                        any_evidence.push(ev.details);
                        max_confidence = max_confidence.max(ev.confidence);
                        if any_ts.is_none() {
                            any_ts = ev.timestamp;
                        }
                    }
                }
                if any_evidence.is_empty() {
                    None
                } else {
                    Some(RuleEvidence {
                        matched: true,
                        details: any_evidence.join(" OR "),
                        timestamp: any_ts,
                        confidence: max_confidence,
                    })
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RuleEvidence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleEvidence {
    pub matched: bool,
    pub details: String,
    pub timestamp: Option<Timestamp>,
    pub confidence: f64,
}

// ---------------------------------------------------------------------------
// RuleMatch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct RuleMatch {
    pub rule_id: String,
    pub rule_name: String,
    pub category: ViolationCategory,
    pub severity: Severity,
    pub action: RuleAction,
    pub confidence: f64,
    pub evidence: String,
    pub timestamp: Option<Timestamp>,
    pub regions: Vec<String>,
}

impl RuleMatch {
    pub fn to_violation(&self) -> sentinel_core::proto::Violation {
        sentinel_core::proto::Violation::new(&self.evidence)
            .with_category(self.category.clone())
            .with_severity(self.severity.clone())
            .with_confidence(self.confidence)
            .with_evidence(format!(
                "Rule '{}' matched: {} [action={}]",
                self.rule_name,
                self.evidence,
                self.action.label()
            ))
            .with_guideline(format!(
                "https://support.google.com/youtube/answer/{}",
                category_help_id(&self.category)
            ))
    }
}

fn category_help_id(category: &ViolationCategory) -> &'static str {
    match category {
        ViolationCategory::HateSpeech => "2801939",
        ViolationCategory::Harassment => "2801924",
        ViolationCategory::Violence => "2802008",
        ViolationCategory::AdultContent => "2803176",
        ViolationCategory::HarmfulDangerous => "2801964",
        ViolationCategory::Misinformation => "2797387",
        ViolationCategory::ChildSafety => "2802271",
        ViolationCategory::Copyright => "2797370",
        ViolationCategory::SpamDeceptive => "2801973",
        ViolationCategory::FlashingSeizure => "6140493",
        ViolationCategory::ThumbnailIssue => "7240418513",
        ViolationCategory::Unspecified => "2801939",
    }
}

// ---------------------------------------------------------------------------
// PolicyRule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: String,
    pub name: String,
    #[serde(with = "serde_plain_violation_category")]
    pub category: ViolationCategory,
    pub severity: Severity,
    pub conditions: Vec<RuleCondition>,
    pub action: RuleAction,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
}

impl PolicyRule {
    /// Evaluate this rule against an analysis result.
    pub fn evaluate(&self, analysis: &AnalysisResult) -> Option<RuleMatch> {
        if !self.enabled {
            return None;
        }
        let mut all_evidence = Vec::new();
        let mut max_confidence = 0.0;
        let mut first_ts: Option<Timestamp> = None;
        let mut any_matched = false;

        for condition in &self.conditions {
            if let Some(evidence) = condition.evaluate(analysis) {
                any_matched = true;
                all_evidence.push(evidence.details);
                max_confidence = max_confidence.max(evidence.confidence);
                if first_ts.is_none() {
                    first_ts = evidence.timestamp;
                }
            }
        }

        if any_matched {
            Some(RuleMatch {
                rule_id: self.id.clone(),
                rule_name: self.name.clone(),
                category: self.category.clone(),
                severity: self.severity.clone(),
                action: self.action.clone(),
                confidence: max_confidence.min(1.0),
                evidence: all_evidence.join("; "),
                timestamp: first_ts,
                regions: self.regions.clone(),
            })
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Serde helper for ViolationCategory (plain string, not tagged)
// ---------------------------------------------------------------------------

mod serde_plain_violation_category {
    use super::ViolationCategory;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(cat: &ViolationCategory, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&cat.label())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ViolationCategory, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "hate_speech" => ViolationCategory::HateSpeech,
            "harassment" => ViolationCategory::Harassment,
            "violence" => ViolationCategory::Violence,
            "adult_content" => ViolationCategory::AdultContent,
            "harmful_dangerous" => ViolationCategory::HarmfulDangerous,
            "misinformation" => ViolationCategory::Misinformation,
            "child_safety" => ViolationCategory::ChildSafety,
            "copyright" => ViolationCategory::Copyright,
            "spam_deceptive" => ViolationCategory::SpamDeceptive,
            "flashing_seizure" => ViolationCategory::FlashingSeizure,
            "thumbnail_issue" => ViolationCategory::ThumbnailIssue,
            _ => ViolationCategory::Unspecified,
        })
    }
}

// ---------------------------------------------------------------------------
// RuleSet
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RuleSet {
    rules: Vec<PolicyRule>,
    rule_lookup: HashMap<String, usize>,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleSet {
    /// Create an empty rule set.
    pub fn new() -> Self {
        RuleSet {
            rules: Vec::new(),
            rule_lookup: HashMap::new(),
        }
    }

    /// Add a rule to the set. Returns the previous rule with the same ID if any.
    pub fn add_rule(&mut self, rule: PolicyRule) -> Option<PolicyRule> {
        let id = rule.id.clone();
        if let Some(&idx) = self.rule_lookup.get(&id) {
            let old = self.rules[idx].clone();
            self.rules[idx] = rule;
            Some(old)
        } else {
            let idx = self.rules.len();
            self.rules.push(rule);
            self.rule_lookup.insert(id, idx);
            None
        }
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&mut self, rule_id: &str) -> Option<PolicyRule> {
        let idx = self.rule_lookup.remove(rule_id)?;
        let rule = self.rules.remove(idx);
        // Rebuild indices after removal
        self.rule_lookup.clear();
        for (i, r) in self.rules.iter().enumerate() {
            self.rule_lookup.insert(r.id.clone(), i);
        }
        Some(rule)
    }

    /// Get a rule by ID.
    pub fn get_rule(&self, rule_id: &str) -> Option<&PolicyRule> {
        self.rule_lookup
            .get(rule_id)
            .and_then(|&idx| self.rules.get(idx))
    }

    /// Enable or disable a rule.
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) -> bool {
        if let Some(&idx) = self.rule_lookup.get(rule_id) {
            if let Some(rule) = self.rules.get_mut(idx) {
                rule.enabled = enabled;
                return true;
            }
        }
        false
    }

    /// Evaluate all rules against an analysis result and return matches.
    pub fn evaluate(&self, analysis: &AnalysisResult) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        for rule in &self.rules {
            if let Some(m) = rule.evaluate(analysis) {
                matches.push(m);
            }
        }
        // Sort by confidence descending
        matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        matches
    }

    /// Load rules from a JSON string.
    pub fn load_from_json(&mut self, json: &str) -> Result<(), crate::PolicyEngineError> {
        let new_rules: Vec<PolicyRule> =
            serde_json::from_str(json).map_err(crate::PolicyEngineError::JsonParse)?;
        for rule in new_rules {
            self.add_rule(rule);
        }
        Ok(())
    }

    /// Serialize current rules to JSON.
    pub fn to_json(&self) -> Result<String, crate::PolicyEngineError> {
        serde_json::to_string_pretty(&self.rules)
            .map_err(crate::PolicyEngineError::JsonParse)
    }

    /// Return all rules.
    pub fn all_rules(&self) -> &[PolicyRule] {
        &self.rules
    }

    /// Number of rules in the set.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Load the built-in default rules from the embedded JSON file.
    pub fn load_defaults(&mut self) {
        const DEFAULT_RULES_JSON: &str =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/data/default_policies.json"));
        if let Ok(rules) = serde_json::from_str::<Vec<PolicyRule>>(DEFAULT_RULES_JSON) {
            for rule in rules {
                self.add_rule(rule);
            }
            tracing::info!("Loaded {} default policy rules", self.rules.len());
        } else {
            tracing::error!("Failed to parse default_policies.json");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::proto::{AudioAnalysis, BoundingBox, ContextType, Timestamp, VisualAnalysis};

    fn make_test_analysis() -> AnalysisResult {
        AnalysisResult {
            job_id: uuid::Uuid::new_v4(),
            source: sentinel_core::proto::VideoSource::new("test", "/tmp/test.mp4"),
            visual: Some(VisualAnalysis {
                objects: vec![
                    sentinel_core::proto::DetectedObject::new("weapon", 0.92)
                        .with_category(ViolationCategory::Violence)
                        .with_context(ContextType::Gratuitous)
                        .at_timestamp(Timestamp::new(10.0, 300))
                        .with_bbox(BoundingBox::new(0.2, 0.2, 0.8, 0.8)),
                ],
                texts: vec![
                    sentinel_core::proto::DetectedText::new("hate speech example", 0.85)
                        .at_timestamp(Timestamp::new(5.0, 150)),
                ],
                transitions: vec![],
                flashing: vec![FlashingSegment {
                    start: Timestamp::new(20.0, 600),
                    end: Timestamp::new(22.0, 660),
                    frequency_hz: 12.0,
                    severity: Severity::High,
                }],
                overall_risk_score: 45.0,
            }),
            audio: Some(AudioAnalysis {
                transcript: vec![
                    sentinel_core::proto::TranscriptSegment {
                        start: Timestamp::new(0.0, 0),
                        end: Timestamp::new(5.0, 150),
                        text: "This is harmful content with profanity".into(),
                        confidence: 0.9,
                        sentiment: -0.6,
                        is_sarcasm: false,
                        is_background_speech: false,
                    },
                ],
                events: vec![
                    AudioEvent::new("gunshot", 0.88, Timestamp::new(10.5, 315)),
                ],
                copyright_matches: vec![
                    sentinel_core::proto::CopyrightMatch::new("Famous Song", "Major Label")
                        .with_confidence(0.92)
                        .with_match_type("audio_fingerprint")
                        .with_span(Timestamp::new(30.0, 900), Timestamp::new(35.0, 1050)),
                ],
                overall_risk_score: 50.0,
            }),
            metadata: Some(sentinel_core::proto::MetadataAnalysis {
                clickbait_score: 0.85,
                keyword_stuffing_score: 0.3,
                misleading_score: 0.7,
                flagged_keywords: vec!["clickbait".into(), "scam".into()],
                suggestions: vec![],
                overall_risk_score: 62.0,
            }),
            final_score: 50.0,
            completed_at: None,
        }
    }

    #[test]
    fn rule_condition_visual_object_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::VisualObject {
            label: "weapon".into(),
            confidence_threshold: 0.5,
            context_excludes: vec![],
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.matched);
        assert!(ev.confidence > 0.9);
    }

    #[test]
    fn rule_condition_visual_object_no_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::VisualObject {
            label: "puppy".into(),
            confidence_threshold: 0.1,
            context_excludes: vec![],
        };
        assert!(cond.evaluate(&analysis).is_none());
    }

    #[test]
    fn rule_condition_visual_text_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::VisualText {
            pattern: "hate speech".into(),
            case_sensitive: false,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        assert!(result.unwrap().details.contains("hate speech"));
    }

    #[test]
    fn rule_condition_visual_text_no_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::VisualText {
            pattern: "friendly content".into(),
            case_sensitive: false,
        };
        assert!(cond.evaluate(&analysis).is_none());
    }

    #[test]
    fn rule_condition_audio_event_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::AudioEvent {
            event_type: "gunshot".into(),
            min_confidence: 0.5,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.confidence > 0.85);
    }

    #[test]
    fn rule_condition_transcript_phrase_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::TranscriptPhrase {
            phrases: vec!["harmful content".into()],
            match_any: true,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        assert!(result.unwrap().details.contains("harmful content"));
    }

    #[test]
    fn rule_condition_metadata_keyword_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::MetadataKeyword {
            keywords: vec!["scam".into()],
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.details.contains("scam"));
    }

    #[test]
    fn rule_condition_flashing_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::Flashing {
            frequency_threshold: 10.0,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.details.contains("12.0Hz"));
    }

    #[test]
    fn rule_condition_copyright_match() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::CopyrightMatch {
            confidence_threshold: 0.9,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.confidence > 0.91);
    }

    #[test]
    fn rule_condition_composite_and() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::Composite {
            conditions: vec![
                RuleCondition::VisualObject {
                    label: "weapon".into(),
                    confidence_threshold: 0.5,
                    context_excludes: vec![],
                },
                RuleCondition::AudioEvent {
                    event_type: "gunshot".into(),
                    min_confidence: 0.5,
                },
            ],
            operator: CompositeOperator::And,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.details.contains("AND"));
        assert!(ev.matched);
    }

    #[test]
    fn rule_condition_composite_or() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::Composite {
            conditions: vec![
                RuleCondition::VisualObject {
                    label: "nonexistent".into(),
                    confidence_threshold: 0.1,
                    context_excludes: vec![],
                },
                RuleCondition::AudioEvent {
                    event_type: "gunshot".into(),
                    min_confidence: 0.5,
                },
            ],
            operator: CompositeOperator::Or,
        };
        let result = cond.evaluate(&analysis);
        assert!(result.is_some());
        let ev = result.unwrap();
        assert!(ev.details.contains("gunshot"));
        assert!(ev.matched);
    }

    #[test]
    fn rule_condition_composite_and_fails() {
        let analysis = make_test_analysis();
        let cond = RuleCondition::Composite {
            conditions: vec![
                RuleCondition::VisualObject {
                    label: "nonexistent".into(),
                    confidence_threshold: 0.1,
                    context_excludes: vec![],
                },
                RuleCondition::AudioEvent {
                    event_type: "gunshot".into(),
                    min_confidence: 0.5,
                },
            ],
            operator: CompositeOperator::And,
        };
        assert!(cond.evaluate(&analysis).is_none());
    }

    #[test]
    fn policy_rule_evaluate_match() {
        let analysis = make_test_analysis();
        let rule = PolicyRule {
            id: "rule-001".into(),
            name: "Weapon Detection".into(),
            category: ViolationCategory::Violence,
            severity: Severity::High,
            conditions: vec![RuleCondition::VisualObject {
                label: "weapon".into(),
                confidence_threshold: 0.5,
                context_excludes: vec![],
            }],
            action: RuleAction::Flag,
            regions: vec![],
            enabled: true,
            description: "Detects weapons in video".into(),
        };
        let result = rule.evaluate(&analysis);
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.rule_id, "rule-001");
        assert_eq!(m.category, ViolationCategory::Violence);
        assert_eq!(m.severity, Severity::High);
    }

    #[test]
    fn policy_rule_evaluate_disabled() {
        let analysis = make_test_analysis();
        let rule = PolicyRule {
            id: "rule-002".into(),
            name: "Disabled Rule".into(),
            category: ViolationCategory::Violence,
            severity: Severity::Critical,
            conditions: vec![RuleCondition::VisualObject {
                label: "weapon".into(),
                confidence_threshold: 0.5,
                context_excludes: vec![],
            }],
            action: RuleAction::Block,
            regions: vec![],
            enabled: false,
            description: "Should not trigger".into(),
        };
        assert!(rule.evaluate(&analysis).is_none());
    }

    #[test]
    fn rule_set_add_and_get() {
        let mut rs = RuleSet::new();
        let rule = PolicyRule {
            id: "r1".into(),
            name: "Test".into(),
            category: ViolationCategory::HateSpeech,
            severity: Severity::Medium,
            conditions: vec![],
            action: RuleAction::Flag,
            regions: vec![],
            enabled: true,
            description: "".into(),
        };
        rs.add_rule(rule.clone());
        assert_eq!(rs.len(), 1);
        let got = rs.get_rule("r1");
        assert!(got.is_some());
        assert_eq!(got.unwrap().name, "Test");
    }

    #[test]
    fn rule_set_remove() {
        let mut rs = RuleSet::new();
        rs.add_rule(PolicyRule {
            id: "r1".into(),
            name: "Test".into(),
            category: ViolationCategory::HateSpeech,
            severity: Severity::Medium,
            conditions: vec![],
            action: RuleAction::Flag,
            regions: vec![],
            enabled: true,
            description: "".into(),
        });
        let removed = rs.remove_rule("r1");
        assert!(removed.is_some());
        assert_eq!(rs.len(), 0);
    }

    #[test]
    fn rule_set_evaluate() {
        let mut rs = RuleSet::new();
        rs.add_rule(PolicyRule {
            id: "weapon-rule".into(),
            name: "Weapon".into(),
            category: ViolationCategory::Violence,
            severity: Severity::High,
            conditions: vec![RuleCondition::VisualObject {
                label: "weapon".into(),
                confidence_threshold: 0.5,
                context_excludes: vec![],
            }],
            action: RuleAction::Flag,
            regions: vec![],
            enabled: true,
            description: "".into(),
        });
        let analysis = make_test_analysis();
        let matches = rs.evaluate(&analysis);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule_id, "weapon-rule");
    }

    #[test]
    fn rule_set_load_from_json() {
        let json = r#"[
            {
                "id": "json-rule-1",
                "name": "Hate Speech",
                "category": "hate_speech",
                "severity": "High",
                "conditions": [
                    {
                        "type": "transcript_phrase",
                        "phrases": ["hate"],
                        "match_any": true
                    }
                ],
                "action": "flag",
                "enabled": true
            }
        ]"#;
        let mut rs = RuleSet::new();
        rs.load_from_json(json).unwrap();
        assert_eq!(rs.len(), 1);
        let rule = rs.get_rule("json-rule-1");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().name, "Hate Speech");
    }

    #[test]
    fn rule_set_load_from_json_bad() {
        let mut rs = RuleSet::new();
        let result = rs.load_from_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn rule_set_to_json() {
        let mut rs = RuleSet::new();
        rs.add_rule(PolicyRule {
            id: "r1".into(),
            name: "Test".into(),
            category: ViolationCategory::SpamDeceptive,
            severity: Severity::Low,
            conditions: vec![],
            action: RuleAction::LogOnly,
            regions: vec!["US".into()],
            enabled: true,
            description: "A test rule".into(),
        });
        let json = rs.to_json();
        assert!(json.is_ok());
        let s = json.unwrap();
        assert!(s.contains("r1"));
        assert!(s.contains("Test"));
    }

    #[test]
    fn rule_action_labels() {
        assert_eq!(RuleAction::Flag.label(), "flag");
        assert_eq!(
            RuleAction::AutoFix {
                fix_type: "blur".into()
            }
            .label(),
            "auto_fix:blur"
        );
        assert_eq!(RuleAction::Block.label(), "block");
        assert_eq!(RuleAction::Escalate.label(), "escalate");
        assert_eq!(RuleAction::LogOnly.label(), "log_only");
    }

    #[test]
    fn rule_match_to_violation() {
        let m = RuleMatch {
            rule_id: "rm1".into(),
            rule_name: "Test Match".into(),
            category: ViolationCategory::Violence,
            severity: Severity::High,
            action: RuleAction::Block,
            confidence: 0.85,
            evidence: "Weapon detected".into(),
            timestamp: Some(Timestamp::new(5.0, 150)),
            regions: vec![],
        };
        let v = m.to_violation();
        assert_eq!(v.category, ViolationCategory::Violence);
        assert_eq!(v.severity, Severity::High);
        assert!((v.confidence - 0.85).abs() < 0.01);
    }
}
