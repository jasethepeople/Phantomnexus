//! Synthesis engine — contextual AI synthesis and multi-modal data fusion.
//!
//! Orchestrates a 6-stage synthesis pipeline:
//!
//! 1. **Extract content elements** — normalise visual, audio, and metadata
//!    into unified `ContentElement`s.
//! 2. **Classify primary context** — score against 5 indicator lexicons.
//! 3. **Detect contradictions** — find cross-modal inconsistencies.
//! 4. **Resolve cross-modal conflicts** — apply resolution strategies.
//! 5. **Temporal correlation** — detect escalation, repetition, etc.
//! 6. **Generate recommendation** — produce final `ContentRecommendation`.

use crate::content_elements::{ContentElement, TextSource};
use crate::context_classifier::{ContextClassification, ContextClassifier, ContextType};
use crate::cross_modal_fusion::{ContradictionResolution, CrossModalFusion, ModalContradiction};
use crate::temporal_correlator::{TemporalCorrelator, TemporalCorrelation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

// ---------------------------------------------------------------------------
// SynthesisError
// ---------------------------------------------------------------------------

/// Errors that can occur during the synthesis pipeline.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SynthesisError {
    #[error("synthesis failed: {0}")]
    General(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("classification failed: {0}")]
    ClassificationFailed(String),
}

/// Shorthand result type for synthesis operations.
pub type Result<T> = std::result::Result<T, SynthesisError>;

// ---------------------------------------------------------------------------
// ContentRecommendation — final recommendation enum
// ---------------------------------------------------------------------------

/// The final content moderation recommendation produced by the synthesis
/// engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentRecommendation {
    /// Content is safe for all audiences — approve for monetization.
    Safe,
    /// Content is mostly safe but has minor concerns — monetize with
    /// limited or no ads in sensitive categories.
    MonetizableWithCaution,
    /// Content violates policies — demonetize (no ads).
    Demonetized,
    /// Content poses a high strike risk — may result in channel strike.
    StrikeRisk,
    /// Content is ambiguous or borderline — requires human review.
    RequiresHumanReview,
}

impl ContentRecommendation {
    /// Human-readable label for the recommendation.
    pub fn label(&self) -> &'static str {
        match self {
            ContentRecommendation::Safe => "safe",
            ContentRecommendation::MonetizableWithCaution => "monetizable_with_caution",
            ContentRecommendation::Demonetized => "demonetized",
            ContentRecommendation::StrikeRisk => "strike_risk",
            ContentRecommendation::RequiresHumanReview => "requires_human_review",
        }
    }

    /// Numeric severity score (0..100) for the recommendation.
    pub fn severity_score(&self) -> f64 {
        match self {
            ContentRecommendation::Safe => 0.0,
            ContentRecommendation::MonetizableWithCaution => 25.0,
            ContentRecommendation::Demonetized => 60.0,
            ContentRecommendation::StrikeRisk => 90.0,
            ContentRecommendation::RequiresHumanReview => 50.0,
        }
    }

    /// Whether this recommendation allows monetization in any form.
    pub fn allows_monetization(&self) -> bool {
        matches!(
            self,
            ContentRecommendation::Safe | ContentRecommendation::MonetizableWithCaution
        )
    }

    /// Whether this recommendation requires human intervention.
    pub fn requires_human(&self) -> bool {
        matches!(self, ContentRecommendation::RequiresHumanReview)
    }
}

// ---------------------------------------------------------------------------
// ContextAssessment — a scored context finding
// ---------------------------------------------------------------------------

/// An individual context assessment attached to a content segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextAssessment {
    pub context_type: ContextType,
    pub confidence: f64,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub supporting_evidence: Vec<String>,
}

impl ContextAssessment {
    pub fn new(context_type: ContextType, confidence: f64, start_seconds: f64, end_seconds: f64) -> Self {
        ContextAssessment {
            context_type,
            confidence: confidence.clamp(0.0, 1.0),
            start_seconds,
            end_seconds,
            supporting_evidence: Vec::new(),
        }
    }

    pub fn with_evidence(mut self, evidence: Vec<String>) -> Self {
        self.supporting_evidence = evidence;
        self
    }
}

// ---------------------------------------------------------------------------
// SarcasmDetectionResult
// ---------------------------------------------------------------------------

/// Result of sarcasm / satire detection analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SarcasmDetectionResult {
    /// Overall sarcasm probability (0.0..1.0).
    pub sarcasm_score: f64,
    /// Which detection methods fired.
    pub matched_patterns: Vec<String>,
    /// Visual-audio incongruity score.
    pub incongruity_score: f64,
    /// Exaggeration score.
    pub exaggeration_score: f64,
}

// ---------------------------------------------------------------------------
// SynthesisResult — the final output
// ---------------------------------------------------------------------------

/// Complete result of the synthesis pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthesisResult {
    /// Unified content elements extracted from all modalities.
    pub elements: Vec<ContentElement>,
    /// Primary context classification.
    pub context_classification: ContextClassification,
    /// Temporal correlations detected.
    pub temporal_correlations: Vec<TemporalCorrelation>,
    /// Cross-modal contradictions detected.
    pub contradictions: Vec<ModalContradiction>,
    /// Context assessments per time segment.
    pub context_assessments: Vec<ContextAssessment>,
    /// Final aggregated risk score (0.0..100.0).
    pub total_risk_score: f64,
    /// The content recommendation.
    pub recommendation: ContentRecommendation,
    /// Sarcasm detection result.
    pub sarcasm_result: Option<SarcasmDetectionResult>,
    /// Educational value score (0.0..1.0).
    pub educational_value: f64,
    /// Artistic merit score (0.0..1.0).
    pub artistic_merit: f64,
    /// Stage-by-stage processing metrics.
    pub stage_metrics: HashMap<String, f64>,
}

impl SynthesisResult {
    pub fn empty() -> Self {
        SynthesisResult {
            elements: Vec::new(),
            context_classification: ContextClassification::unknown(),
            temporal_correlations: Vec::new(),
            contradictions: Vec::new(),
            context_assessments: Vec::new(),
            total_risk_score: 0.0,
            recommendation: ContentRecommendation::Safe,
            sarcasm_result: None,
            educational_value: 0.0,
            artistic_merit: 0.0,
            stage_metrics: HashMap::new(),
        }
    }

    /// Whether the result requires human review.
    pub fn needs_human_review(&self) -> bool {
        self.recommendation.requires_human()
            || self.total_risk_score >= 40.0 && self.total_risk_score < 70.0
    }
}

// ---------------------------------------------------------------------------
// SynthesisEngine
// ---------------------------------------------------------------------------

/// The central synthesis engine that orchestrates multi-modal content
/// understanding.
///
/// Runs a 6-stage synthesis pipeline over visual, audio, and metadata
/// analysis results to produce a unified risk assessment and content
/// recommendation.
#[derive(Debug, Clone)]
pub struct SynthesisEngine {
    context_classifier: ContextClassifier,
    temporal_correlator: TemporalCorrelator,
    cross_modal_fusion: CrossModalFusion,
}

impl Default for SynthesisEngine {
    fn default() -> Self {
        SynthesisEngine {
            context_classifier: ContextClassifier::new(),
            temporal_correlator: TemporalCorrelator::new(),
            cross_modal_fusion: CrossModalFusion::new(),
        }
    }
}

impl SynthesisEngine {
    /// Create a new synthesis engine with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new synthesis engine with custom sub-components.
    pub fn with_components(
        classifier: ContextClassifier,
        correlator: TemporalCorrelator,
        fusion: CrossModalFusion,
    ) -> Self {
        SynthesisEngine {
            context_classifier: classifier,
            temporal_correlator: correlator,
            cross_modal_fusion: fusion,
        }
    }

    // ==================================================================
    // Main synthesis pipeline (6 stages)
    // ==================================================================

    /// Run the full 6-stage synthesis pipeline.
    ///
    /// # Arguments
    /// * `visual` — visual analysis results (objects, OCR, flashing).
    /// * `audio` — audio analysis results (transcript, events, copyright).
    /// * `metadata` — metadata analysis results (clickbait, keywords).
    ///
    /// # Returns
    /// A `SynthesisResult` containing the fused assessment and
    /// recommendation.
    #[instrument(skip(self, visual, audio, metadata))]
    pub fn synthesize(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
        metadata: &sentinel_core::proto::MetadataAnalysis,
    ) -> Result<SynthesisResult> {
        let mut result = SynthesisResult::empty();
        let mut stage_metrics: HashMap<String, f64> = HashMap::new();

        info!("starting 6-stage synthesis pipeline");

        // ------------------------------------------------------------------
        // Stage 1: Extract content elements
        // ------------------------------------------------------------------
        debug!("stage 1: extracting content elements");
        let elements = self.extract_content_elements(visual, audio, metadata);
        stage_metrics.insert("stage1_elements_extracted".into(), elements.len() as f64);
        result.elements = elements;

        // ------------------------------------------------------------------
        // Stage 2: Classify primary context
        // ------------------------------------------------------------------
        debug!("stage 2: classifying context");
        let context_classification = self.classify_context(&result.elements);
        stage_metrics.insert(
            "stage2_primary_context".into(),
            match context_classification.primary_context {
                ContextType::Educational => 1.0,
                ContextType::Satirical => 2.0,
                ContextType::Documentary => 3.0,
                ContextType::Artistic => 4.0,
                ContextType::News => 5.0,
                ContextType::Unknown => 0.0,
            },
        );
        stage_metrics.insert("stage2_confidence".into(), context_classification.confidence);
        result.context_classification = context_classification.clone();

        // Build context assessments from secondary contexts.
        let mut assessments: Vec<ContextAssessment> = context_classification
            .secondary_contexts
            .iter()
            .map(|(ctx, conf)| {
                ContextAssessment::new(*ctx, *conf, 0.0, 0.0)
            })
            .collect();
        assessments.push(ContextAssessment::new(
            context_classification.primary_context,
            context_classification.confidence,
            0.0,
            0.0,
        ));
        result.context_assessments = assessments;

        // ------------------------------------------------------------------
        // Stage 3: Detect contradictions
        // ------------------------------------------------------------------
        debug!("stage 3: detecting cross-modal contradictions");
        let fused = self.cross_modal_fusion.fuse(visual, audio);
        let contradictions = fused.contradictions;
        stage_metrics.insert(
            "stage3_contradictions".into(),
            contradictions.len() as f64,
        );
        stage_metrics.insert(
            "stage3_artistic_resolutions".into(),
            fused.artistic_resolutions as f64,
        );
        stage_metrics.insert(
            "stage3_review_required".into(),
            fused.review_required_count as f64,
        );
        result.contradictions = contradictions.clone();
        result.elements = fused.elements;

        // ------------------------------------------------------------------
        // Stage 4: Resolve cross-modal conflicts
        // ------------------------------------------------------------------
        debug!("stage 4: resolving cross-modal conflicts");
        let resolved_risk: f64 = contradictions
            .iter()
            .map(|c| c.resolved_risk)
            .sum();
        stage_metrics.insert("stage4_resolved_risk".into(), resolved_risk);

        // ------------------------------------------------------------------
        // Stage 5: Temporal correlation
        // ------------------------------------------------------------------
        debug!("stage 5: temporal correlation");
        let temporal_correlations = self.temporal_correlator.correlate_events(visual, audio);
        stage_metrics.insert(
            "stage5_temporal_patterns".into(),
            temporal_correlations.len() as f64,
        );
        let temporal_risk: f64 = temporal_correlations
            .iter()
            .map(|tc| tc.risk_score)
            .sum();
        stage_metrics.insert("stage5_temporal_risk".into(), temporal_risk);
        result.temporal_correlations = temporal_correlations;

        // ------------------------------------------------------------------
        // Stage 6: Generate recommendation
        // ------------------------------------------------------------------
        debug!("stage 6: generating recommendation");

        // Compute sarcasm score.
        let sarcasm_result = if !audio.transcript.is_empty() {
            let score = self.detect_sarcasm(&audio.transcript, visual);
            let matched = self.find_sarcasm_patterns(&audio.transcript);
            Some(SarcasmDetectionResult {
                sarcasm_score: score,
                matched_patterns: matched,
                incongruity_score: self.compute_visual_audio_incongruity(visual, audio),
                exaggeration_score: self.detect_exaggeration(&audio.transcript),
            })
        } else {
            None
        };
        result.sarcasm_result = sarcasm_result.clone();
        stage_metrics.insert(
            "stage6_sarcasm_score".into(),
            sarcasm_result.as_ref().map_or(0.0, |s| s.sarcasm_score),
        );

        // Compute educational value.
        let educational_value = self.assess_educational_value(&result.elements);
        result.educational_value = educational_value;
        stage_metrics.insert("stage6_educational_value".into(), educational_value);

        // Compute artistic merit.
        let artistic_merit = self.evaluate_artistic_merit(visual, audio);
        result.artistic_merit = artistic_merit;
        stage_metrics.insert("stage6_artistic_merit".into(), artistic_merit);

        // Aggregate total risk.
        let base_risk = visual.overall_risk_score
            + audio.overall_risk_score
            + metadata.overall_risk_score;
        let adjusted_risk = base_risk
            + resolved_risk
            + temporal_risk
            - educational_value * 5.0  // Educational content reduces risk
            - artistic_merit * 3.0;     // Artistic merit slightly reduces risk

        // Apply sarcasm modifier (satirical content reduces literal risk).
        let sarcasm_modifier = sarcasm_result.as_ref().map_or(1.0, |s| {
            if s.sarcasm_score > 0.6 {
                0.7 // Significant reduction for clear satire
            } else if s.sarcasm_score > 0.3 {
                0.85 // Moderate reduction
            } else {
                1.0
            }
        });

        // Apply context risk reduction.
        let context_reduction = context_classification.apply_risk_reduction(1.0);

        let total_risk = (adjusted_risk * sarcasm_modifier * context_reduction)
            .clamp(0.0, 100.0);
        result.total_risk_score = total_risk;
        stage_metrics.insert("stage6_total_risk".into(), total_risk);

        // Generate recommendation.
        let recommendation = self.generate_recommendation(
            total_risk,
            &result.context_assessments,
            &result.contradictions,
        );
        result.recommendation = recommendation;
        stage_metrics.insert(
            "stage6_recommendation".into(),
            recommendation.severity_score(),
        );

        result.stage_metrics = stage_metrics;

        info!(
            "synthesis complete: total_risk={:.2}, recommendation={}",
            total_risk,
            recommendation.label()
        );

        Ok(result)
    }

    // ==================================================================
    // Stage 1: Extract content elements
    // ==================================================================

    /// Extract unified content elements from all three analysis modalities.
    fn extract_content_elements(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
        metadata: &sentinel_core::proto::MetadataAnalysis,
    ) -> Vec<ContentElement> {
        let mut elements = Vec::new();

        // Visual objects.
        for obj in &visual.objects {
            let element = ContentElement::visual(
                &obj.label,
                obj.confidence,
                crate::content_elements::Timestamp::new(
                    obj.timestamp.seconds,
                    obj.timestamp.frame_number,
                ),
                crate::content_elements::BoundingBox::new(
                    obj.bbox.x1, obj.bbox.y1, obj.bbox.x2, obj.bbox.y2,
                ),
            );
            elements.push(element);
        }

        // Visual text (OCR).
        for text in &visual.texts {
            let source = if text.is_obscured {
                TextSource::Embedded
            } else {
                TextSource::Ocr
            };
            elements.push(ContentElement::text(&text.text, source, text.confidence));
        }

        // Audio events.
        for evt in &audio.events {
            elements.push(ContentElement::audio(
                &evt.event_type,
                evt.confidence,
                crate::content_elements::Timestamp::new(
                    evt.timestamp.seconds,
                    evt.timestamp.frame_number,
                ),
                None,
            ));
        }

        // Transcript segments.
        for seg in &audio.transcript {
            elements.push(ContentElement::audio(
                "speech",
                seg.confidence,
                crate::content_elements::Timestamp::new(
                    seg.start.seconds,
                    seg.start.frame_number,
                ),
                Some(seg.text.clone()),
            ));
        }

        // Metadata.
        elements.push(ContentElement::metadata(
            "clickbait_score",
            format!("{:.4}", metadata.clickbait_score),
        ));
        elements.push(ContentElement::metadata(
            "keyword_stuffing_score",
            format!("{:.4}", metadata.keyword_stuffing_score),
        ));
        elements.push(ContentElement::metadata(
            "misleading_score",
            format!("{:.4}", metadata.misleading_score),
        ));

        for kw in &metadata.flagged_keywords {
            elements.push(ContentElement::metadata("flagged_keyword", kw.clone()));
        }

        elements
    }

    // ==================================================================
    // Stage 2: Classify context
    // ==================================================================

    /// Classify the primary context of the content by scoring elements
    /// against five indicator lexicons (educational, satirical,
    /// documentary, artistic, news).
    ///
    /// Each lexicon has 50+ weighted patterns.  Returns the primary and
    /// secondary contexts with confidence scores.
    pub fn classify_context(
        &self,
        elements: &[ContentElement],
    ) -> ContextClassification {
        self.context_classifier.classify(elements)
    }

    // ==================================================================
    // Stage 3+4: Sarcasm detection
    // ==================================================================

    /// Detect sarcasm in transcript segments using 18 language patterns,
    /// exaggeration detection, and visual-audio incongruity scoring.
    ///
    /// Returns a score in `[0.0, 1.0]` where higher values indicate
    /// stronger sarcasm / satirical intent.
    pub fn detect_sarcasm(
        &self,
        transcript: &[sentinel_core::proto::TranscriptSegment],
        visual: &sentinel_core::proto::VisualAnalysis,
    ) -> f64 {
        if transcript.is_empty() {
            return 0.0;
        }

        let mut total_sarcasm_score = 0.0;
        let mut total_confidence = 0.0;

        for seg in transcript {
            let text_lower = seg.text.to_lowercase();
            let mut seg_score = 0.0;

            // Check 18 sarcasm language patterns.
            let patterns = self.sarcasm_language_patterns();
            for (pattern, weight) in patterns {
                if text_lower.contains(pattern) {
                    seg_score += weight;
                }
            }

            // Check for exaggeration.
            let exaggeration = self.detect_exaggeration_in_text(&seg.text);
            seg_score += exaggeration * 0.3;

            total_sarcasm_score += seg_score * seg.confidence;
            total_confidence += seg.confidence;
        }

        let language_score = if total_confidence > 0.0 {
            (total_sarcasm_score / total_confidence).min(1.0)
        } else {
            0.0
        };

        // Visual-audio incongruity.
        let incongruity = self.compute_visual_audio_incongruity(visual, &sentinel_core::proto::AudioAnalysis {
            transcript: transcript.to_vec(),
            events: Vec::new(),
            copyright_matches: Vec::new(),
            overall_risk_score: 0.0,
        });

        // Combine scores: language patterns are primary, incongruity boosts.
        let combined = language_score * 0.7 + incongruity * 0.3;
        combined.min(1.0)
    }

    /// Return the 18 sarcasm language patterns with their weights.
    fn sarcasm_language_patterns(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("yeah right", 0.9),
            ("sure", 0.5),
            ("obviously", 0.6),
            ("clearly", 0.55),
            ("totally", 0.5),
            ("definitely", 0.5),
            ("of course", 0.6),
            ("because that makes sense", 0.95),
            ("what a surprise", 0.9),
            ("shocking", 0.7),
            ("obviously", 0.55),
            ("as if", 0.85),
            ("like i care", 0.9),
            ("tell me about it", 0.6),
            ("oh great", 0.8),
            ("just what i needed", 0.85),
            ("couldn't be better", 0.85),
            ("perfect", 0.4),
        ]
    }

    /// Find which sarcasm patterns matched in the transcript.
    fn find_sarcasm_patterns(
        &self,
        transcript: &[sentinel_core::proto::TranscriptSegment],
    ) -> Vec<String> {
        let patterns = self.sarcasm_language_patterns();
        let mut matched = Vec::new();

        for seg in transcript {
            let lower = seg.text.to_lowercase();
            for (pattern, _) in &patterns {
                if lower.contains(pattern) && !matched.contains(&pattern.to_string()) {
                    matched.push(pattern.to_string());
                }
            }
        }

        matched
    }

    /// Detect exaggeration in a transcript segment.
    fn detect_exaggeration_in_text(&self, text: &str) -> f64 {
        let lower = text.to_lowercase();
        let mut score = 0.0;

        // Exaggeration indicators.
        let exaggeration_words = [
            ("literally", 0.3),
            ("absolutely", 0.2),
            ("completely", 0.2),
            ("totally", 0.2),
            ("never", 0.15),
            ("always", 0.15),
            ("everyone", 0.15),
            ("nobody", 0.15),
            ("impossible", 0.25),
            ("unbelievable", 0.3),
            ("incredible", 0.25),
            ("amazing", 0.2),
            ("worst", 0.2),
            ("best", 0.15),
            ("million", 0.1),
            ("billion", 0.1),
        ];

        for (word, weight) in &exaggeration_words {
            if lower.contains(word) {
                score += weight;
            }
        }

        // Repeated punctuation suggests exaggeration.
        let exclamation_count = lower.matches('!').count();
        score += (exclamation_count as f64 * 0.1).min(0.3);

        // ALL CAPS words.
        let caps_words: Vec<&str> = text
            .split_whitespace()
            .filter(|w| w.len() > 2 && w.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()))
            .collect();
        score += (caps_words.len() as f64 * 0.15).min(0.4);

        score.min(1.0)
    }

    /// Detect exaggeration across all transcript segments.
    pub fn detect_exaggeration(
        &self,
        transcript: &[sentinel_core::proto::TranscriptSegment],
    ) -> f64 {
        if transcript.is_empty() {
            return 0.0;
        }

        let total: f64 = transcript
            .iter()
            .map(|seg| self.detect_exaggeration_in_text(&seg.text) * seg.confidence)
            .sum();
        let total_confidence: f64 = transcript.iter().map(|seg| seg.confidence).sum();

        if total_confidence > 0.0 {
            (total / total_confidence).min(1.0)
        } else {
            0.0
        }
    }

    /// Compute visual-audio incongruity score.
    ///
    /// High incongruity (e.g. violent visual + calm transcript) suggests
    /// satirical intent.
    fn compute_visual_audio_incongruity(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> f64 {
        let visual_risk = visual.overall_risk_score;

        // Estimate audio sentiment risk.
        let audio_sentiment_risk: f64 = audio
            .transcript
            .iter()
            .map(|seg| {
                let sentiment_abs = seg.sentiment.abs();
                if seg.is_sarcasm {
                    sentiment_abs * 1.5
                } else {
                    sentiment_abs
                }
            })
            .sum::<f64>()
            / (audio.transcript.len().max(1) as f64);

        // Incongruity is high when visual risk and audio sentiment diverge.
        let divergence = (visual_risk - audio_sentiment_risk * 10.0).abs();
        (divergence / 50.0).min(1.0)
    }

    // ==================================================================
    // Stage 6: Educational value assessment
    // ==================================================================

    /// Assess the educational value of content elements using 25
    /// educational keywords across all modalities.
    ///
    /// Returns a score in `[0.0, 1.0]`.
    pub fn assess_educational_value(&self, elements: &[ContentElement]) -> f64 {
        if elements.is_empty() {
            return 0.0;
        }

        let edu_keywords: [(&str, f64); 25] = [
            ("tutorial", 0.9),
            ("lesson", 0.9),
            ("learn", 0.85),
            ("teach", 0.85),
            ("explain", 0.8),
            ("education", 0.9),
            ("educational", 0.9),
            ("course", 0.8),
            ("lecture", 0.85),
            ("study", 0.75),
            ("science", 0.8),
            ("mathematics", 0.85),
            ("physics", 0.85),
            ("programming", 0.8),
            ("how to", 0.75),
            ("step by step", 0.75),
            ("demonstration", 0.8),
            ("example", 0.6),
            ("exercise", 0.7),
            ("homework", 0.7),
            ("quiz", 0.65),
            ("curriculum", 0.85),
            ("knowledge", 0.7),
            ("understand", 0.65),
            ("concept", 0.6),
        ];

        let mut total_score = 0.0;
        let mut total_weight = 0.0;

        for elem in elements {
            let text = elem.to_display_string().to_lowercase();
            let mut elem_score = 0.0;

            for (keyword, weight) in &edu_keywords {
                if text.contains(keyword) {
                    elem_score += weight;
                }
            }

            let conf = elem.confidence();
            total_score += elem_score * conf;
            total_weight += conf;
        }

        if total_weight > 0.0 {
            (total_score / total_weight).min(1.0)
        } else {
            0.0
        }
    }

    // ==================================================================
    // Stage 6: Artistic merit evaluation
    // ==================================================================

    /// Evaluate the artistic merit of visual and audio content.
    ///
    /// Scores based on cinematic quality indicators, visual complexity,
    /// audio production quality, and creative elements.
    ///
    /// Returns a score in `[0.0, 1.0]`.
    pub fn evaluate_artistic_merit(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> f64 {
        let mut score = 0.0;
        let mut factor_count = 0.0;

        // Factor 1: Visual variety (number of unique object labels).
        let unique_labels: std::collections::HashSet<&str> = visual
            .objects
            .iter()
            .map(|o| o.label.as_str())
            .collect();
        let variety_score = (unique_labels.len() as f64 / 10.0).min(1.0);
        score += variety_score * 0.2;
        factor_count += 0.2;

        // Factor 2: Scene transitions indicate production quality.
        let transition_score = if visual.transitions.len() >= 3 {
            0.7 + (visual.transitions.len() as f64 / 20.0).min(0.3)
        } else if visual.transitions.len() >= 1 {
            0.4
        } else {
            0.1
        };
        score += transition_score * 0.2;
        factor_count += 0.2;

        // Factor 3: Text / OCR density (suggests titles, captions).
        let text_score = if visual.texts.len() >= 5 {
            0.8
        } else if visual.texts.len() >= 2 {
            0.5
        } else {
            0.2
        };
        score += text_score * 0.15;
        factor_count += 0.15;

        // Factor 4: Audio variety (event types).
        let unique_audio: std::collections::HashSet<&str> = audio
            .events
            .iter()
            .map(|e| e.event_type.as_str())
            .collect();
        let audio_variety = (unique_audio.len() as f64 / 5.0).min(1.0);
        score += audio_variety * 0.15;
        factor_count += 0.15;

        // Factor 5: Copyright matches suggest professional music.
        let copyright_score = if !audio.copyright_matches.is_empty() {
            0.6 // Professional production often uses copyrighted music
        } else {
            0.2
        };
        score += copyright_score * 0.15;
        factor_count += 0.15;

        // Factor 6: Transcript length / quality.
        let transcript_quality = if audio.transcript.len() >= 10 {
            0.7
        } else if audio.transcript.len() >= 3 {
            0.4
        } else {
            0.1
        };
        score += transcript_quality * 0.15;
        factor_count += 0.15;

        if factor_count > 0.0 {
            (score / factor_count).min(1.0)
        } else {
            0.0
        }
    }

    // ==================================================================
    // Stage 6: Generate recommendation
    // ==================================================================

    /// Generate the final content recommendation based on total risk,
    /// context assessments, and detected contradictions.
    pub fn generate_recommendation(
        &self,
        total_risk: f64,
        contexts: &[ContextAssessment],
        contradictions: &[ModalContradiction],
    ) -> ContentRecommendation {
        // Count contradictions requiring review.
        let review_count = contradictions
            .iter()
            .filter(|c| c.resolution == ContradictionResolution::RequiresReview)
            .count();

        // Check for artistic context contradictions (reduces severity).
        let artistic_count = contradictions
            .iter()
            .filter(|c| c.resolution == ContradictionResolution::ArtisticContext)
            .count();

        // Check if educational or documentary context dominates.
        let has_educational_context = contexts.iter().any(|c| {
            matches!(c.context_type, ContextType::Educational | ContextType::Documentary)
                && c.confidence > 0.5
        });

        // Decision logic.
        if total_risk >= 80.0 {
            ContentRecommendation::StrikeRisk
        } else if total_risk >= 60.0 {
            if has_educational_context && artistic_count > 0 {
                ContentRecommendation::Demonetized
            } else {
                ContentRecommendation::StrikeRisk
            }
        } else if total_risk >= 45.0 {
            if review_count > 0 {
                ContentRecommendation::RequiresHumanReview
            } else {
                ContentRecommendation::Demonetized
            }
        } else if total_risk >= 30.0 {
            if review_count >= 2 {
                ContentRecommendation::RequiresHumanReview
            } else {
                ContentRecommendation::Demonetized
            }
        } else if total_risk >= 20.0 {
            if has_educational_context {
                ContentRecommendation::MonetizableWithCaution
            } else if review_count > 0 {
                ContentRecommendation::RequiresHumanReview
            } else {
                ContentRecommendation::MonetizableWithCaution
            }
        } else if total_risk >= 10.0 {
            ContentRecommendation::MonetizableWithCaution
        } else {
            ContentRecommendation::Safe
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::proto::{
        AudioAnalysis, AudioEvent, BoundingBox, DetectedObject, FlashingSegment, MetadataAnalysis,
        SceneTransition, Severity, Timestamp, TranscriptSegment, ViolationCategory, VisualAnalysis,
    };

    fn empty_visual() -> VisualAnalysis {
        VisualAnalysis::empty()
    }

    fn empty_audio() -> AudioAnalysis {
        AudioAnalysis::empty()
    }

    fn empty_metadata() -> MetadataAnalysis {
        MetadataAnalysis::empty()
    }

    // Helper: make a visual analysis with a violent object.
    fn violent_visual() -> VisualAnalysis {
        let mut va = empty_visual();
        va.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(5.0, 150)),
        );
        va.overall_risk_score = 15.0;
        va
    }

    // Helper: make an audio analysis with calm events.
    fn calm_audio() -> AudioAnalysis {
        let mut aa = empty_audio();
        aa.events.push(AudioEvent::new("potential_music", 0.8, Timestamp::new(5.1, 153)));
        aa.overall_risk_score = 5.0;
        aa
    }

    #[test]
    fn engine_new() {
        let engine = SynthesisEngine::new();
        assert_eq!(engine.cross_modal_fusion.min_confidence, 0.5);
    }

    #[test]
    fn content_recommendation_labels() {
        assert_eq!(ContentRecommendation::Safe.label(), "safe");
        assert_eq!(
            ContentRecommendation::MonetizableWithCaution.label(),
            "monetizable_with_caution"
        );
        assert_eq!(ContentRecommendation::Demonetized.label(), "demonetized");
        assert_eq!(ContentRecommendation::StrikeRisk.label(), "strike_risk");
        assert_eq!(
            ContentRecommendation::RequiresHumanReview.label(),
            "requires_human_review"
        );
    }

    #[test]
    fn content_recommendation_monetization() {
        assert!(ContentRecommendation::Safe.allows_monetization());
        assert!(ContentRecommendation::MonetizableWithCaution.allows_monetization());
        assert!(!ContentRecommendation::Demonetized.allows_monetization());
        assert!(!ContentRecommendation::StrikeRisk.allows_monetization());
        assert!(!ContentRecommendation::RequiresHumanReview.allows_monetization());
    }

    #[test]
    fn content_recommendation_severity_ordering() {
        assert!(
            ContentRecommendation::Safe.severity_score()
                < ContentRecommendation::MonetizableWithCaution.severity_score()
        );
        assert!(
            ContentRecommendation::MonetizableWithCaution.severity_score()
                < ContentRecommendation::Demonetized.severity_score()
        );
        assert!(
            ContentRecommendation::Demonetized.severity_score()
                < ContentRecommendation::StrikeRisk.severity_score()
        );
    }

    #[test]
    fn synthesize_empty_analyses() {
        let engine = SynthesisEngine::new();
        let result = engine
            .synthesize(&empty_visual(), &empty_audio(), &empty_metadata())
            .unwrap();

        assert_eq!(result.total_risk_score, 0.0);
        assert_eq!(result.recommendation, ContentRecommendation::Safe);
        assert!(result.elements.is_empty());
    }

    #[test]
    fn synthesize_with_violent_visual() {
        let engine = SynthesisEngine::new();
        let result = engine
            .synthesize(&violent_visual(), &empty_audio(), &empty_metadata())
            .unwrap();

        assert!(result.total_risk_score > 0.0);
        assert!(!result.elements.is_empty());
    }

    #[test]
    fn synthesize_detects_contradiction() {
        let engine = SynthesisEngine::new();
        let result = engine
            .synthesize(&violent_visual(), &calm_audio(), &empty_metadata())
            .unwrap();

        // Should detect violent visual + calm audio contradiction.
        assert!(!result.contradictions.is_empty());
    }

    #[test]
    fn classify_context_with_educational_elements() {
        let engine = SynthesisEngine::new();
        let elements = vec![
            ContentElement::text(
                "Welcome to this physics tutorial lesson",
                TextSource::Transcript,
                0.9,
            ),
            ContentElement::text(
                "Today we will learn the basics of quantum mechanics",
                TextSource::Transcript,
                0.85,
            ),
        ];

        let classification = engine.classify_context(&elements);
        assert_eq!(classification.primary_context, ContextType::Educational);
        assert!(classification.confidence > 0.0);
    }

    #[test]
    fn classify_context_with_satirical_elements() {
        let engine = SynthesisEngine::new();
        let elements = vec![ContentElement::text(
            "This is pure satire and comedy, not serious at all",
            TextSource::Transcript,
            0.9,
        )];

        let classification = engine.classify_context(&elements);
        assert_eq!(classification.primary_context, ContextType::Satirical);
    }

    #[test]
    fn detect_sarcasm_empty_transcript() {
        let engine = SynthesisEngine::new();
        let score = engine.detect_sarcasm(&[], &empty_visual());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn detect_sarcasm_with_patterns() {
        let engine = SynthesisEngine::new();
        let transcript = vec![
            TranscriptSegment {
                start: Timestamp::new(0.0, 0),
                end: Timestamp::new(3.0, 90),
                text: "Yeah right, like that is totally going to work".into(),
                confidence: 0.9,
                sentiment: -0.3,
                is_sarcasm: false,
                is_background_speech: false,
            },
            TranscriptSegment {
                start: Timestamp::new(3.0, 90),
                end: Timestamp::new(6.0, 180),
                text: "What a surprise, another delay".into(),
                confidence: 0.85,
                sentiment: -0.5,
                is_sarcasm: false,
                is_background_speech: false,
            },
        ];

        let score = engine.detect_sarcasm(&transcript, &empty_visual());
        assert!(score > 0.0, "sarcasm score should be > 0, got {}", score);
    }

    #[test]
    fn assess_educational_value() {
        let engine = SynthesisEngine::new();
        let elements = vec![
            ContentElement::text("Tutorial on programming", TextSource::Transcript, 0.9),
            ContentElement::text("Lesson about mathematics", TextSource::Transcript, 0.85),
            ContentElement::text("Learn science concepts", TextSource::Transcript, 0.8),
        ];

        let score = engine.assess_educational_value(&elements);
        assert!(score > 0.0, "educational score should be > 0");
        assert!(score <= 1.0);
    }

    #[test]
    fn assess_educational_value_empty() {
        let engine = SynthesisEngine::new();
        let score = engine.assess_educational_value(&[]);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn evaluate_artistic_merit() {
        let mut visual = empty_visual();
        visual.objects.push(DetectedObject::new("person", 0.8).at_timestamp(Timestamp::new(1.0, 30)));
        visual.objects.push(DetectedObject::new("car", 0.7).at_timestamp(Timestamp::new(2.0, 60)));
        visual.objects.push(DetectedObject::new("building", 0.75).at_timestamp(Timestamp::new(3.0, 90)));
        visual.transitions.push(SceneTransition::new(
            "fade",
            Timestamp::new(2.0, 60),
            Severity::Low,
        ));
        visual.transitions.push(SceneTransition::new(
            "cut",
            Timestamp::new(4.0, 120),
            Severity::Low,
        ));
        visual.texts.push(
            sentinel_core::proto::DetectedText::new("Title Card", 0.9)
                .at_timestamp(Timestamp::new(0.5, 15)),
        );

        let mut audio = empty_audio();
        audio.events.push(AudioEvent::new("potential_music", 0.8, Timestamp::new(1.0, 30)));
        audio.events.push(AudioEvent::new("speech", 0.7, Timestamp::new(3.0, 90)));
        audio.transcript.push(TranscriptSegment {
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(5.0, 150),
            text: "Welcome to the show".into(),
            confidence: 0.9,
            sentiment: 0.2,
            is_sarcasm: false,
            is_background_speech: false,
        });

        let engine = SynthesisEngine::new();
        let score = engine.evaluate_artistic_merit(&visual, &audio);
        assert!(score > 0.0, "artistic merit should be > 0");
        assert!(score <= 1.0);
    }

    #[test]
    fn generate_recommendation_safe() {
        let engine = SynthesisEngine::new();
        let rec = engine.generate_recommendation(5.0, &[], &[]);
        assert_eq!(rec, ContentRecommendation::Safe);
    }

    #[test]
    fn generate_recommendation_caution() {
        let engine = SynthesisEngine::new();
        let rec = engine.generate_recommendation(15.0, &[], &[]);
        assert_eq!(rec, ContentRecommendation::MonetizableWithCaution);
    }

    #[test]
    fn generate_recommendation_demonetized() {
        let engine = SynthesisEngine::new();
        let rec = engine.generate_recommendation(50.0, &[], &[]);
        assert_eq!(rec, ContentRecommendation::Demonetized);
    }

    #[test]
    fn generate_recommendation_strike_risk() {
        let engine = SynthesisEngine::new();
        let rec = engine.generate_recommendation(85.0, &[], &[]);
        assert_eq!(rec, ContentRecommendation::StrikeRisk);
    }

    #[test]
    fn generate_recommendation_human_review() {
        let engine = SynthesisEngine::new();
        let contradictions = vec![ModalContradiction::new(
            crate::cross_modal_fusion::ContradictionType::SexualVisualSeriousAudio,
            10.0,
            0.8,
        )];
        let rec = engine.generate_recommendation(40.0, &[], &contradictions);
        assert_eq!(rec, ContentRecommendation::RequiresHumanReview);
    }

    #[test]
    fn generate_recommendation_educational_reduces() {
        let engine = SynthesisEngine::new();
        let contexts = vec![ContextAssessment::new(ContextType::Educational, 0.8, 0.0, 0.0)];
        // With educational context, a moderate score becomes caution instead of demonetized.
        let rec = engine.generate_recommendation(25.0, &contexts, &[]);
        assert_eq!(rec, ContentRecommendation::MonetizableWithCaution);
    }

    #[test]
    fn synthesis_error_display() {
        let err = SynthesisError::General("test error".into());
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn context_assessment_builder() {
        let ca = ContextAssessment::new(ContextType::Educational, 0.75, 0.0, 10.0)
            .with_evidence(vec!["whiteboard".into(), "lecture".into()]);
        assert_eq!(ca.context_type, ContextType::Educational);
        assert_eq!(ca.supporting_evidence.len(), 2);
    }

    #[test]
    fn sarcasm_detection_result_defaults() {
        let sr = SarcasmDetectionResult {
            sarcasm_score: 0.5,
            matched_patterns: vec!["yeah right".into()],
            incongruity_score: 0.3,
            exaggeration_score: 0.2,
        };
        assert_eq!(sr.sarcasm_score, 0.5);
        assert_eq!(sr.matched_patterns.len(), 1);
    }

    #[test]
    fn synthesis_result_needs_human_review() {
        let mut result = SynthesisResult::empty();
        result.total_risk_score = 55.0;
        result.recommendation = ContentRecommendation::RequiresHumanReview;
        assert!(result.needs_human_review());
    }

    #[test]
    fn stage_metrics_populated() {
        let engine = SynthesisEngine::new();
        let mut visual = violent_visual();
        let audio = calm_audio();
        let metadata = empty_metadata();

        // Add some transcript for sarcasm detection.
        let mut audio_with_transcript = audio;
        audio_with_transcript.transcript.push(TranscriptSegment {
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(5.0, 150),
            text: "Yeah right, that is totally believable".into(),
            confidence: 0.9,
            sentiment: -0.3,
            is_sarcasm: false,
            is_background_speech: false,
        });

        let result = engine.synthesize(&visual, &audio_with_transcript, &metadata).unwrap();

        assert!(!result.stage_metrics.is_empty());
        assert!(result.stage_metrics.contains_key("stage1_elements_extracted"));
        assert!(result.stage_metrics.contains_key("stage2_primary_context"));
        assert!(result.stage_metrics.contains_key("stage6_total_risk"));
    }
}
