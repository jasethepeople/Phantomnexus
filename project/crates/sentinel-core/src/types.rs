use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for tracked items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub Uuid);

impl TrackId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TrackId {
    fn default() -> Self {
        Self::new()
    }
}

/// A rectangular bounding box in normalized coordinates (0.0 - 1.0)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl BoundingBox {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    pub fn to_pixel_coords(&self, frame_width: u32, frame_height: u32) -> (u32, u32, u32, u32) {
        let x = (self.x * frame_width as f64).round() as u32;
        let y = (self.y * frame_height as f64).round() as u32;
        let w = (self.width * frame_width as f64).round() as u32;
        let h = (self.height * frame_height as f64).round() as u32;
        (x, y, w, h)
    }
}

/// Types of context that may require disclaimers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextType {
    Educational,
    Medical,
    Financial,
    Legal,
    News,
    Entertainment,
    Sponsored,
    Political,
    General,
}

/// A detected object in a video frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObject {
    pub id: TrackId,
    pub class_name: String,
    pub confidence: f64,
    pub bbox: BoundingBox,
    pub frame_timestamp: f64,
    pub metadata: serde_json::Value,
}

/// Violation severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// A policy violation found during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub severity: Severity,
    pub timestamp: f64,
    pub duration: f64,
    pub description: String,
    pub bbox: Option<BoundingBox>,
    pub confidence: f64,
    pub metadata: serde_json::Value,
}

/// A segment with flashing content (photosensitive epilepsy risk)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashingSegment {
    pub start_time: f64,
    pub end_time: f64,
    pub frequency_hz: f64,
    pub intensity: f64,
    pub frames_affected: Vec<u32>,
}

/// Overall analysis result for a video
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub video_id: String,
    pub duration_seconds: f64,
    pub resolution: (u32, u32),
    pub fps: f64,
    pub violations: Vec<Violation>,
    pub flashing_segments: Vec<FlashingSegment>,
    pub detected_objects: Vec<DetectedObject>,
    pub audio_issues: Vec<AudioIssue>,
    pub metadata_issues: Vec<MetadataIssue>,
    pub overall_score: f64,
    pub confidence: f64,
}

/// Audio-specific issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioIssue {
    pub issue_type: AudioIssueType,
    pub start_time: f64,
    pub end_time: f64,
    pub severity: Severity,
    pub description: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioIssueType {
    ExplicitLanguage,
    CopyrightedMusic,
    LoudnessViolation,
    HateSpeech,
    Spam,
    SilenceTooLong,
}

/// Metadata-specific issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataIssue {
    pub issue_type: MetadataIssueType,
    pub severity: Severity,
    pub description: String,
    pub field: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetadataIssueType {
    ClickbaitTitle,
    ExcessiveCaps,
    ExcessivePunctuation,
    MisleadingTags,
    MissingDisclaimer,
    KeywordStuffing,
    ProfanityInMetadata,
}
