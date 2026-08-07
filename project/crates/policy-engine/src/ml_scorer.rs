use sentinel_core::proto::{
    AnalysisResult, AudioAnalysis, MetadataAnalysis, Severity, VisualAnalysis, ViolationCategory,
};

// ---------------------------------------------------------------------------
// RiskScores
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RiskScores {
    /// Probability of receiving a community guidelines strike.
    pub strike_probability: f64,
    /// Probability of demonetization (limited ads).
    pub demonetization_probability: f64,
    /// Probability of a copyright claim or takedown.
    pub copyright_probability: f64,
    /// Overall composite risk score (0..1).
    pub overall_risk: f64,
    /// Per-category risk contributions.
    pub category_scores: std::collections::HashMap<ViolationCategory, f64>,
    /// Human-readable explanation of the scoring.
    pub explanation: Vec<String>,
}

impl RiskScores {
    /// Returns the highest risk category and its score.
    pub fn highest_risk_category(&self) -> Option<(ViolationCategory, f64)> {
        self.category_scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(k, v)| (k.clone(), *v))
    }

    /// Returns true if the overall risk exceeds a threshold.
    pub fn exceeds_threshold(&self, threshold: f64) -> bool {
        self.overall_risk >= threshold
    }

    /// Generate a human-readable risk summary.
    pub fn summary(&self) -> String {
        let mut lines = vec![
            format!("Strike probability: {:.1}%", self.strike_probability * 100.0),
            format!(
                "Demonetization probability: {:.1}%",
                self.demonetization_probability * 100.0
            ),
            format!(
                "Copyright probability: {:.1}%",
                self.copyright_probability * 100.0
            ),
            format!("Overall risk: {:.1}%", self.overall_risk * 100.0),
        ];
        if let Some((cat, score)) = self.highest_risk_category() {
            lines.push(format!(
                "Highest risk category: {} ({:.1}%)",
                cat.label(),
                score * 100.0
            ));
        }
        if !self.explanation.is_empty() {
            lines.push("Explanations:".into());
            for e in &self.explanation {
                lines.push(format!("  - {}", e));
            }
        }
        lines.join("\n")
    }

    /// Clamp all probabilities to [0.0, 1.0].
    pub fn clamp(&mut self) {
        self.strike_probability = self.strike_probability.clamp(0.0, 1.0);
        self.demonetization_probability = self.demonetization_probability.clamp(0.0, 1.0);
        self.copyright_probability = self.copyright_probability.clamp(0.0, 1.0);
        self.overall_risk = self.overall_risk.clamp(0.0, 1.0);
        for v in self.category_scores.values_mut() {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

// ---------------------------------------------------------------------------
// CategoryInteractionMatrix
// ---------------------------------------------------------------------------

/// Models multiplicative interaction effects between categories.
/// Certain category combinations amplify risk beyond their individual scores.
#[derive(Debug, Clone)]
pub struct CategoryInteractionMatrix {
    /// Base interactions: (cat_a, cat_b) -> multiplier (both orders treated equally)
    interactions: std::collections::HashMap<(ViolationCategory, ViolationCategory), f64>,
}

impl Default for CategoryInteractionMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl CategoryInteractionMatrix {
    /// Create the default interaction matrix with known amplifying pairs.
    pub fn new() -> Self {
        let mut interactions = std::collections::HashMap::new();

        // Violence + HateSpeech -> amplified strike risk
        interactions.insert(
            (ViolationCategory::Violence, ViolationCategory::HateSpeech),
            1.4,
        );
        interactions.insert(
            (ViolationCategory::HateSpeech, ViolationCategory::Violence),
            1.4,
        );

        // Violence + ChildSafety -> critical amplification
        interactions.insert(
            (ViolationCategory::Violence, ViolationCategory::ChildSafety),
            1.8,
        );
        interactions.insert(
            (ViolationCategory::ChildSafety, ViolationCategory::Violence),
            1.8,
        );

        // AdultContent + ChildSafety -> extremely high risk
        interactions.insert(
            (ViolationCategory::AdultContent, ViolationCategory::ChildSafety),
            2.0,
        );
        interactions.insert(
            (ViolationCategory::ChildSafety, ViolationCategory::AdultContent),
            2.0,
        );

        // HateSpeech + Harassment -> amplified
        interactions.insert(
            (ViolationCategory::HateSpeech, ViolationCategory::Harassment),
            1.3,
        );
        interactions.insert(
            (ViolationCategory::Harassment, ViolationCategory::HateSpeech),
            1.3,
        );

        // Misinformation + HarmfulDangerous -> public safety risk
        interactions.insert(
            (ViolationCategory::Misinformation, ViolationCategory::HarmfulDangerous),
            1.5,
        );
        interactions.insert(
            (ViolationCategory::HarmfulDangerous, ViolationCategory::Misinformation),
            1.5,
        );

        // SpamDeceptive + Misinformation -> platform manipulation
        interactions.insert(
            (ViolationCategory::SpamDeceptive, ViolationCategory::Misinformation),
            1.3,
        );
        interactions.insert(
            (ViolationCategory::Misinformation, ViolationCategory::SpamDeceptive),
            1.3,
        );

        // Copyright + SpamDeceptive -> repeat offender pattern
        interactions.insert(
            (ViolationCategory::Copyright, ViolationCategory::SpamDeceptive),
            1.2,
        );
        interactions.insert(
            (ViolationCategory::SpamDeceptive, ViolationCategory::Copyright),
            1.2,
        );

        CategoryInteractionMatrix { interactions }
    }

    /// Get the interaction multiplier for a pair of categories.
    /// Returns 1.0 (no interaction) if not explicitly defined.
    pub fn get_multiplier(
        &self,
        a: &ViolationCategory,
        b: &ViolationCategory,
    ) -> f64 {
        self.interactions
            .get(&(a.clone(), b.clone()))
            .copied()
            .unwrap_or(1.0)
    }

    /// Compute the combined interaction score for a set of category scores.
    pub fn apply_interactions(
        &self,
        category_scores: &std::collections::HashMap<ViolationCategory, f64>,
    ) -> f64 {
        let mut multiplier = 1.0;
        let cats: Vec<_> = category_scores.keys().collect();
        for (i, a) in cats.iter().enumerate() {
            for b in cats.iter().skip(i + 1) {
                if category_scores[*a] > 0.0 && category_scores[*b] > 0.0 {
                    let m = self.get_multiplier(a, b);
                    // The interaction amplifies proportionally to both scores
                    let pair_weight =
                        category_scores[*a].min(category_scores[*b]) * (m - 1.0);
                    multiplier += pair_weight;
                }
            }
        }
        multiplier
    }
}

// ---------------------------------------------------------------------------
// PlattScaling
// ---------------------------------------------------------------------------

/// Platt scaling (sigmoid calibration) converts raw classifier scores into
/// calibrated probabilities: P(y=1|x) = 1 / (1 + exp(A*x + B))
#[derive(Debug, Clone)]
pub struct PlattScaling {
    /// Slope parameter (controls steepness).
    pub a: f64,
    /// Offset parameter (controls shift).
    pub b: f64,
}

impl Default for PlattScaling {
    fn default() -> Self {
        Self::new(-2.0, 0.0)
    }
}

impl PlattScaling {
    /// Create Platt scaling with given parameters.
    pub fn new(a: f64, b: f64) -> Self {
        PlattScaling { a, b }
    }

    /// Calibrate a raw score [0..1] into a probability [0..1].
    pub fn calibrate(&self, raw_score: f64) -> f64 {
        if raw_score <= 0.0 {
            return 0.0;
        }
        if raw_score >= 1.0 {
            return 1.0;
        }
        let exp_arg = self.a * raw_score + self.b;
        // Clip to avoid overflow
        let clipped = exp_arg.clamp(-709.0, 709.0);
        1.0 / (1.0 + clipped.exp())
    }

    /// Calibrate multiple raw scores.
    pub fn calibrate_batch(&self, raw_scores: &[f64]) -> Vec<f64> {
        raw_scores.iter().map(|&s| self.calibrate(s)).collect()
    }
}

// ---------------------------------------------------------------------------
// MlRiskScorer
// ---------------------------------------------------------------------------

/// ML-based risk scorer that produces calibrated probability estimates.
pub struct MlRiskScorer {
    /// Visual stage weight.
    visual_weight: f64,
    /// Audio stage weight.
    audio_weight: f64,
    /// Metadata stage weight.
    metadata_weight: f64,
    /// Category interaction matrix.
    interactions: CategoryInteractionMatrix,
    /// Platt scaling for strike probability.
    strike_platt: PlattScaling,
    /// Platt scaling for demonetization probability.
    demon_platt: PlattScaling,
    /// Platt scaling for copyright probability.
    copy_platt: PlattScaling,
    /// Platt scaling for overall risk.
    overall_platt: PlattScaling,
    /// Category severity weights for strike risk.
    strike_category_weights: std::collections::HashMap<ViolationCategory, f64>,
    /// Category severity weights for demonetization risk.
    demon_category_weights: std::collections::HashMap<ViolationCategory, f64>,
}

impl Default for MlRiskScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl MlRiskScorer {
    /// Create a new ML risk scorer with default parameters.
    pub fn new() -> Self {
        let mut strike_weights = std::collections::HashMap::new();
        strike_weights.insert(ViolationCategory::HateSpeech, 0.95);
        strike_weights.insert(ViolationCategory::Harassment, 0.75);
        strike_weights.insert(ViolationCategory::Violence, 0.85);
        strike_weights.insert(ViolationCategory::AdultContent, 0.70);
        strike_weights.insert(ViolationCategory::HarmfulDangerous, 0.90);
        strike_weights.insert(ViolationCategory::Misinformation, 0.65);
        strike_weights.insert(ViolationCategory::ChildSafety, 0.98);
        strike_weights.insert(ViolationCategory::Copyright, 0.50);
        strike_weights.insert(ViolationCategory::SpamDeceptive, 0.55);
        strike_weights.insert(ViolationCategory::FlashingSeizure, 0.45);
        strike_weights.insert(ViolationCategory::ThumbnailIssue, 0.25);
        strike_weights.insert(ViolationCategory::Unspecified, 0.10);

        let mut demon_weights = std::collections::HashMap::new();
        demon_weights.insert(ViolationCategory::HateSpeech, 0.90);
        demon_weights.insert(ViolationCategory::Harassment, 0.65);
        demon_weights.insert(ViolationCategory::Violence, 0.80);
        demon_weights.insert(ViolationCategory::AdultContent, 0.85);
        demon_weights.insert(ViolationCategory::HarmfulDangerous, 0.75);
        demon_weights.insert(ViolationCategory::Misinformation, 0.70);
        demon_weights.insert(ViolationCategory::ChildSafety, 0.95);
        demon_weights.insert(ViolationCategory::Copyright, 0.40);
        demon_weights.insert(ViolationCategory::SpamDeceptive, 0.60);
        demon_weights.insert(ViolationCategory::FlashingSeizure, 0.55);
        demon_weights.insert(ViolationCategory::ThumbnailIssue, 0.35);
        demon_weights.insert(ViolationCategory::Unspecified, 0.10);

        MlRiskScorer {
            visual_weight: 0.45,
            audio_weight: 0.30,
            metadata_weight: 0.25,
            interactions: CategoryInteractionMatrix::new(),
            strike_platt: PlattScaling::new(-4.0, 1.0),
            demon_platt: PlattScaling::new(-3.0, 0.5),
            copy_platt: PlattScaling::new(-5.0, 2.0),
            overall_platt: PlattScaling::new(-2.5, 0.0),
            strike_category_weights: strike_weights,
            demon_category_weights: demon_weights,
        }
    }

    /// Set custom stage weights (must sum to <= 1.0).
    pub fn with_weights(mut self, visual: f64, audio: f64, metadata: f64) -> Self {
        let total = visual + audio + metadata;
        if total > 0.0 {
            self.visual_weight = visual / total;
            self.audio_weight = audio / total;
            self.metadata_weight = metadata / total;
        }
        self
    }

    /// Score a complete analysis result and return calibrated risk probabilities.
    pub fn score(&self, analysis: &AnalysisResult) -> RiskScores {
        let mut scores = RiskScores::default();
        let mut explanations = Vec::new();

        // --- Per-stage raw scores ---
        let visual_scores = self.score_visual(analysis.visual.as_ref());
        let audio_scores = self.score_audio(analysis.audio.as_ref());
        let metadata_scores = self.score_metadata(analysis.metadata.as_ref());

        // --- Combine category scores with weights ---
        let mut combined_categories: std::collections::HashMap<ViolationCategory, f64> =
            std::collections::HashMap::new();

        for (cat, score) in &visual_scores {
            let weighted = score * self.visual_weight;
            let entry = combined_categories.entry(cat.clone()).or_insert(0.0);
            *entry = (*entry + weighted).min(1.0);
        }
        for (cat, score) in &audio_scores {
            let weighted = score * self.audio_weight;
            let entry = combined_categories.entry(cat.clone()).or_insert(0.0);
            *entry = (*entry + weighted).min(1.0);
        }
        for (cat, score) in &metadata_scores {
            let weighted = score * self.metadata_weight;
            let entry = combined_categories.entry(cat.clone()).or_insert(0.0);
            *entry = (*entry + weighted).min(1.0);
        }

        // --- Apply category interaction multipliers ---
        let interaction_multiplier =
            self.interactions.apply_interactions(&combined_categories);
        for v in combined_categories.values_mut() {
            *v = (*v * interaction_multiplier).min(1.0);
        }

        if interaction_multiplier > 1.05 {
            explanations.push(format!(
                "Category interaction multiplier: {:.2}",
                interaction_multiplier
            ));
        }

        // --- Compute strike probability ---
        let strike_raw = self.compute_strike_raw(&combined_categories);
        scores.strike_probability = self.strike_platt.calibrate(strike_raw);

        // --- Compute demonetization probability ---
        let demon_raw = self.compute_demonetization_raw(&combined_categories);
        scores.demonetization_probability = self.demon_platt.calibrate(demon_raw);

        // --- Compute copyright probability ---
        let copy_raw = self.compute_copyright_raw(analysis.audio.as_ref());
        scores.copyright_probability = self.copy_platt.calibrate(copy_raw);

        // --- Overall risk: weighted combination + calibration ---
        let overall_raw = 0.4 * strike_raw + 0.35 * demon_raw + 0.25 * copy_raw;
        scores.overall_risk = self.overall_platt.calibrate(overall_raw);

        scores.category_scores = combined_categories;
        scores.explanation = explanations;

        // Add per-stage explanations
        if analysis.visual.is_some() {
            let v_score = analysis.visual.as_ref().unwrap().overall_risk_score / 100.0;
            if v_score > 0.3 {
                scores.explanation.push(format!(
                    "Visual risk score: {:.1}%",
                    v_score * 100.0
                ));
            }
        }
        if analysis.audio.is_some() {
            let a_score = analysis.audio.as_ref().unwrap().overall_risk_score / 100.0;
            if a_score > 0.3 {
                scores.explanation.push(format!(
                    "Audio risk score: {:.1}%",
                    a_score * 100.0
                ));
            }
        }
        if analysis.metadata.is_some() {
            let m_score = analysis.metadata.as_ref().unwrap().overall_risk_score / 100.0;
            if m_score > 0.3 {
                scores.explanation.push(format!(
                    "Metadata risk score: {:.1}%",
                    m_score * 100.0
                ));
            }
        }

        scores.clamp();
        scores
    }

    // -----------------------------------------------------------------------
    // Stage-specific scoring
    // -----------------------------------------------------------------------

    fn score_visual(
        &self,
        visual: Option<&VisualAnalysis>,
    ) -> std::collections::HashMap<ViolationCategory, f64> {
        let mut scores = std::collections::HashMap::new();
        let v = match visual {
            Some(v) => v,
            None => return scores,
        };

        // Object-based category scores
        for obj in &v.objects {
            if obj.confidence > 0.3 {
                let entry = scores.entry(obj.category.clone()).or_insert(0.0);
                let ctx_modifier = 1.0 + obj.context.risk_modifier();
                *entry = (*entry + obj.confidence * ctx_modifier).min(1.0);
            }
        }

        // Flashing scores
        if !v.flashing.is_empty() {
            let flash_score: f64 = v
                .flashing
                .iter()
                .map(|f| {
                    let freq_factor = (f.frequency_hz / 20.0).min(1.0);
                    freq_factor * f.severity.weight() / 10.0
                })
                .sum::<f64>()
                .min(1.0);
            let entry = scores
                .entry(ViolationCategory::FlashingSeizure)
                .or_insert(0.0);
            *entry = (*entry + flash_score).min(1.0);
        }

        // Text-based scores (profanity -> hate speech)
        for txt in &v.texts {
            if txt.is_profanity && txt.confidence > 0.5 {
                let entry = scores.entry(ViolationCategory::HateSpeech).or_insert(0.0);
                *entry = (*entry + txt.confidence * 0.5).min(1.0);
            }
        }

        scores
    }

    fn score_audio(
        &self,
        audio: Option<&AudioAnalysis>,
    ) -> std::collections::HashMap<ViolationCategory, f64> {
        let mut scores = std::collections::HashMap::new();
        let a = match audio {
            Some(a) => a,
            None => return scores,
        };

        // Audio event scoring
        for ev in &a.events {
            if ev.confidence > 0.3 {
                let category = audio_event_to_category(&ev.event_type);
                let entry = scores.entry(category).or_insert(0.0);
                *entry = (*entry + ev.confidence).min(1.0);
            }
        }

        // Transcript sentiment scoring (negative sentiment -> harassment)
        for seg in &a.transcript {
            if seg.sentiment < -0.5 && seg.confidence > 0.5 {
                let entry = scores.entry(ViolationCategory::Harassment).or_insert(0.0);
                let sentiment_score = seg.sentiment.abs().min(1.0);
                *entry = (*entry + sentiment_score * 0.4).min(1.0);
            }
            if seg.is_sarcasm && seg.confidence > 0.5 {
                // Sarcasm in negative context -> harassment
                let entry = scores.entry(ViolationCategory::Harassment).or_insert(0.0);
                *entry = (*entry + seg.confidence * 0.2).min(1.0);
            }
        }

        scores
    }

    fn score_metadata(
        &self,
        meta: Option<&MetadataAnalysis>,
    ) -> std::collections::HashMap<ViolationCategory, f64> {
        let mut scores = std::collections::HashMap::new();
        let m = match meta {
            Some(m) => m,
            None => return scores,
        };

        // Clickbait -> spam / misinformation
        if m.clickbait_score > 0.3 {
            let entry = scores.entry(ViolationCategory::SpamDeceptive).or_insert(0.0);
            *entry = (*entry + m.clickbait_score).min(1.0);
        }

        // Misleading content
        if m.misleading_score > 0.3 {
            let entry = scores.entry(ViolationCategory::Misinformation).or_insert(0.0);
            *entry = (*entry + m.misleading_score).min(1.0);
        }

        // Keyword stuffing -> spam
        if m.keyword_stuffing_score > 0.5 {
            let entry = scores.entry(ViolationCategory::SpamDeceptive).or_insert(0.0);
            *entry = (*entry + m.keyword_stuffing_score * 0.6).min(1.0);
        }

        scores
    }

    // -----------------------------------------------------------------------
    // Strike / demonetization / copyright raw score computation
    // -----------------------------------------------------------------------

    fn compute_strike_raw(
        &self,
        category_scores: &std::collections::HashMap<ViolationCategory, f64>,
    ) -> f64 {
        let mut total = 0.0;
        for (cat, score) in category_scores {
            let weight = self.strike_category_weights.get(cat).copied().unwrap_or(0.5);
            total += score * weight;
        }
        total.min(1.0)
    }

    fn compute_demonetization_raw(
        &self,
        category_scores: &std::collections::HashMap<ViolationCategory, f64>,
    ) -> f64 {
        let mut total = 0.0;
        for (cat, score) in category_scores {
            let weight = self.demon_category_weights.get(cat).copied().unwrap_or(0.5);
            total += score * weight;
        }
        total.min(1.0)
    }

    fn compute_copyright_raw(&self, audio: Option<&AudioAnalysis>) -> f64 {
        let a = match audio {
            Some(a) => a,
            None => return 0.0,
        };
        let max_conf: f64 = a
            .copyright_matches
            .iter()
            .map(|c| c.match_confidence)
            .fold(0.0, f64::max);
        // Scale to [0, 1]
        max_conf
    }
}

// ---------------------------------------------------------------------------
// Helper: map audio event types to violation categories
// ---------------------------------------------------------------------------

fn audio_event_to_category(event_type: &str) -> ViolationCategory {
    let et = event_type.to_lowercase();
    match et.as_str() {
        "gunshot" | "explosion" | "scream" | "fight" => ViolationCategory::Violence,
        "profanity" | "slur" | "hate_speech" => ViolationCategory::HateSpeech,
        "harassment" | "threat" => ViolationCategory::Harassment,
        "explicit_lyrics" | "sexual_content" => ViolationCategory::AdultContent,
        "dangerous_act" | "self_harm" => ViolationCategory::HarmfulDangerous,
        "misinformation" | "conspiracy" => ViolationCategory::Misinformation,
        "child_endangerment" => ViolationCategory::ChildSafety,
        "copyright_music" | "copyright_video" => ViolationCategory::Copyright,
        _ => ViolationCategory::Unspecified,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::proto::{
        AnalysisResult, AudioAnalysis, BoundingBox, ContextType, DetectedObject, FlashingSegment,
        MetadataAnalysis, Timestamp, VisualAnalysis, ViolationCategory,
    };

    fn make_test_analysis() -> AnalysisResult {
        AnalysisResult {
            job_id: uuid::Uuid::new_v4(),
            source: sentinel_core::proto::VideoSource::new("test", "/tmp/test.mp4"),
            visual: Some(VisualAnalysis {
                objects: vec![
                    DetectedObject::new("weapon", 0.92)
                        .with_category(ViolationCategory::Violence)
                        .with_context(ContextType::Gratuitous)
                        .at_timestamp(Timestamp::new(10.0, 300))
                        .with_bbox(BoundingBox::new(0.2, 0.2, 0.8, 0.8)),
                ],
                texts: vec![],
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
                        text: "Negative hateful speech".into(),
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
                flagged_keywords: vec!["scam".into()],
                suggestions: vec![],
                overall_risk_score: 62.0,
            }),
            final_score: 56.0,
            completed_at: None,
        }
    }

    #[test]
    fn platt_scaling_calibrate_midpoint() {
        let platt = PlattScaling::new(-4.0, 1.0);
        let p = platt.calibrate(0.5);
        assert!(p > 0.0 && p < 1.0);
    }

    #[test]
    fn platt_scaling_calibrate_zero() {
        let platt = PlattScaling::default();
        assert_eq!(platt.calibrate(0.0), 0.0);
    }

    #[test]
    fn platt_scaling_calibrate_one() {
        let platt = PlattScaling::default();
        assert_eq!(platt.calibrate(1.0), 1.0);
    }

    #[test]
    fn platt_scaling_calibrate_batch() {
        let platt = PlattScaling::new(-4.0, 1.0);
        let scores = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        let calibrated = platt.calibrate_batch(&scores);
        assert_eq!(calibrated.len(), 5);
        // Monotonicity check
        for i in 1..calibrated.len() {
            if scores[i] > scores[i - 1] {
                assert!(
                    calibrated[i] >= calibrated[i - 1],
                    "Platt scaling should be monotonic"
                );
            }
        }
    }

    #[test]
    fn category_interaction_matrix_default() {
        let m = CategoryInteractionMatrix::new();
        // No interaction between unrelated categories
        assert_eq!(
            m.get_multiplier(&ViolationCategory::Copyright, &ViolationCategory::HateSpeech),
            1.0
        );
        // Violence + ChildSafety should be amplified
        assert!(
            m.get_multiplier(&ViolationCategory::Violence, &ViolationCategory::ChildSafety) > 1.5
        );
    }

    #[test]
    fn category_interaction_symmetry() {
        let m = CategoryInteractionMatrix::new();
        assert_eq!(
            m.get_multiplier(&ViolationCategory::Violence, &ViolationCategory::HateSpeech),
            m.get_multiplier(&ViolationCategory::HateSpeech, &ViolationCategory::Violence)
        );
    }

    #[test]
    fn category_interaction_apply() {
        let m = CategoryInteractionMatrix::new();
        let mut scores = std::collections::HashMap::new();
        scores.insert(ViolationCategory::Violence, 0.8);
        scores.insert(ViolationCategory::HateSpeech, 0.6);
        let mult = m.apply_interactions(&scores);
        assert!(mult > 1.0); // Should be amplified
    }

    #[test]
    fn category_interaction_apply_single_category() {
        let m = CategoryInteractionMatrix::new();
        let mut scores = std::collections::HashMap::new();
        scores.insert(ViolationCategory::Violence, 0.8);
        let mult = m.apply_interactions(&scores);
        assert!((mult - 1.0).abs() < f64::EPSILON); // No interactions
    }

    #[test]
    fn category_interaction_apply_empty() {
        let m = CategoryInteractionMatrix::new();
        let scores: std::collections::HashMap<ViolationCategory, f64> =
            std::collections::HashMap::new();
        let mult = m.apply_interactions(&scores);
        assert!((mult - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ml_scorer_new() {
        let scorer = MlRiskScorer::new();
        assert!(scorer.visual_weight > 0.0);
        assert!(scorer.audio_weight > 0.0);
        assert!(scorer.metadata_weight > 0.0);
    }

    #[test]
    fn ml_scorer_with_weights() {
        let scorer = MlRiskScorer::new().with_weights(1.0, 1.0, 1.0);
        let total = scorer.visual_weight + scorer.audio_weight + scorer.metadata_weight;
        assert!((total - 1.0).abs() < 0.001);
    }

    #[test]
    fn ml_scorer_score_returns_results() {
        let scorer = MlRiskScorer::new();
        let analysis = make_test_analysis();
        let scores = scorer.score(&analysis);
        assert!(scores.overall_risk >= 0.0 && scores.overall_risk <= 1.0);
    }

    #[test]
    fn ml_scorer_strike_probability() {
        let scorer = MlRiskScorer::new();
        let analysis = make_test_analysis();
        let scores = scorer.score(&analysis);
        assert!(
            scores.strike_probability >= 0.0 && scores.strike_probability <= 1.0,
            "Strike probability should be in [0, 1]"
        );
    }

    #[test]
    fn ml_scorer_demonetization_probability() {
        let scorer = MlRiskScorer::new();
        let analysis = make_test_analysis();
        let scores = scorer.score(&analysis);
        assert!(
            scores.demonetization_probability >= 0.0 && scores.demonetization_probability <= 1.0
        );
    }

    #[test]
    fn ml_scorer_copyright_probability() {
        let scorer = MlRiskScorer::new();
        let analysis = make_test_analysis();
        let scores = scorer.score(&analysis);
        assert!(
            scores.copyright_probability >= 0.0 && scores.copyright_probability <= 1.0
        );
    }

    #[test]
    fn ml_scorer_category_scores_populated() {
        let scorer = MlRiskScorer::new();
        let analysis = make_test_analysis();
        let scores = scorer.score(&analysis);
        assert!(!scores.category_scores.is_empty());
    }

    #[test]
    fn ml_scorer_highest_risk_category() {
        let scorer = MlRiskScorer::new();
        let analysis = make_test_analysis();
        let scores = scorer.score(&analysis);
        let highest = scores.highest_risk_category();
        assert!(highest.is_some());
    }

    #[test]
    fn ml_scorer_exceeds_threshold() {
        let mut scores = RiskScores::default();
        scores.overall_risk = 0.7;
        assert!(scores.exceeds_threshold(0.5));
        assert!(!scores.exceeds_threshold(0.9));
    }

    #[test]
    fn ml_scorer_score_empty_analysis() {
        let scorer = MlRiskScorer::new();
        let analysis = AnalysisResult {
            job_id: uuid::Uuid::new_v4(),
            source: sentinel_core::proto::VideoSource::new("empty", "/tmp/empty.mp4"),
            visual: None,
            audio: None,
            metadata: None,
            final_score: 0.0,
            completed_at: None,
        };
        let scores = scorer.score(&analysis);
        assert_eq!(scores.overall_risk, 0.0);
        assert_eq!(scores.strike_probability, 0.0);
        assert_eq!(scores.demonetization_probability, 0.0);
        assert_eq!(scores.copyright_probability, 0.0);
    }

    #[test]
    fn ml_scorer_score_no_copyright() {
        let scorer = MlRiskScorer::new();
        let mut analysis = make_test_analysis();
        analysis.audio = Some(AudioAnalysis {
            transcript: vec![],
            events: vec![],
            copyright_matches: vec![],
            overall_risk_score: 10.0,
        });
        let scores = scorer.score(&analysis);
        assert_eq!(scores.copyright_probability, 0.0);
    }

    #[test]
    fn risk_scores_clamp() {
        let mut scores = RiskScores {
            strike_probability: 1.5,
            demonetization_probability: -0.3,
            copyright_probability: 2.0,
            overall_risk: 0.5,
            category_scores: std::collections::HashMap::new(),
            explanation: vec![],
        };
        scores.clamp();
        assert_eq!(scores.strike_probability, 1.0);
        assert_eq!(scores.demonetization_probability, 0.0);
        assert_eq!(scores.copyright_probability, 1.0);
    }

    #[test]
    fn risk_scores_summary() {
        let scores = RiskScores {
            strike_probability: 0.5,
            demonetization_probability: 0.3,
            copyright_probability: 0.1,
            overall_risk: 0.4,
            category_scores: {
                let mut m = std::collections::HashMap::new();
                m.insert(ViolationCategory::Violence, 0.6);
                m
            },
            explanation: vec!["Test explanation".into()],
        };
        let summary = scores.summary();
        assert!(summary.contains("50.0%"));
        assert!(summary.contains("Violence"));
        assert!(summary.contains("Test explanation"));
    }

    #[test]
    fn risk_scores_highest_risk_category_empty() {
        let scores = RiskScores::default();
        assert!(scores.highest_risk_category().is_none());
    }

    #[test]
    fn audio_event_category_mapping() {
        assert_eq!(
            audio_event_to_category("gunshot"),
            ViolationCategory::Violence
        );
        assert_eq!(
            audio_event_to_category("slur"),
            ViolationCategory::HateSpeech
        );
        assert_eq!(
            audio_event_to_category("copyright_music"),
            ViolationCategory::Copyright
        );
        assert_eq!(
            audio_event_to_category("unknown_event"),
            ViolationCategory::Unspecified
        );
    }

    #[test]
    fn platt_scaling_steeper_slope() {
        let steep = PlattScaling::new(-10.0, 0.0);
        let shallow = PlattScaling::new(-1.0, 0.0);
        let mid = 0.5;
        // Steeper slope should produce more extreme probabilities at same input
        let steep_p = steep.calibrate(mid);
        let shallow_p = shallow.calibrate(mid);
        // At 0.5 with negative A, steeper should give lower probability
        assert!(steep_p < shallow_p);
    }
}
