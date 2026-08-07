//! Protobuf-equivalent Rust types for the Sentinel gRPC API.
//!
//! Since we cannot run `protoc` in this build environment, we define the
//! generated types manually and provide conversion traits to/from the
//! internal `sentinel-core` domain types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Overall status of an analysis job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisStatus {
    Pending,
    ExtractingFrames,
    AnalyzingVisual,
    AnalyzingAudio,
    AnalyzingMetadata,
    Synthesizing,
    ApplyingPolicy,
    RunningAutoFix,
    ValidatingFixes,
    Completed,
    Failed,
    Cancelled,
}

/// Severity level for a detected issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Category of content policy violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationCategory {
    InappropriateVisual,
    HateSpeech,
    Harassment,
    Copyright,
    Spam,
    Misinformation,
    GraphicContent,
    DangerousContent,
    ChildSafety,
    Privacy,
    FlashingContent,
    AudioMismatch,
    MetadataIssue,
    Other,
}

// ---------------------------------------------------------------------------
// Shared message types
// ---------------------------------------------------------------------------

/// Where the video came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSource {
    pub source_id: String,
    pub source_type: String, // "youtube", "upload", "url"
    pub url: String,
    pub metadata: HashMap<String, String>,
}

/// A timestamp in seconds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Timestamp {
    pub seconds: f64,
}

/// 2D bounding box in normalised coordinates [0, 1].
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

// ---------------------------------------------------------------------------
// Analysis result sub-messages
// ---------------------------------------------------------------------------

/// Object detected by the visual analysis engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObject {
    pub label: String,
    pub confidence: f32,
    pub bbox: Option<BoundingBox>,
    pub frame_index: u64,
    pub timestamp: Timestamp,
}

/// Text detected by OCR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedText {
    pub text: String,
    pub confidence: f32,
    pub bbox: Option<BoundingBox>,
    pub frame_index: u64,
    pub timestamp: Timestamp,
}

/// A scene transition event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneTransition {
    pub frame_index: u64,
    pub timestamp: Timestamp,
    pub transition_type: String, // "cut", "fade", "dissolve"
    pub confidence: f32,
}

/// A segment with flashing / rapid luminance changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashingSegment {
    pub start: Timestamp,
    pub end: Timestamp,
    pub frequency_hz: f32,
    pub max_luminance_delta: f32,
    pub risk_score: f32,
}

/// A transcript segment from speech recognition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start: Timestamp,
    pub end: Timestamp,
    pub text: String,
    pub confidence: f32,
    pub language: String,
    pub speaker_id: Option<String>,
}

/// An audio event detected by the audio analysis engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEvent {
    pub event_type: String,
    pub start: Timestamp,
    pub end: Timestamp,
    pub confidence: f32,
    pub source_track: String, // "music", "speech", "sfx"
    pub related_transcript_idx: Option<usize>,
}

/// A copyright match (audio or visual).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyrightMatch {
    pub match_type: String, // "audio", "visual"
    pub start: Timestamp,
    pub end: Timestamp,
    pub confidence: f32,
    pub reference_id: String,
    pub reference_title: String,
    pub reference_owner: String,
    pub claim_policy: String,
}

// ---------------------------------------------------------------------------
// Top-level analysis messages
// ---------------------------------------------------------------------------

/// Result of the visual analysis stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisualAnalysis {
    pub objects: Vec<DetectedObject>,
    pub detected_text: Vec<DetectedText>,
    pub scene_transitions: Vec<SceneTransition>,
    pub flashing_segments: Vec<FlashingSegment>,
    pub frame_count: u64,
    pub duration_seconds: f64,
    pub summary: String,
}

/// Result of the audio analysis stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioAnalysis {
    pub transcript: Vec<TranscriptSegment>,
    pub audio_events: Vec<AudioEvent>,
    pub copyright_matches: Vec<CopyrightMatch>,
    pub language: String,
    pub duration_seconds: f64,
    pub summary: String,
}

/// Result of the metadata analysis stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataAnalysis {
    pub title_issues: Vec<String>,
    pub description_issues: Vec<String>,
    pub tag_issues: Vec<String>,
    pub category_mismatch: Vec<String>,
    pub age_gating_issues: Vec<String>,
    pub thumbnails_issues: Vec<String>,
    pub summary: String,
}

// ---------------------------------------------------------------------------
// Violation & synthesis
// ---------------------------------------------------------------------------

/// A single policy violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub violation_id: String,
    pub category: ViolationCategory,
    pub severity: Severity,
    pub start: Timestamp,
    pub end: Timestamp,
    pub description: String,
    pub recommendation: String,
    pub source_stage: String, // "visual", "audio", "metadata", "synthesis"
    pub evidence_json: String,
}

/// Result of the synthesis stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub violations: Vec<Violation>,
    pub overall_risk_score: f32,
    pub risk_level: String,
    pub summary: String,
    pub suggested_actions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Auto-fix
// ---------------------------------------------------------------------------

/// A requested or applied auto-fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoFix {
    pub fix_id: String,
    pub fix_type: String, // "blur", "mute", "trim", "replace_audio", "add_warning"
    pub target_start: Timestamp,
    pub target_end: Timestamp,
    pub description: String,
    pub parameters_json: String,
}

/// Result of applying a fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    pub fix_id: String,
    pub applied: bool,
    pub output_path: String,
    pub processing_time_ms: u64,
    pub error_message: String,
}

// ---------------------------------------------------------------------------
// Progress & report
// ---------------------------------------------------------------------------

/// A progress update streamed to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub job_id: String,
    pub status: AnalysisStatus,
    pub current_stage: u32,
    pub total_stages: u32,
    pub stage_name: String,
    pub progress_pct: f32, // 0.0 .. 100.0
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub detail_json: String,
}

/// The final analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub job_id: String,
    pub status: AnalysisStatus,
    pub video_source: VideoSource,
    pub visual_analysis: Option<VisualAnalysis>,
    pub audio_analysis: Option<AudioAnalysis>,
    pub metadata_analysis: Option<MetadataAnalysis>,
    pub synthesis: Option<SynthesisResult>,
    pub fixes_applied: Vec<FixResult>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub processing_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Request / Response messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitRequest {
    pub video_source: VideoSource,
    pub config_json: String,
    pub enable_autofix: bool,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub job_id: String,
    pub status: AnalysisStatus,
    pub message: String,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixRequest {
    pub job_id: String,
    pub fixes: Vec<AutoFix>,
    pub apply_automatically: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResponse {
    pub job_id: String,
    pub results: Vec<FixResult>,
    pub all_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRequest {
    pub source: VideoSource,
    pub fps: f32,
    pub max_resolution: u32,
    pub extract_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameBatch {
    pub batch_index: u32,
    pub frames: Vec<FrameInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameInfo {
    pub frame_index: u64,
    pub timestamp: Timestamp,
    pub data_base64: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRequest {
    pub job_id: String,
    pub audio_path: String,
}

// ---------------------------------------------------------------------------
// Conversions to/from sentinel-core types
// ---------------------------------------------------------------------------

use sentinel_core::{AnalysisConfig, JobId, VideoMetadata};

impl From<JobId> for String {
    fn from(id: JobId) -> String {
        id.to_string()
    }
}

impl Timestamp {
    pub fn from_seconds(s: f64) -> Self {
        Self { seconds: s }
    }

    pub fn to_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(self.seconds)
    }
}

impl VideoSource {
    /// Convert to sentinel-core `VideoMetadata`.
    pub fn to_metadata(&self) -> VideoMetadata {
        VideoMetadata {
            source_id: self.source_id.clone(),
            source_type: self.source_type.clone(),
            url: self.url.clone(),
            metadata: self.metadata.clone(),
        }
    }

    pub fn from_metadata(meta: &VideoMetadata) -> Self {
        Self {
            source_id: meta.source_id.clone(),
            source_type: meta.source_type.clone(),
            url: meta.url.clone(),
            metadata: meta.metadata.clone(),
        }
    }
}

impl SubmitRequest {
    /// Parse the JSON config blob into a typed `AnalysisConfig`.
    pub fn parse_config(&self) -> anyhow::Result<AnalysisConfig> {
        if self.config_json.is_empty() {
            Ok(AnalysisConfig::default())
        } else {
            let cfg: AnalysisConfig = serde_json::from_str(&self.config_json)?;
            Ok(cfg)
        }
    }
}

impl AnalysisStatus {
    /// Human-readable name used in progress updates.
    pub fn stage_name(&self) -> &'static str {
        match self {
            AnalysisStatus::Pending => "Pending",
            AnalysisStatus::ExtractingFrames => "Extracting Frames",
            AnalysisStatus::AnalyzingVisual => "Visual Analysis",
            AnalysisStatus::AnalyzingAudio => "Audio Analysis",
            AnalysisStatus::AnalyzingMetadata => "Metadata Analysis",
            AnalysisStatus::Synthesizing => "Synthesizing Results",
            AnalysisStatus::ApplyingPolicy => "Applying Policies",
            AnalysisStatus::RunningAutoFix => "Running Auto-Fix",
            AnalysisStatus::ValidatingFixes => "Validating Fixes",
            AnalysisStatus::Completed => "Completed",
            AnalysisStatus::Failed => "Failed",
            AnalysisStatus::Cancelled => "Cancelled",
        }
    }

    pub fn stage_number(&self) -> u32 {
        match self {
            AnalysisStatus::Pending => 0,
            AnalysisStatus::ExtractingFrames => 1,
            AnalysisStatus::AnalyzingVisual => 2,
            AnalysisStatus::AnalyzingAudio => 3,
            AnalysisStatus::AnalyzingMetadata => 4,
            AnalysisStatus::Synthesizing => 5,
            AnalysisStatus::ApplyingPolicy => 6,
            AnalysisStatus::RunningAutoFix => 7,
            AnalysisStatus::ValidatingFixes => 8,
            AnalysisStatus::Completed => 9,
            AnalysisStatus::Failed => 9,
            AnalysisStatus::Cancelled => 9,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_roundtrip() {
        let ts = Timestamp::from_seconds(123.456);
        assert!((ts.seconds - 123.456).abs() < f64::EPSILON);
    }

    #[test]
    fn test_analysis_status_stage_name() {
        assert_eq!(AnalysisStatus::Pending.stage_name(), "Pending");
        assert_eq!(AnalysisStatus::Completed.stage_name(), "Completed");
    }

    #[test]
    fn test_video_source_to_metadata() {
        let vs = VideoSource {
            source_id: "vid-1".into(),
            source_type: "youtube".into(),
            url: "https://youtu.be/abc".into(),
            metadata: {
                let mut m = HashMap::new();
                m.insert("title".into(), "Test".into());
                m
            },
        };
        let meta = vs.to_metadata();
        assert_eq!(meta.source_id, "vid-1");
        assert_eq!(meta.source_type, "youtube");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let update = ProgressUpdate {
            job_id: "job-1".into(),
            status: AnalysisStatus::AnalyzingVisual,
            current_stage: 2,
            total_stages: 9,
            stage_name: "Visual Analysis".into(),
            progress_pct: 45.5,
            message: "Running object detection".into(),
            timestamp: Utc::now(),
            detail_json: "{}".into(),
        };
        let json = serde_json::to_string(&update).unwrap();
        let back: ProgressUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.job_id, "job-1");
        assert_eq!(back.current_stage, 2);
        assert!((back.progress_pct - 45.5).abs() < f32::EPSILON);
    }
}
