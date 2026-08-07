//! Cross-modal fusion — detects contradictions and fuses signals from
//! visual and audio analysis pipelines.
//!
//! Identifies five cross-modal contradiction patterns (e.g. peaceful visual
//! + aggressive audio) and provides resolution strategies via the
//! `ContradictionResolution` enum.

use crate::content_elements::{ContentElement, TextSource};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ContradictionType — five known cross-modal contradiction patterns
// ---------------------------------------------------------------------------

/// The type of contradiction detected between visual and audio modalities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContradictionType {
    /// Peaceful visual content paired with aggressive audio.
    PeacefulVisualAggressiveAudio,
    /// Violent visual content paired with calm audio.
    ViolentVisualCalmAudio,
    /// Sexual visual content paired with serious/narrative audio.
    SexualVisualSeriousAudio,
    /// Violent visual content paired with educational audio.
    ViolentVisualEducationalAudio,
    /// Comedic audio paired with serious visual content.
    ComedicAudioSeriousVisual,
}

impl ContradictionType {
    pub fn label(&self) -> &'static str {
        match self {
            ContradictionType::PeacefulVisualAggressiveAudio => {
                "peaceful_visual_aggressive_audio"
            }
            ContradictionType::ViolentVisualCalmAudio => "violent_visual_calm_audio",
            ContradictionType::SexualVisualSeriousAudio => "sexual_visual_serious_audio",
            ContradictionType::ViolentVisualEducationalAudio => {
                "violent_visual_educational_audio"
            }
            ContradictionType::ComedicAudioSeriousVisual => "comedic_audio_serious_visual",
        }
    }

    /// Risk score contribution for this contradiction type.
    pub fn base_risk(&self) -> f64 {
        match self {
            // Most concerning: violence masked by calm audio.
            ContradictionType::ViolentVisualCalmAudio => 35.0,
            // Violence in educational context is suspicious.
            ContradictionType::ViolentVisualEducationalAudio => 30.0,
            // Sexual content with serious audio is concerning.
            ContradictionType::SexualVisualSeriousAudio => 28.0,
            // Peaceful visual + aggressive audio suggests hidden conflict.
            ContradictionType::PeacefulVisualAggressiveAudio => 25.0,
            // Comedy over serious visual may be satirical.
            ContradictionType::ComedicAudioSeriousVisual => 15.0,
        }
    }

    /// Human-readable description template.
    pub fn description(&self) -> &'static str {
        match self {
            ContradictionType::PeacefulVisualAggressiveAudio => {
                "Peaceful visual content accompanied by aggressive audio signals"
            }
            ContradictionType::ViolentVisualCalmAudio => {
                "Violent visual content with unexpectedly calm audio track"
            }
            ContradictionType::SexualVisualSeriousAudio => {
                "Potentially sexual visual content overlaid with serious narration"
            }
            ContradictionType::ViolentVisualEducationalAudio => {
                "Violent imagery in what appears to be an educational context"
            }
            ContradictionType::ComedicAudioSeriousVisual => {
                "Comedic audio overlaid on serious/dramatic visual content"
            }
        }
    }

    /// Default resolution strategy for this contradiction type.
    pub fn default_resolution(&self) -> ContradictionResolution {
        match self {
            ContradictionType::PeacefulVisualAggressiveAudio => {
                // Audio signals intent; visual may be deceptive.
                ContradictionResolution::FavorAudio
            }
            ContradictionType::ViolentVisualCalmAudio => {
                // Visual evidence is stronger; audio may be masking.
                ContradictionResolution::FavorVisual
            }
            ContradictionType::SexualVisualSeriousAudio => {
                // Could be artistic/educational context.
                ContradictionResolution::RequiresReview
            }
            ContradictionType::ViolentVisualEducationalAudio => {
                // Educational context reduces risk.
                ContradictionResolution::ArtisticContext
            }
            ContradictionType::ComedicAudioSeriousVisual => {
                // Satirical framing likely.
                ContradictionResolution::ArtisticContext
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ContradictionResolution — how to resolve a detected contradiction
// ---------------------------------------------------------------------------

/// Strategy for resolving a cross-modal contradiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContradictionResolution {
    /// Trust the visual signal over the audio.
    FavorVisual,
    /// Trust the audio signal over the visual.
    FavorAudio,
    /// The contradiction is explained by artistic / satirical / educational
    /// context — reduce risk.
    ArtisticContext,
    /// The contradiction is ambiguous and requires human review.
    RequiresReview,
}

impl ContradictionResolution {
    pub fn label(&self) -> &'static str {
        match self {
            ContradictionResolution::FavorVisual => "favor_visual",
            ContradictionResolution::FavorAudio => "favor_audio",
            ContradictionResolution::ArtisticContext => "artistic_context",
            ContradictionResolution::RequiresReview => "requires_review",
        }
    }
}

// ---------------------------------------------------------------------------
// ModalContradiction — a detected contradiction instance
// ---------------------------------------------------------------------------

/// A single detected cross-modal contradiction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModalContradiction {
    /// The type of contradiction.
    pub contradiction_type: ContradictionType,
    /// Timestamp where the contradiction occurs (seconds).
    pub timestamp_seconds: f64,
    /// Confidence in the contradiction detection (0.0..1.0).
    pub confidence: f64,
    /// Resolved risk score after applying the resolution strategy.
    pub resolved_risk: f64,
    /// The resolution strategy applied.
    pub resolution: ContradictionResolution,
    /// Visual evidence labels.
    pub visual_evidence: Vec<String>,
    /// Audio evidence labels.
    pub audio_evidence: Vec<String>,
    /// Human-readable explanation.
    pub explanation: String,
}

impl ModalContradiction {
    pub fn new(
        contradiction_type: ContradictionType,
        timestamp_seconds: f64,
        confidence: f64,
    ) -> Self {
        let resolution = contradiction_type.default_resolution();
        let base_risk = contradiction_type.base_risk();
        let resolved_risk = match resolution {
            ContradictionResolution::FavorVisual => base_risk * confidence,
            ContradictionResolution::FavorAudio => base_risk * confidence * 0.9,
            ContradictionResolution::ArtisticContext => base_risk * confidence * 0.5,
            ContradictionResolution::RequiresReview => base_risk * confidence * 0.8,
        };

        ModalContradiction {
            contradiction_type,
            timestamp_seconds,
            confidence: confidence.clamp(0.0, 1.0),
            resolved_risk: resolved_risk.clamp(0.0, 100.0),
            resolution,
            visual_evidence: Vec::new(),
            audio_evidence: Vec::new(),
            explanation: contradiction_type.description().to_string(),
        }
    }

    pub fn with_visual_evidence(mut self, evidence: Vec<String>) -> Self {
        self.visual_evidence = evidence;
        self
    }

    pub fn with_audio_evidence(mut self, evidence: Vec<String>) -> Self {
        self.audio_evidence = evidence;
        self
    }

    pub fn with_resolution(mut self, resolution: ContradictionResolution) -> Self {
        self.resolution = resolution;
        // Recalculate resolved risk.
        let base_risk = self.contradiction_type.base_risk();
        self.resolved_risk = match resolution {
            ContradictionResolution::FavorVisual => base_risk * self.confidence,
            ContradictionResolution::FavorAudio => base_risk * self.confidence * 0.9,
            ContradictionResolution::ArtisticContext => base_risk * self.confidence * 0.5,
            ContradictionResolution::RequiresReview => base_risk * self.confidence * 0.8,
        }
        .clamp(0.0, 100.0);
        self
    }
}

// ---------------------------------------------------------------------------
// FusedSignals — the result of fusing visual and audio signals
// ---------------------------------------------------------------------------

/// Fused signals produced by cross-modal fusion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FusedSignals {
    /// Unified content elements from both modalities.
    pub elements: Vec<ContentElement>,
    /// Detected cross-modal contradictions.
    pub contradictions: Vec<ModalContradiction>,
    /// Overall fused risk score.
    pub fused_risk_score: f64,
    /// Number of visual signals processed.
    pub visual_signal_count: usize,
    /// Number of audio signals processed.
    pub audio_signal_count: usize,
    /// Number of contradictions resolved via artistic context.
    pub artistic_resolutions: usize,
    /// Number of contradictions requiring human review.
    pub review_required_count: usize,
}

impl FusedSignals {
    pub fn empty() -> Self {
        FusedSignals {
            elements: Vec::new(),
            contradictions: Vec::new(),
            fused_risk_score: 0.0,
            visual_signal_count: 0,
            audio_signal_count: 0,
            artistic_resolutions: 0,
            review_required_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// CrossModalFusion
// ---------------------------------------------------------------------------

/// Fuses visual and audio analysis results and detects cross-modal
/// contradictions.
#[derive(Debug, Clone)]
pub struct CrossModalFusion {
    /// Minimum confidence for an object/event to be considered.
    pub min_confidence: f64,
    /// Time tolerance for aligning visual and audio events (seconds).
    pub temporal_tolerance_seconds: f64,
}

impl Default for CrossModalFusion {
    fn default() -> Self {
        CrossModalFusion {
            min_confidence: 0.5,
            temporal_tolerance_seconds: 2.0,
        }
    }
}

impl CrossModalFusion {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fuse visual and audio analysis results into unified signals.
    ///
    /// Extracts content elements from both modalities, detects
    /// contradictions, and produces a fused risk score.
    pub fn fuse(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> FusedSignals {
        let mut elements = Vec::new();

        // Convert visual objects to content elements.
        for obj in &visual.objects {
            if obj.confidence < self.min_confidence {
                continue;
            }
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

        // Convert visual text (OCR) to content elements.
        for text in &visual.texts {
            if text.confidence < self.min_confidence {
                continue;
            }
            let source = if text.is_obscured {
                TextSource::Embedded
            } else {
                TextSource::Ocr
            };
            let element = ContentElement::text(&text.text, source, text.confidence);
            elements.push(element);
        }

        // Convert audio events to content elements.
        for evt in &audio.events {
            if evt.confidence < self.min_confidence {
                continue;
            }
            let element = ContentElement::audio(
                &evt.event_type,
                evt.confidence,
                crate::content_elements::Timestamp::new(
                    evt.timestamp.seconds,
                    evt.timestamp.frame_number,
                ),
                None,
            );
            elements.push(element);
        }

        // Convert transcript segments to content elements.
        for seg in &audio.transcript {
            if seg.confidence < self.min_confidence {
                continue;
            }
            let element = ContentElement::audio(
                "speech",
                seg.confidence,
                crate::content_elements::Timestamp::new(
                    seg.start.seconds,
                    seg.start.frame_number,
                ),
                Some(seg.text.clone()),
            );
            elements.push(element);
        }

        let visual_signal_count = visual.objects.len() + visual.texts.len();
        let audio_signal_count = audio.events.len() + audio.transcript.len();

        // Detect contradictions.
        let contradictions = self.detect_contradictions(visual, audio);

        let artistic_resolutions = contradictions
            .iter()
            .filter(|c| c.resolution == ContradictionResolution::ArtisticContext)
            .count();
        let review_required_count = contradictions
            .iter()
            .filter(|c| c.resolution == ContradictionResolution::RequiresReview)
            .count();

        // Compute fused risk score.
        let base_risk = visual.overall_risk_score + audio.overall_risk_score;
        let contradiction_adjustment: f64 = contradictions
            .iter()
            .map(|c| c.resolved_risk)
            .sum();

        let fused_risk_score = (base_risk + contradiction_adjustment).min(100.0).max(0.0);

        FusedSignals {
            elements,
            contradictions,
            fused_risk_score,
            visual_signal_count,
            audio_signal_count,
            artistic_resolutions,
            review_required_count,
        }
    }

    /// Detect cross-modal contradictions between visual and audio analysis.
    ///
    /// Checks five contradiction patterns:
    /// 1. Peaceful visual + aggressive audio
    /// 2. Violent visual + calm audio
    /// 3. Sexual visual + serious audio
    /// 4. Violent visual + educational audio
    /// 5. Comedic audio + serious visual
    pub fn detect_contradictions(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<ModalContradiction> {
        let mut contradictions = Vec::new();

        // Pattern 1: Peaceful visual + aggressive audio
        contradictions.extend(
            self.check_peaceful_visual_aggressive_audio(visual, audio),
        );

        // Pattern 2: Violent visual + calm audio
        contradictions.extend(self.check_violent_visual_calm_audio(visual, audio));

        // Pattern 3: Sexual visual + serious audio
        contradictions.extend(self.check_sexual_visual_serious_audio(visual, audio));

        // Pattern 4: Violent visual + educational audio
        contradictions.extend(
            self.check_violent_visual_educational_audio(visual, audio),
        );

        // Pattern 5: Comedic audio + serious visual
        contradictions.extend(self.check_comedic_audio_serious_visual(visual, audio));

        // Sort by timestamp.
        contradictions.sort_by(|a, b| {
            a.timestamp_seconds
                .partial_cmp(&b.timestamp_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        contradictions
    }

    // ------------------------------------------------------------------
    // Contradiction pattern: Peaceful visual + Aggressive audio
    // ------------------------------------------------------------------

    fn check_peaceful_visual_aggressive_audio(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<ModalContradiction> {
        let mut contradictions = Vec::new();

        // Check for peaceful visual labels.
        let peaceful_labels = ["flower", "sunset", "landscape", "nature", "baby", "pet",
            "peaceful", "calm", "serene", "garden", "park", "beach", "mountain"];

        // Check for aggressive audio events.
        let aggressive_audio = ["scream", "gunshot", "explosion", "shout", "yell",
            "loud_high_freq", "loud_mid_freq", "sharp_transient", "aggressive"];

        for obj in &visual.objects {
            if !peaceful_labels.iter().any(|&l| obj.label.to_lowercase().contains(l)) {
                continue;
            }

            for evt in &audio.events {
                if !aggressive_audio.iter().any(|&l| evt.event_type.to_lowercase().contains(l)) {
                    continue;
                }

                let time_diff = (obj.timestamp.seconds - evt.timestamp.seconds).abs();
                if time_diff <= self.temporal_tolerance_seconds {
                    let confidence = obj.confidence * evt.confidence;
                    contradictions.push(
                        ModalContradiction::new(
                            ContradictionType::PeacefulVisualAggressiveAudio,
                            (obj.timestamp.seconds + evt.timestamp.seconds) / 2.0,
                            confidence,
                        )
                        .with_visual_evidence(vec![obj.label.clone()])
                        .with_audio_evidence(vec![evt.event_type.clone()]),
                    );
                }
            }
        }

        contradictions
    }

    // ------------------------------------------------------------------
    // Contradiction pattern: Violent visual + Calm audio
    // ------------------------------------------------------------------

    fn check_violent_visual_calm_audio(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<ModalContradiction> {
        let mut contradictions = Vec::new();

        let violent_labels = ["weapon", "gun", "knife", "blood", "fight", "punch",
            "kick", "weapon", "violence", "attack", "injury", "corpse", "explosion"];

        let calm_audio = ["extended_silence", "potential_music", "quiet", "soft",
            "ambient", "gentle", "calm"];

        for obj in &visual.objects {
            if !violent_labels.iter().any(|&l| obj.label.to_lowercase().contains(l)) {
                continue;
            }

            for evt in &audio.events {
                if !calm_audio.iter().any(|&l| evt.event_type.to_lowercase().contains(l)) {
                    continue;
                }

                let time_diff = (obj.timestamp.seconds - evt.timestamp.seconds).abs();
                if time_diff <= self.temporal_tolerance_seconds {
                    let confidence = obj.confidence * evt.confidence;
                    contradictions.push(
                        ModalContradiction::new(
                            ContradictionType::ViolentVisualCalmAudio,
                            (obj.timestamp.seconds + evt.timestamp.seconds) / 2.0,
                            confidence,
                        )
                        .with_visual_evidence(vec![obj.label.clone()])
                        .with_audio_evidence(vec![evt.event_type.clone()]),
                    );
                }
            }
        }

        contradictions
    }

    // ------------------------------------------------------------------
    // Contradiction pattern: Sexual visual + Serious audio
    // ------------------------------------------------------------------

    fn check_sexual_visual_serious_audio(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<ModalContradiction> {
        let mut contradictions = Vec::new();

        let sexual_labels = ["nudity", "underwear", "lingerie", "intimate", "kiss",
            "suggestive", "provocative", "adult"];

        let serious_audio = ["speech", "narration", "news", "report", "lecture",
            "interview", "discussion"];

        for obj in &visual.objects {
            if !sexual_labels.iter().any(|&l| obj.label.to_lowercase().contains(l)) {
                continue;
            }

            for evt in &audio.events {
                if !serious_audio.iter().any(|&l| evt.event_type.to_lowercase().contains(l)) {
                    continue;
                }

                let time_diff = (obj.timestamp.seconds - evt.timestamp.seconds).abs();
                if time_diff <= self.temporal_tolerance_seconds {
                    let confidence = obj.confidence * evt.confidence;
                    contradictions.push(
                        ModalContradiction::new(
                            ContradictionType::SexualVisualSeriousAudio,
                            (obj.timestamp.seconds + evt.timestamp.seconds) / 2.0,
                            confidence,
                        )
                        .with_visual_evidence(vec![obj.label.clone()])
                        .with_audio_evidence(vec![evt.event_type.clone()]),
                    );
                }
            }
        }

        // Also check transcript for serious narration.
        for obj in &visual.objects {
            if !sexual_labels.iter().any(|&l| obj.label.to_lowercase().contains(l)) {
                continue;
            }

            for seg in &audio.transcript {
                let transcript_lower = seg.text.to_lowercase();
                if serious_audio.iter().any(|&l| transcript_lower.contains(l)) {
                    let time_diff = (obj.timestamp.seconds - seg.start.seconds).abs();
                    if time_diff <= self.temporal_tolerance_seconds {
                        let confidence = obj.confidence * seg.confidence;
                        contradictions.push(
                            ModalContradiction::new(
                                ContradictionType::SexualVisualSeriousAudio,
                                (obj.timestamp.seconds + seg.start.seconds) / 2.0,
                                confidence,
                            )
                            .with_visual_evidence(vec![obj.label.clone()])
                            .with_audio_evidence(vec![format!("transcript:{}", seg.text)]),
                        );
                    }
                }
            }
        }

        contradictions
    }

    // ------------------------------------------------------------------
    // Contradiction pattern: Violent visual + Educational audio
    // ------------------------------------------------------------------

    fn check_violent_visual_educational_audio(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<ModalContradiction> {
        let mut contradictions = Vec::new();

        let violent_labels = ["weapon", "gun", "knife", "blood", "fight", "punch",
            "kick", "weapon", "violence", "attack", "explosion", "battle", "war"];

        let educational_keywords = ["tutorial", "lesson", "educational", "learn",
            "teach", "explain", "course", "lecture", "study", "science", "history"];

        for obj in &visual.objects {
            if !violent_labels.iter().any(|&l| obj.label.to_lowercase().contains(l)) {
                continue;
            }

            for seg in &audio.transcript {
                let transcript_lower = seg.text.to_lowercase();
                if educational_keywords.iter().any(|&l| transcript_lower.contains(l)) {
                    let time_diff = (obj.timestamp.seconds - seg.start.seconds).abs();
                    if time_diff <= self.temporal_tolerance_seconds {
                        let confidence = obj.confidence * seg.confidence;
                        contradictions.push(
                            ModalContradiction::new(
                                ContradictionType::ViolentVisualEducationalAudio,
                                (obj.timestamp.seconds + seg.start.seconds) / 2.0,
                                confidence,
                            )
                            .with_visual_evidence(vec![obj.label.clone()])
                            .with_audio_evidence(vec![format!("transcript:{}", seg.text)]),
                        );
                    }
                }
            }
        }

        contradictions
    }

    // ------------------------------------------------------------------
    // Contradiction pattern: Comedic audio + Serious visual
    // ------------------------------------------------------------------

    fn check_comedic_audio_serious_visual(
        &self,
        visual: &sentinel_core::proto::VisualAnalysis,
        audio: &sentinel_core::proto::AudioAnalysis,
    ) -> Vec<ModalContradiction> {
        let mut contradictions = Vec::new();

        let serious_labels = ["disaster", "accident", "injury", "hospital", "emergency",
            "funeral", "crime", "police", "court", "prison"];

        let comedic_audio = ["laughter", "comedy", "joke", "funny", "humor", "sitcom",
            "stand_up", "applause"];

        for obj in &visual.objects {
            if !serious_labels.iter().any(|&l| obj.label.to_lowercase().contains(l)) {
                continue;
            }

            for evt in &audio.events {
                if !comedic_audio.iter().any(|&l| evt.event_type.to_lowercase().contains(l)) {
                    continue;
                }

                let time_diff = (obj.timestamp.seconds - evt.timestamp.seconds).abs();
                if time_diff <= self.temporal_tolerance_seconds {
                    let confidence = obj.confidence * evt.confidence;
                    contradictions.push(
                        ModalContradiction::new(
                            ContradictionType::ComedicAudioSeriousVisual,
                            (obj.timestamp.seconds + evt.timestamp.seconds) / 2.0,
                            confidence,
                        )
                        .with_visual_evidence(vec![obj.label.clone()])
                        .with_audio_evidence(vec![evt.event_type.clone()]),
                    );
                }
            }
        }

        contradictions
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::proto::{AudioEvent, BoundingBox, DetectedObject, Timestamp, ViolationCategory};

    fn empty_visual() -> sentinel_core::proto::VisualAnalysis {
        sentinel_core::proto::VisualAnalysis::empty()
    }

    fn empty_audio() -> sentinel_core::proto::AudioAnalysis {
        sentinel_core::proto::AudioAnalysis::empty()
    }

    #[test]
    fn contradiction_type_labels() {
        assert_eq!(
            ContradictionType::PeacefulVisualAggressiveAudio.label(),
            "peaceful_visual_aggressive_audio"
        );
        assert_eq!(
            ContradictionType::ViolentVisualCalmAudio.label(),
            "violent_visual_calm_audio"
        );
        assert_eq!(
            ContradictionType::ComedicAudioSeriousVisual.label(),
            "comedic_audio_serious_visual"
        );
    }

    #[test]
    fn contradiction_type_risk_ordering() {
        assert!(
            ContradictionType::ViolentVisualCalmAudio.base_risk()
                > ContradictionType::ComedicAudioSeriousVisual.base_risk()
        );
    }

    #[test]
    fn contradiction_default_resolutions() {
        assert_eq!(
            ContradictionType::ViolentVisualCalmAudio.default_resolution(),
            ContradictionResolution::FavorVisual
        );
        assert_eq!(
            ContradictionType::PeacefulVisualAggressiveAudio.default_resolution(),
            ContradictionResolution::FavorAudio
        );
        assert_eq!(
            ContradictionType::SexualVisualSeriousAudio.default_resolution(),
            ContradictionResolution::RequiresReview
        );
    }

    #[test]
    fn modal_contradiction_builder() {
        let mc = ModalContradiction::new(
            ContradictionType::ViolentVisualCalmAudio,
            10.0,
            0.8,
        )
        .with_visual_evidence(vec!["weapon".into()])
        .with_audio_evidence(vec!["calm_music".into()]);

        assert_eq!(mc.visual_evidence.len(), 1);
        assert_eq!(mc.audio_evidence.len(), 1);
        assert!(mc.resolved_risk > 0.0);
    }

    #[test]
    fn modal_contradiction_with_resolution_override() {
        let mc = ModalContradiction::new(
            ContradictionType::ViolentVisualCalmAudio,
            5.0,
            0.9,
        )
        .with_resolution(ContradictionResolution::ArtisticContext);

        assert_eq!(mc.resolution, ContradictionResolution::ArtisticContext);
        // Artistic context reduces risk.
        assert!(mc.resolved_risk < mc.contradiction_type.base_risk());
    }

    #[test]
    fn fused_signals_empty() {
        let signals = FusedSignals::empty();
        assert!(signals.elements.is_empty());
        assert!(signals.contradictions.is_empty());
        assert_eq!(signals.fused_risk_score, 0.0);
    }

    #[test]
    fn cross_modal_fusion_new() {
        let fusion = CrossModalFusion::new();
        assert_eq!(fusion.min_confidence, 0.5);
        assert_eq!(fusion.temporal_tolerance_seconds, 2.0);
    }

    #[test]
    fn fuse_empty_analyses() {
        let fusion = CrossModalFusion::new();
        let result = fusion.fuse(&empty_visual(), &empty_audio());
        assert!(result.elements.is_empty());
        assert!(result.contradictions.is_empty());
    }

    #[test]
    fn fuse_with_visual_objects() {
        let mut visual = empty_visual();
        visual.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(5.0, 150)),
        );

        let audio = empty_audio();
        let fusion = CrossModalFusion::new();
        let result = fusion.fuse(&visual, &audio);

        assert_eq!(result.visual_signal_count, 1);
        assert!(!result.elements.is_empty());
        assert!(result
            .elements
            .iter()
            .any(|e| e.to_display_string().contains("weapon")));
    }

    #[test]
    fn detect_contradiction_violent_visual_calm_audio() {
        let mut visual = empty_visual();
        visual.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence)
                .at_timestamp(Timestamp::new(5.0, 150)),
        );

        let mut audio = empty_audio();
        audio.events.push(AudioEvent::new("potential_music", 0.8, Timestamp::new(5.1, 153)));

        let fusion = CrossModalFusion::new();
        let contradictions = fusion.detect_contradictions(&visual, &audio);

        assert!(!contradictions.is_empty());
        assert!(contradictions
            .iter()
            .any(|c| c.contradiction_type == ContradictionType::ViolentVisualCalmAudio));
    }

    #[test]
    fn detect_contradiction_peaceful_visual_aggressive_audio() {
        let mut visual = empty_visual();
        visual.objects.push(
            DetectedObject::new("flower", 0.85)
                .at_timestamp(Timestamp::new(3.0, 90)),
        );

        let mut audio = empty_audio();
        audio.events.push(AudioEvent::new("scream", 0.9, Timestamp::new(3.2, 96)));

        let fusion = CrossModalFusion::new();
        let contradictions = fusion.detect_contradictions(&visual, &audio);

        assert!(!contradictions.is_empty());
        assert!(contradictions.iter().any(|c| c.contradiction_type
            == ContradictionType::PeacefulVisualAggressiveAudio));
    }

    #[test]
    fn detect_contradiction_comedic_audio_serious_visual() {
        let mut visual = empty_visual();
        visual.objects.push(
            DetectedObject::new("disaster", 0.85)
                .at_timestamp(Timestamp::new(10.0, 300)),
        );

        let mut audio = empty_audio();
        audio.events.push(AudioEvent::new("laughter", 0.8, Timestamp::new(10.1, 303)));

        let fusion = CrossModalFusion::new();
        let contradictions = fusion.detect_contradictions(&visual, &audio);

        assert!(!contradictions.is_empty());
        assert!(contradictions.iter().any(|c| c.contradiction_type
            == ContradictionType::ComedicAudioSeriousVisual));
    }

    #[test]
    fn contradiction_resolution_labels() {
        assert_eq!(ContradictionResolution::FavorVisual.label(), "favor_visual");
        assert_eq!(ContradictionResolution::FavorAudio.label(), "favor_audio");
        assert_eq!(
            ContradictionResolution::ArtisticContext.label(),
            "artistic_context"
        );
        assert_eq!(
            ContradictionResolution::RequiresReview.label(),
            "requires_review"
        );
    }

    #[test]
    fn fused_signals_counts_resolutions() {
        let signals = FusedSignals {
            elements: Vec::new(),
            contradictions: vec![
                ModalContradiction::new(ContradictionType::ComedicAudioSeriousVisual, 1.0, 0.5),
                ModalContradiction::new(ContradictionType::ComedicAudioSeriousVisual, 2.0, 0.5)
                    .with_resolution(ContradictionResolution::ArtisticContext),
                ModalContradiction::new(ContradictionType::SexualVisualSeriousAudio, 3.0, 0.5),
            ],
            fused_risk_score: 0.0,
            visual_signal_count: 0,
            audio_signal_count: 0,
            artistic_resolutions: 0,
            review_required_count: 0,
        };
        assert_eq!(
            signals
                .contradictions
                .iter()
                .filter(|c| c.resolution == ContradictionResolution::ArtisticContext)
                .count(),
            1
        );
        assert_eq!(
            signals
                .contradictions
                .iter()
                .filter(|c| c.resolution == ContradictionResolution::RequiresReview)
                .count(),
            1
        );
    }
}
