use serde::{Deserialize, Serialize};

// ===========================================================================
// Enums
// ===========================================================================

/// High-level status of an analysis job as it flows through the pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnalysisStatus {
    Pending,
    ExtractingFrames,
    AnalyzingVisual,
    AnalyzingAudio,
    AnalyzingMetadata,
    Synthesizing,
    ApplyingPolicies,
    AutoFixing,
    Validating,
    Completed,
    Failed,
}

impl Default for AnalysisStatus {
    fn default() -> Self {
        AnalysisStatus::Pending
    }
}

impl AnalysisStatus {
    /// Returns `true` when the status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, AnalysisStatus::Completed | AnalysisStatus::Failed)
    }

    /// Human-readable label for dashboard / log output.
    pub fn label(&self) -> &'static str {
        match self {
            AnalysisStatus::Pending => "Pending",
            AnalysisStatus::ExtractingFrames => "Extracting Frames",
            AnalysisStatus::AnalyzingVisual => "Analyzing Visual",
            AnalysisStatus::AnalyzingAudio => "Analyzing Audio",
            AnalysisStatus::AnalyzingMetadata => "Analyzing Metadata",
            AnalysisStatus::Synthesizing => "Synthesizing",
            AnalysisStatus::ApplyingPolicies => "Applying Policies",
            AnalysisStatus::AutoFixing => "Auto-Fixing",
            AnalysisStatus::Validating => "Validating",
            AnalysisStatus::Completed => "Completed",
            AnalysisStatus::Failed => "Failed",
        }
    }
}

/// Severity level attached to a single violation or overall risk score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Severity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl Default for Severity {
    fn default() -> Self {
        Severity::None
    }
}

impl Severity {
    /// Numeric weight used when aggregating scores.
    pub fn weight(&self) -> f64 {
        match self {
            Severity::None => 0.0,
            Severity::Low => 1.0,
            Severity::Medium => 3.0,
            Severity::High => 6.0,
            Severity::Critical => 10.0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Severity::None => "none",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

/// Category of policy violation detected in the content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViolationCategory {
    Unspecified,
    HateSpeech,
    Harassment,
    Violence,
    AdultContent,
    HarmfulDangerous,
    Misinformation,
    ChildSafety,
    Copyright,
    SpamDeceptive,
    FlashingSeizure,
    ThumbnailIssue,
}

impl Default for ViolationCategory {
    fn default() -> Self {
        ViolationCategory::Unspecified
    }
}

impl ViolationCategory {
    pub fn label(&self) -> &'static str {
        match self {
            ViolationCategory::Unspecified => "unspecified",
            ViolationCategory::HateSpeech => "hate_speech",
            ViolationCategory::Harassment => "harassment",
            ViolationCategory::Violence => "violence",
            ViolationCategory::AdultContent => "adult_content",
            ViolationCategory::HarmfulDangerous => "harmful_dangerous",
            ViolationCategory::Misinformation => "misinformation",
            ViolationCategory::ChildSafety => "child_safety",
            ViolationCategory::Copyright => "copyright",
            ViolationCategory::SpamDeceptive => "spam_deceptive",
            ViolationCategory::FlashingSeizure => "flashing_seizure",
            ViolationCategory::ThumbnailIssue => "thumbnail_issue",
        }
    }
}

/// Contextual framing that can modulate the severity of detected content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContextType {
    Educational,
    Documentary,
    Satire,
    News,
    Entertainment,
    Artistic,
    Gratuitous,
    Ambiguous,
}

impl ContextType {
    pub fn label(&self) -> &'static str {
        match self {
            ContextType::Educational => "educational",
            ContextType::Documentary => "documentary",
            ContextType::Satire => "satire",
            ContextType::News => "news",
            ContextType::Entertainment => "entertainment",
            ContextType::Artistic => "artistic",
            ContextType::Gratuitous => "gratuitous",
            ContextType::Ambiguous => "ambiguous",
        }
    }

    /// Modifier in the range `[-1.0, 1.0]` applied to risk scores.
    /// Negative values reduce perceived risk (educational / documentary),
    /// positive values increase it (gratuitous).
    pub fn risk_modifier(&self) -> f64 {
        match self {
            ContextType::Educational => -0.4,
            ContextType::Documentary => -0.3,
            ContextType::Satire => -0.2,
            ContextType::News => -0.1,
            ContextType::Entertainment => 0.0,
            ContextType::Artistic => -0.1,
            ContextType::Gratuitous => 0.5,
            ContextType::Ambiguous => 0.1,
        }
    }
}

// ===========================================================================
// Structs
// ===========================================================================

/// Identifies the video under analysis and carries its basic metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoSource {
    pub video_id: String,
    pub file_path: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub thumbnail_path: String,
    pub duration_seconds: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

impl VideoSource {
    pub fn new<S: Into<String>>(video_id: S, file_path: S) -> Self {
        VideoSource {
            video_id: video_id.into(),
            file_path: file_path.into(),
            title: String::new(),
            description: String::new(),
            tags: Vec::new(),
            thumbnail_path: String::new(),
            duration_seconds: 0.0,
            width: 0,
            height: 0,
            fps: 0.0,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn resolution(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// A point in the video timeline expressed in both seconds and frame number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timestamp {
    pub seconds: f64,
    pub frame_number: u64,
}

impl Timestamp {
    pub fn new(seconds: f64, frame_number: u64) -> Self {
        Timestamp {
            seconds,
            frame_number,
        }
    }

    /// Format seconds as `HH:MM:SS.mmm`.
    pub fn formatted(&self) -> String {
        let total_secs = self.seconds as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        let millis = ((self.seconds.fract()) * 1000.0) as u32;
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, secs, millis)
    }
}

/// Normalized bounding box (`0.0..=1.0`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl BoundingBox {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        BoundingBox { x1, y1, x2, y2 }
    }

    pub fn width(&self) -> f64 {
        (self.x2 - self.x1).abs()
    }

    pub fn height(&self) -> f64 {
        (self.y2 - self.y1).abs()
    }

    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    pub fn is_valid(&self) -> bool {
        self.x1 >= 0.0
            && self.y1 >= 0.0
            && self.x2 <= 1.0
            && self.y2 <= 1.0
            && self.x2 > self.x1
            && self.y2 > self.y1
    }
}

/// An object detected by the visual analysis model (e.g. "gun", "person").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedObject {
    pub label: String,
    pub confidence: f64,
    pub bbox: BoundingBox,
    pub timestamp: Timestamp,
    pub category: ViolationCategory,
    pub context: ContextType,
}

impl DetectedObject {
    pub fn new<S: Into<String>>(label: S, confidence: f64) -> Self {
        DetectedObject {
            label: label.into(),
            confidence,
            bbox: BoundingBox::new(0.0, 0.0, 1.0, 1.0),
            timestamp: Timestamp::new(0.0, 0),
            category: ViolationCategory::Unspecified,
            context: ContextType::Ambiguous,
        }
    }

    pub fn with_bbox(mut self, bbox: BoundingBox) -> Self {
        self.bbox = bbox;
        self
    }

    pub fn at_timestamp(mut self, ts: Timestamp) -> Self {
        self.timestamp = ts;
        self
    }

    pub fn with_category(mut self, cat: ViolationCategory) -> Self {
        self.category = cat;
        self
    }

    pub fn with_context(mut self, ctx: ContextType) -> Self {
        self.context = ctx;
        self
    }
}

/// Text detected inside a frame via OCR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedText {
    pub text: String,
    pub confidence: f64,
    pub bbox: BoundingBox,
    pub timestamp: Timestamp,
    pub is_obscured: bool,
    pub is_profanity: bool,
}

impl DetectedText {
    pub fn new<S: Into<String>>(text: S, confidence: f64) -> Self {
        DetectedText {
            text: text.into(),
            confidence,
            bbox: BoundingBox::new(0.0, 0.0, 1.0, 1.0),
            timestamp: Timestamp::new(0.0, 0),
            is_obscured: false,
            is_profanity: false,
        }
    }

    pub fn with_bbox(mut self, bbox: BoundingBox) -> Self {
        self.bbox = bbox;
        self
    }

    pub fn at_timestamp(mut self, ts: Timestamp) -> Self {
        self.timestamp = ts;
        self
    }

    pub fn obscured(mut self) -> Self {
        self.is_obscured = true;
        self
    }

    pub fn with_profanity(mut self) -> Self {
        self.is_profanity = true;
        self
    }
}

/// A scene transition detected in the video stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTransition {
    pub timestamp: Timestamp,
    pub severity: Severity,
    pub transition_type: String,
}

impl SceneTransition {
    pub fn new<S: Into<String>>(transition_type: S, timestamp: Timestamp, severity: Severity) -> Self {
        SceneTransition {
            timestamp,
            severity,
            transition_type: transition_type.into(),
        }
    }
}

/// A segment of the video containing flashing imagery that may trigger
/// photosensitive epilepsy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashingSegment {
    pub start: Timestamp,
    pub end: Timestamp,
    pub frequency_hz: f64,
    pub severity: Severity,
}

impl FlashingSegment {
    pub fn duration_seconds(&self) -> f64 {
        (self.end.seconds - self.start.seconds).max(0.0)
    }
}

/// A single transcript segment produced by the ASR pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: Timestamp,
    pub end: Timestamp,
    pub text: String,
    pub confidence: f64,
    pub sentiment: f64,
    pub is_sarcasm: bool,
    pub is_background_speech: bool,
}

impl TranscriptSegment {
    pub fn duration_seconds(&self) -> f64 {
        (self.end.seconds - self.start.seconds).max(0.0)
    }
}

/// An acoustic event detected in the audio stream (e.g. gunshot, scream).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioEvent {
    pub timestamp: Timestamp,
    pub event_type: String,
    pub confidence: f64,
    pub volume_db: f64,
}

impl AudioEvent {
    pub fn new<S: Into<String>>(event_type: S, confidence: f64, timestamp: Timestamp) -> Self {
        AudioEvent {
            timestamp,
            event_type: event_type.into(),
            confidence,
            volume_db: 0.0,
        }
    }
}

/// A match against a known copyrighted work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyrightMatch {
    pub matched_work: String,
    pub copyright_holder: String,
    pub match_confidence: f64,
    pub start: Timestamp,
    pub end: Timestamp,
    pub match_type: String,
}

impl CopyrightMatch {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        matched_work: S1,
        copyright_holder: S2,
    ) -> Self {
        CopyrightMatch {
            matched_work: matched_work.into(),
            copyright_holder: copyright_holder.into(),
            match_confidence: 0.0,
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(0.0, 0),
            match_type: String::new(),
        }
    }

    pub fn with_confidence(mut self, c: f64) -> Self {
        self.match_confidence = c;
        self
    }

    pub fn with_span(mut self, start: Timestamp, end: Timestamp) -> Self {
        self.start = start;
        self.end = end;
        self
    }

    pub fn with_match_type<S: Into<String>>(mut self, t: S) -> Self {
        self.match_type = t.into();
        self
    }
}

// ===========================================================================
// Analysis result structs
// ===========================================================================

/// Results produced by the visual-analysis stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnalysis {
    pub objects: Vec<DetectedObject>,
    pub texts: Vec<DetectedText>,
    pub transitions: Vec<SceneTransition>,
    pub flashing: Vec<FlashingSegment>,
    pub overall_risk_score: f64,
}

impl VisualAnalysis {
    pub fn empty() -> Self {
        VisualAnalysis {
            objects: Vec::new(),
            texts: Vec::new(),
            transitions: Vec::new(),
            flashing: Vec::new(),
            overall_risk_score: 0.0,
        }
    }

    /// Aggregate risk score from objects and flashing segments.
    pub fn compute_risk_score(&mut self) {
        let obj_score: f64 = self
            .objects
            .iter()
            .map(|o| o.confidence * o.category.risk_modifier().abs())
            .sum();
        let flash_score: f64 = self
            .flashing
            .iter()
            .map(|f| f.frequency_hz * f.severity.weight())
            .sum();
        self.overall_risk_score = (obj_score + flash_score).min(100.0).max(0.0);
    }
}

/// Results produced by the audio-analysis stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAnalysis {
    pub transcript: Vec<TranscriptSegment>,
    pub events: Vec<AudioEvent>,
    pub copyright_matches: Vec<CopyrightMatch>,
    pub overall_risk_score: f64,
}

impl AudioAnalysis {
    pub fn empty() -> Self {
        AudioAnalysis {
            transcript: Vec::new(),
            events: Vec::new(),
            copyright_matches: Vec::new(),
            overall_risk_score: 0.0,
        }
    }

    pub fn compute_risk_score(&mut self) {
        let event_score: f64 = self
            .events
            .iter()
            .map(|e| e.confidence)
            .sum::<f64>()
            .min(50.0);
        let copyright_score: f64 = self
            .copyright_matches
            .iter()
            .map(|c| c.match_confidence * 10.0)
            .sum::<f64>()
            .min(50.0);
        self.overall_risk_score = (event_score + copyright_score).min(100.0).max(0.0);
    }
}

/// Results produced by the metadata-analysis stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataAnalysis {
    pub clickbait_score: f64,
    pub keyword_stuffing_score: f64,
    pub misleading_score: f64,
    pub flagged_keywords: Vec<String>,
    pub suggestions: Vec<String>,
    pub overall_risk_score: f64,
}

impl MetadataAnalysis {
    pub fn empty() -> Self {
        MetadataAnalysis {
            clickbait_score: 0.0,
            keyword_stuffing_score: 0.0,
            misleading_score: 0.0,
            flagged_keywords: Vec::new(),
            suggestions: Vec::new(),
            overall_risk_score: 0.0,
        }
    }

    pub fn compute_risk_score(&mut self) {
        self.overall_risk_score =
            (self.clickbait_score + self.keyword_stuffing_score + self.misleading_score)
                .min(100.0)
                .max(0.0);
    }
}

// ===========================================================================
// Synthesis & violation structs
// ===========================================================================

/// A single detected policy violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    pub category: ViolationCategory,
    pub severity: Severity,
    pub description: String,
    pub timestamp: Timestamp,
    pub confidence: f64,
    pub evidence: String,
    pub guideline_reference: String,
}

impl Violation {
    pub fn new<S: Into<String>>(description: S) -> Self {
        Violation {
            category: ViolationCategory::Unspecified,
            severity: Severity::Low,
            description: description.into(),
            timestamp: Timestamp::new(0.0, 0),
            confidence: 0.0,
            evidence: String::new(),
            guideline_reference: String::new(),
        }
    }

    pub fn with_category(mut self, c: ViolationCategory) -> Self {
        self.category = c;
        self
    }

    pub fn with_severity(mut self, s: Severity) -> Self {
        self.severity = s;
        self
    }

    pub fn at_timestamp(mut self, ts: Timestamp) -> Self {
        self.timestamp = ts;
        self
    }

    pub fn with_confidence(mut self, c: f64) -> Self {
        self.confidence = c.clamp(0.0, 1.0);
        self
    }

    pub fn with_evidence<S: Into<String>>(mut self, e: S) -> Self {
        self.evidence = e.into();
        self
    }

    pub fn with_guideline<S: Into<String>>(mut self, g: S) -> Self {
        self.guideline_reference = g.into();
        self
    }
}

/// An automated fix that can be applied to the video or metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoFix {
    pub fix_id: String,
    pub fix_type: String,
    pub target_start: Timestamp,
    pub target_end: Timestamp,
    pub parameters: std::collections::HashMap<String, String>,
    pub estimated_effectiveness: f64,
    pub description: String,
}

impl AutoFix {
    pub fn new<S1: Into<String>, S2: Into<String>>(
        fix_id: S1,
        fix_type: S2,
        description: S2,
    ) -> Self {
        AutoFix {
            fix_id: fix_id.into(),
            fix_type: fix_type.into(),
            target_start: Timestamp::new(0.0, 0),
            target_end: Timestamp::new(0.0, 0),
            parameters: std::collections::HashMap::new(),
            estimated_effectiveness: 0.0,
            description: description.into(),
        }
    }

    pub fn with_span(mut self, start: Timestamp, end: Timestamp) -> Self {
        self.target_start = start;
        self.target_end = end;
        self
    }

    pub fn with_effectiveness(mut self, e: f64) -> Self {
        self.estimated_effectiveness = e.clamp(0.0, 1.0);
        self
    }

    pub fn with_parameter<S1: Into<String>, S2: Into<String>>(
        mut self,
        key: S1,
        value: S2,
    ) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }
}

/// Outcome after applying an `AutoFix`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixResult {
    pub fix_id: String,
    pub success: bool,
    pub output_path: String,
    pub safety_margin: f64,
    pub confidence: f64,
}

impl FixResult {
    pub fn new<S: Into<String>>(fix_id: S, success: bool) -> Self {
        FixResult {
            fix_id: fix_id.into(),
            success,
            output_path: String::new(),
            safety_margin: 0.0,
            confidence: 0.0,
        }
    }
}

// ===========================================================================
// Synthesis & progress
// ===========================================================================

/// Final synthesis combining all analysis stages into a single decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub category_scores: std::collections::HashMap<ViolationCategory, f64>,
    pub violations: Vec<Violation>,
    pub contexts: Vec<ContextType>,
    pub total_risk_score: f64,
    pub recommendation: String,
}

impl SynthesisResult {
    pub fn empty() -> Self {
        SynthesisResult {
            category_scores: std::collections::HashMap::new(),
            violations: Vec::new(),
            contexts: Vec::new(),
            total_risk_score: 0.0,
            recommendation: String::from("pending"),
        }
    }

    /// Compute the total risk score from category scores and violations.
    pub fn compute_total_risk(&mut self) {
        let cat_score: f64 = self.category_scores.values().copied().sum();
        let violation_score: f64 = self
            .violations
            .iter()
            .map(|v| v.confidence * v.severity.weight())
            .sum();
        self.total_risk_score = (cat_score + violation_score).min(100.0).max(0.0);
    }

    /// Generate a human-readable recommendation based on the total risk score.
    pub fn generate_recommendation(&mut self) {
        self.recommendation = if self.total_risk_score >= 80.0 {
            "reject".into()
        } else if self.total_risk_score >= 50.0 {
            "manual_review".into()
        } else if self.total_risk_score >= 20.0 {
            "approve_with_caution".into()
        } else {
            "approve".into()
        };
    }
}

/// A progress update emitted by the pipeline for real-time monitoring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub status: AnalysisStatus,
    pub progress_percent: f64,
    pub current_stage: String,
    pub message: String,
}

impl ProgressUpdate {
    pub fn new<S: Into<String>>(status: AnalysisStatus, current_stage: S, message: S) -> Self {
        ProgressUpdate {
            status,
            progress_percent: 0.0,
            current_stage: current_stage.into(),
            message: message.into(),
        }
    }

    pub fn with_percent(mut self, p: f64) -> Self {
        self.progress_percent = p.clamp(0.0, 100.0);
        self
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- Enums ---

    #[test]
    fn analysis_status_terminal() {
        assert!(!AnalysisStatus::Pending.is_terminal());
        assert!(!AnalysisStatus::AnalyzingVisual.is_terminal());
        assert!(AnalysisStatus::Completed.is_terminal());
        assert!(AnalysisStatus::Failed.is_terminal());
    }

    #[test]
    fn severity_weight_ordering() {
        assert!(Severity::Critical.weight() > Severity::High.weight());
        assert!(Severity::High.weight() > Severity::Medium.weight());
        assert!(Severity::Medium.weight() > Severity::Low.weight());
        assert_eq!(Severity::None.weight(), 0.0);
    }

    #[test]
    fn context_type_risk_modifier() {
        assert!(ContextType::Gratuitous.risk_modifier() > 0.0);
        assert!(ContextType::Educational.risk_modifier() < 0.0);
        assert_eq!(ContextType::Entertainment.risk_modifier(), 0.0);
    }

    // --- Structs ---

    #[test]
    fn video_source_resolution() {
        let vs = VideoSource::new("abc123", "/tmp/v.mp4")
            .with_title("Test Video");
        assert_eq!(vs.video_id, "abc123");
        assert_eq!(vs.resolution(), "0x0");
    }

    #[test]
    fn timestamp_formatting() {
        let ts = Timestamp::new(3661.123, 100);
        assert_eq!(ts.formatted(), "01:01:01.123");
    }

    #[test]
    fn bounding_box_valid() {
        let bb = BoundingBox::new(0.1, 0.1, 0.9, 0.9);
        assert!(bb.is_valid());
        assert_eq!(bb.area(), 0.64);
    }

    #[test]
    fn bounding_box_invalid() {
        let bb = BoundingBox::new(0.9, 0.1, 0.1, 0.9);
        assert!(!bb.is_valid());
    }

    #[test]
    fn detected_object_builder() {
        let obj = DetectedObject::new("weapon", 0.92)
            .with_bbox(BoundingBox::new(0.2, 0.2, 0.8, 0.8))
            .with_category(ViolationCategory::Violence);
        assert_eq!(obj.label, "weapon");
        assert!(obj.bbox.is_valid());
        assert_eq!(obj.category.label(), "violence");
    }

    #[test]
    fn flashing_segment_duration() {
        let fs = FlashingSegment {
            start: Timestamp::new(1.0, 30),
            end: Timestamp::new(3.5, 105),
            frequency_hz: 8.0,
            severity: Severity::High,
        };
        assert!((fs.duration_seconds() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn copyright_match_builder() {
        let cm = CopyrightMatch::new("Song X", "Label Y")
            .with_confidence(0.85)
            .with_match_type("audio_fingerprint")
            .with_span(Timestamp::new(10.0, 300), Timestamp::new(15.0, 450));
        assert_eq!(cm.matched_work, "Song X");
        assert_eq!(cm.match_type, "audio_fingerprint");
        assert!((cm.match_confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn visual_analysis_risk_score() {
        let mut va = VisualAnalysis::empty();
        va.objects.push(
            DetectedObject::new("weapon", 0.9)
                .with_category(ViolationCategory::Violence),
        );
        va.flashing.push(FlashingSegment {
            start: Timestamp::new(0.0, 0),
            end: Timestamp::new(1.0, 30),
            frequency_hz: 10.0,
            severity: Severity::Critical,
        });
        va.compute_risk_score();
        assert!(va.overall_risk_score > 0.0);
        assert!(va.overall_risk_score <= 100.0);
    }

    #[test]
    fn synthesis_result_recommendation() {
        let mut sr = SynthesisResult::empty();
        sr.total_risk_score = 85.0;
        sr.generate_recommendation();
        assert_eq!(sr.recommendation, "reject");

        sr.total_risk_score = 60.0;
        sr.generate_recommendation();
        assert_eq!(sr.recommendation, "manual_review");

        sr.total_risk_score = 30.0;
        sr.generate_recommendation();
        assert_eq!(sr.recommendation, "approve_with_caution");

        sr.total_risk_score = 5.0;
        sr.generate_recommendation();
        assert_eq!(sr.recommendation, "approve");
    }

    #[test]
    fn progress_update_percent_clamp() {
        let pu = ProgressUpdate::new(AnalysisStatus::AnalyzingVisual, "vis", "msg").with_percent(150.0);
        assert_eq!(pu.progress_percent, 100.0);

        let pu2 = ProgressUpdate::new(AnalysisStatus::Pending, "p", "m").with_percent(-10.0);
        assert_eq!(pu2.progress_percent, 0.0);
    }

    #[test]
    fn violation_confidence_clamp() {
        let v = Violation::new("test")
            .with_confidence(1.5)
            .with_severity(Severity::Critical);
        assert_eq!(v.confidence, 1.0);
    }

    #[test]
    fn serde_roundtrip_video_source() {
        let vs = VideoSource::new("vid1", "/tmp/v.mp4")
            .with_title("Hello");
        let json = serde_json::to_string(&vs).unwrap();
        let back: VideoSource = serde_json::from_str(&json).unwrap();
        assert_eq!(vs.video_id, back.video_id);
        assert_eq!(vs.title, back.title);
    }

    #[test]
    fn serde_roundtrip_analysis_status() {
        let status = AnalysisStatus::AnalyzingAudio;
        let json = serde_json::to_string(&status).unwrap();
        let back: AnalysisStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, back);
    }

    #[test]
    fn violation_category_label_coverage() {
        let cats = vec![
            ViolationCategory::Unspecified,
            ViolationCategory::HateSpeech,
            ViolationCategory::Harassment,
            ViolationCategory::Violence,
            ViolationCategory::AdultContent,
            ViolationCategory::HarmfulDangerous,
            ViolationCategory::Misinformation,
            ViolationCategory::ChildSafety,
            ViolationCategory::Copyright,
            ViolationCategory::SpamDeceptive,
            ViolationCategory::FlashingSeizure,
            ViolationCategory::ThumbnailIssue,
        ];
        for cat in cats {
            assert!(!cat.label().is_empty());
        }
    }

    #[test]
    fn auto_fix_builder() {
        let fix = AutoFix::new("fix-1", "blur", "Apply Gaussian blur")
            .with_span(Timestamp::new(0.0, 0), Timestamp::new(5.0, 150))
            .with_effectiveness(0.95)
            .with_parameter("radius", "10");
        assert_eq!(fix.fix_id, "fix-1");
        assert_eq!(fix.parameters.get("radius"), Some(&"10".to_string()));
    }

    #[test]
    fn context_type_all_labels_unique() {
        let labels: Vec<_> = vec![
            ContextType::Educational,
            ContextType::Documentary,
            ContextType::Satire,
            ContextType::News,
            ContextType::Entertainment,
            ContextType::Artistic,
            ContextType::Gratuitous,
            ContextType::Ambiguous,
        ]
        .iter()
        .map(|c| c.label())
        .collect();
        let mut uniq = labels.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(labels.len(), uniq.len());
    }
}
