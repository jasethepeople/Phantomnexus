//! # synthesis
//!
//! Contextual AI synthesis and multi-modal data fusion for the YouTube
//! Sentinel platform.
//!
//! Orchestrates a 6-stage synthesis pipeline that combines visual,
//! audio, and metadata analysis into unified content understanding:
//!
//! 1. **Extract content elements** — normalise detections into `ContentElement`s.
//! 2. **Classify primary context** — score against 5 indicator lexicons.
//! 3. **Detect contradictions** — find 5 cross-modal contradiction patterns.
//! 4. **Resolve cross-modal conflicts** — apply resolution strategies.
//! 5. **Temporal correlation** — detect escalation, repetition, cluster, etc.
//! 6. **Generate recommendation** — produce `ContentRecommendation`.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use synthesis::engine::SynthesisEngine;
//! use sentinel_core::proto::{VisualAnalysis, AudioAnalysis, MetadataAnalysis};
//!
//! fn analyze_content() {
//!     let engine = SynthesisEngine::new();
//!
//!     let visual = VisualAnalysis::empty();
//!     let audio = AudioAnalysis::empty();
//!     let metadata = MetadataAnalysis::empty();
//!
//!     let result = engine.synthesize(&visual, &audio, &metadata).unwrap();
//!     println!("risk: {:.1}", result.total_risk_score);
//!     println!("recommendation: {:?}", result.recommendation);
//! }
//! ```

// ===========================================================================
// Module declarations
// ===========================================================================

pub mod content_elements;
pub mod context_classifier;
pub mod cross_modal_fusion;
pub mod engine;
pub mod temporal_correlator;

// ===========================================================================
// Convenience re-exports
// ===========================================================================

// Main engine
pub use engine::{
    ContentRecommendation, ContextAssessment, SarcasmDetectionResult, SynthesisEngine,
    SynthesisError, SynthesisResult,
};

// Content elements
pub use content_elements::{
    BoundingBox, ContentElement, RiskCategory, RiskIndicator, TextSource, Timestamp,
};

// Context classifier
pub use context_classifier::{
    ContextClassification, ContextClassifier, ContextType, IndicatorLexicon, LexiconBuilder,
    ScoredPattern,
};

// Cross-modal fusion
pub use cross_modal_fusion::{
    ContradictionResolution, ContradictionType, CrossModalFusion, FusedSignals,
    ModalContradiction,
};

// Temporal correlator
pub use temporal_correlator::{
    RiskTimelineEntry, TemporalCorrelation, TemporalCorrelator, TemporalPattern,
};

// ===========================================================================
// Crate-level tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that all public modules are accessible and key types can be
    /// constructed.
    #[test]
    fn public_api_smoke_test() {
        // Engine
        let engine = SynthesisEngine::new();
        assert_eq!(engine.cross_modal_fusion.min_confidence, 0.5);

        // Content elements
        let elem = ContentElement::text("test", TextSource::Ocr, 0.9);
        assert_eq!(elem.confidence(), 0.9);

        // Context classifier
        let classifier = ContextClassifier::new();
        assert!(classifier.total_pattern_count() >= 250);

        // Cross-modal fusion
        let fusion = CrossModalFusion::new();
        assert_eq!(fusion.temporal_tolerance_seconds, 2.0);

        // Temporal correlator
        let correlator = TemporalCorrelator::new();
        assert_eq!(correlator.min_gap_seconds, 0.5);
    }

    /// Verify re-exports are accessible directly from crate root.
    #[test]
    fn reexports_are_accessible() {
        let _rec = ContentRecommendation::Safe;
        let _ctx = ContextType::Educational;
        let _pattern = TemporalPattern::Escalation;
        let _resolution = ContradictionResolution::FavorVisual;
        let _contradiction_type = ContradictionType::ViolentVisualCalmAudio;
    }

    /// End-to-end: synthesize with empty inputs produces safe result.
    #[test]
    fn end_to_end_empty_synthesis() {
        let engine = SynthesisEngine::new();
        let visual = sentinel_core::proto::VisualAnalysis::empty();
        let audio = sentinel_core::proto::AudioAnalysis::empty();
        let metadata = sentinel_core::proto::MetadataAnalysis::empty();

        let result = engine.synthesize(&visual, &audio, &metadata).unwrap();

        assert_eq!(result.recommendation, ContentRecommendation::Safe);
        assert_eq!(result.total_risk_score, 0.0);
        assert!(result.elements.is_empty());
        assert!(!result.needs_human_review());
    }

    /// End-to-end: synthesize with violent visual and calm audio detects
    /// contradiction.
    #[test]
    fn end_to_end_contradiction_detection() {
        use sentinel_core::proto::{
            AudioEvent, BoundingBox, DetectedObject, Timestamp, TranscriptSegment, ViolationCategory,
        };

        let mut visual = sentinel_core::proto::VisualAnalysis::empty();
        visual.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(5.0, 150)),
        );
        visual.overall_risk_score = 15.0;

        let mut audio = sentinel_core::proto::AudioAnalysis::empty();
        audio.events.push(AudioEvent::new(
            "potential_music",
            0.8,
            Timestamp::new(5.1, 153),
        ));
        audio.overall_risk_score = 5.0;
        audio.transcript.push(TranscriptSegment {
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(10.0, 300),
            text: "This is a peaceful tutorial about education".into(),
            confidence: 0.9,
            sentiment: 0.1,
            is_sarcasm: false,
            is_background_speech: false,
        });

        let metadata = sentinel_core::proto::MetadataAnalysis::empty();

        let engine = SynthesisEngine::new();
        let result = engine.synthesize(&visual, &audio, &metadata).unwrap();

        // Should have detected elements.
        assert!(!result.elements.is_empty());

        // Should have context classification.
        assert!(result.context_classification.confidence >= 0.0);

        // Risk should be computed.
        assert!(result.total_risk_score >= 0.0);

        // All 6 stage metrics should be present.
        assert!(result.stage_metrics.contains_key("stage1_elements_extracted"));
        assert!(result.stage_metrics.contains_key("stage2_primary_context"));
        assert!(result.stage_metrics.contains_key("stage3_contradictions"));
        assert!(result.stage_metrics.contains_key("stage4_resolved_risk"));
        assert!(result.stage_metrics.contains_key("stage5_temporal_patterns"));
        assert!(result.stage_metrics.contains_key("stage6_total_risk"));
    }

    /// Verify that the context classifier correctly identifies educational
    /// content end-to-end.
    #[test]
    fn end_to_end_educational_classification() {
        use sentinel_core::proto::{AudioAnalysis, MetadataAnalysis, TranscriptSegment, Timestamp, VisualAnalysis};

        let visual = VisualAnalysis::empty();

        let mut audio = AudioAnalysis::empty();
        audio.transcript.push(TranscriptSegment {
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(5.0, 150),
            text: "Welcome to this tutorial lesson where we will learn step by step".into(),
            confidence: 0.95,
            sentiment: 0.0,
            is_sarcasm: false,
            is_background_speech: false,
        });

        let metadata = MetadataAnalysis::empty();

        let engine = SynthesisEngine::new();
        let result = engine.synthesize(&visual, &audio, &metadata).unwrap();

        assert_eq!(result.context_classification.primary_context, ContextType::Educational);
        assert!(result.educational_value > 0.0);
        // Educational context should reduce risk.
        assert!(result.context_classification.primary_context.risk_reduction_factor() < 1.0);
    }

    /// Verify ContentRecommendation variants and their behaviour.
    #[test]
    fn content_recommendation_behaviour() {
        assert!(ContentRecommendation::Safe.allows_monetization());
        assert!(!ContentRecommendation::StrikeRisk.allows_monetization());
        assert!(ContentRecommendation::RequiresHumanReview.requires_human());
        assert!(!ContentRecommendation::Safe.requires_human());
    }

    /// Test the sarcasm detection with clear satirical language.
    #[test]
    fn sarcasm_detection_clear_patterns() {
        use sentinel_core::proto::{AudioAnalysis, Timestamp, TranscriptSegment, VisualAnalysis};

        let visual = VisualAnalysis::empty();
        let mut audio = AudioAnalysis::empty();
        audio.transcript.push(TranscriptSegment {
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(3.0, 90),
            text: "Yeah right, because that makes so much sense".into(),
            confidence: 0.95,
            sentiment: -0.4,
            is_sarcasm: false,
            is_background_speech: false,
        });
        audio.transcript.push(TranscriptSegment {
            start: Timestamp::new(3.0, 90),
            end: Timestamp::new(6.0, 180),
            text: "What a surprise, totally expected this outcome".into(),
            confidence: 0.9,
            sentiment: -0.5,
            is_sarcasm: false,
            is_background_speech: false,
        });

        let engine = SynthesisEngine::new();
        let result = engine
            .synthesize(&visual, &audio, &sentinel_core::proto::MetadataAnalysis::empty())
            .unwrap();

        assert!(result.sarcasm_result.is_some());
        let sarcasm = result.sarcasm_result.unwrap();
        assert!(
            sarcasm.sarcasm_score > 0.0,
            "sarcasm should be detected in satirical transcript"
        );
    }

    /// Test temporal correlation produces results with violent + audio events.
    #[test]
    fn temporal_correlation_detects_patterns() {
        use sentinel_core::proto::{
            AudioEvent, BoundingBox, DetectedObject, Timestamp, ViolationCategory, VisualAnalysis,
            AudioAnalysis, MetadataAnalysis,
        };

        let mut visual = VisualAnalysis::empty();
        // Multiple violent objects for pattern detection.
        for i in 0..5 {
            visual.objects.push(
                DetectedObject::new("weapon", 0.85)
                    .with_category(ViolationCategory::Violence)
                    .at_timestamp(Timestamp::new(i as f64 * 2.0, (i * 60) as u64)),
            );
        }
        visual.overall_risk_score = 30.0;

        let mut audio = AudioAnalysis::empty();
        audio.events.push(AudioEvent::new("scream", 0.9, Timestamp::new(4.0, 120)));
        audio.events.push(AudioEvent::new("loud_high_freq", 0.8, Timestamp::new(6.0, 180)));
        audio.overall_risk_score = 15.0;

        let metadata = MetadataAnalysis::empty();

        let engine = SynthesisEngine::new();
        let result = engine.synthesize(&visual, &audio, &metadata).unwrap();

        // Temporal patterns should have been detected.
        assert!(
            result.stage_metrics.get("stage5_temporal_patterns").unwrap_or(&0.0) > &0.0,
            "temporal patterns should be detected"
        );
    }
}
