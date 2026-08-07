use crate::error::PixelFormat;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A single extracted video frame with its metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Raw pixel data in the specified format.
    pub data: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Timestamp in the video (in milliseconds).
    pub timestamp_ms: f64,
    /// Sequential frame number in the video stream.
    pub frame_number: u64,
    /// Pixel format of the frame data.
    pub format: PixelFormat,
    /// Source video path (for tracking).
    pub source_path: String,
}

impl Frame {
    /// Create a new frame with the given parameters.
    pub fn new(
        data: Vec<u8>,
        width: u32,
        height: u32,
        timestamp_ms: f64,
        frame_number: u64,
        format: PixelFormat,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            data,
            width,
            height,
            timestamp_ms,
            frame_number,
            format,
            source_path: source_path.into(),
        }
    }

    /// Compute the approximate memory size of this frame in bytes.
    pub fn memory_size(&self) -> usize {
        self.data.len()
            + std::mem::size_of::<u32>() * 2
            + std::mem::size_of::<f64>()
            + std::mem::size_of::<u64>()
            + std::mem::size_of::<PixelFormat>()
            + self.source_path.len()
    }

    /// Return the frame data as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Check if the frame is empty (no pixel data).
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Calculate the average brightness of the frame (0.0 - 255.0).
    /// Only works for RGB24 format.
    pub fn average_brightness(&self) -> f64 {
        if self.data.is_empty() || self.format != PixelFormat::RGB24 {
            return 0.0;
        }

        let mut total: u64 = 0;
        let pixel_count = self.data.len() / 3;
        if pixel_count == 0 {
            return 0.0;
        }

        for chunk in self.data.chunks_exact(3) {
            // Use perceived luminance: 0.299*R + 0.587*G + 0.114*B
            let luminance =
                0.299 * chunk[0] as f64 + 0.587 * chunk[1] as f64 + 0.114 * chunk[2] as f64;
            total += luminance as u64;
        }

        total as f64 / pixel_count as f64
    }

    /// Compute a simple RGB histogram for this frame.
    /// Returns a tuple of (red_histogram, green_histogram, blue_histogram).
    /// Only works for RGB24 format.
    pub fn rgb_histogram(&self) -> Option<([u32; 256], [u32; 256], [u32; 256])> {
        if self.format != PixelFormat::RGB24 {
            return None;
        }

        let mut r_hist = [0u32; 256];
        let mut g_hist = [0u32; 256];
        let mut b_hist = [0u32; 256];

        for chunk in self.data.chunks_exact(3) {
            r_hist[chunk[0] as usize] += 1;
            g_hist[chunk[1] as usize] += 1;
            b_hist[chunk[2] as usize] += 1;
        }

        Some((r_hist, g_hist, b_hist))
    }
}

/// A batch of frames delivered through the async channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameBatch {
    /// Unique identifier for this batch.
    pub batch_id: uuid::Uuid,
    /// Sequence number (0-indexed) within the extraction.
    pub sequence: u64,
    /// The frames in this batch.
    pub frames: Vec<Frame>,
    /// Total number of frames in the video (if known).
    pub total_frames: Option<u64>,
    /// Whether this is the final batch.
    pub is_last: bool,
    /// Timestamp when the batch was created.
    pub created_at: u64, // unix timestamp ms
}

impl FrameBatch {
    /// Create a new frame batch.
    pub fn new(sequence: u64, frames: Vec<Frame>, total_frames: Option<u64>, is_last: bool) -> Self {
        Self {
            batch_id: uuid::Uuid::new_v4(),
            sequence,
            frames,
            total_frames,
            is_last,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Returns the number of frames in this batch.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Returns true if this batch contains no frames.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Compute the total memory size of all frames in this batch.
    pub fn total_memory_size(&self) -> usize {
        self.frames.iter().map(|f| f.memory_size()).sum()
    }

    /// Get the time range covered by frames in this batch.
    /// Returns (min_timestamp_ms, max_timestamp_ms).
    pub fn time_range(&self) -> Option<(f64, f64)> {
        if self.frames.is_empty() {
            return None;
        }
        let min_ts = self.frames.iter().map(|f| f.timestamp_ms).fold(f64::INFINITY, f64::min);
        let max_ts = self.frames.iter().map(|f| f.timestamp_ms).fold(f64::NEG_INFINITY, f64::max);
        Some((min_ts, max_ts))
    }
}

/// Metadata extracted from a video file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMetadata {
    /// File path.
    pub path: String,
    /// Video duration.
    pub duration: Duration,
    /// Video width in pixels.
    pub width: u32,
    /// Video height in pixels.
    pub height: u32,
    /// Frames per second.
    pub fps: f64,
    /// Total frame count (estimated).
    pub total_frames: u64,
    /// Video codec name.
    pub codec: String,
    /// Video bitrate in bits per second.
    pub bitrate: u64,
    /// Pixel format used by the decoder.
    pub pixel_format: String,
    /// Whether the video has an audio stream.
    pub has_audio: bool,
    /// File size in bytes.
    pub file_size: u64,
}

impl VideoMetadata {
    /// Create a new VideoMetadata instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: impl Into<String>,
        duration: Duration,
        width: u32,
        height: u32,
        fps: f64,
        total_frames: u64,
        codec: impl Into<String>,
        bitrate: u64,
        pixel_format: impl Into<String>,
        has_audio: bool,
        file_size: u64,
    ) -> Self {
        Self {
            path: path.into(),
            duration,
            width,
            height,
            fps,
            total_frames,
            codec: codec.into(),
            bitrate,
            pixel_format: pixel_format.into(),
            has_audio,
            file_size,
        }
    }

    /// Calculate megapixels (width * height / 1,000,000).
    pub fn megapixels(&self) -> f64 {
        (self.width as f64 * self.height as f64) / 1_000_000.0
    }

    /// Return aspect ratio as a string (e.g., "16:9").
    pub fn aspect_ratio(&self) -> String {
        let gcd = Self::gcd(self.width, self.height);
        format!("{}:{}", self.width / gcd, self.height / gcd)
    }

    /// Format duration as HH:MM:SS.
    pub fn formatted_duration(&self) -> String {
        let total_secs = self.duration.as_secs();
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }

    fn gcd(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let tmp = a % b;
            a = b;
            b = tmp;
        }
        a
    }
}

/// Represents a detected scene transition in the video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTransition {
    /// Frame number where the transition occurs.
    pub frame_number: u64,
    /// Timestamp of the transition (in milliseconds).
    pub timestamp_ms: f64,
    /// Type of transition detected.
    pub transition_type: TransitionType,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
    /// Difference score that triggered the detection.
    pub difference_score: f64,
    /// Frame data at the transition point (optional, for keyframe extraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<Vec<u8>>,
}

impl SceneTransition {
    /// Create a new scene transition.
    pub fn new(
        frame_number: u64,
        timestamp_ms: f64,
        transition_type: TransitionType,
        confidence: f64,
        difference_score: f64,
    ) -> Self {
        Self {
            frame_number,
            timestamp_ms,
            transition_type,
            confidence: confidence.clamp(0.0, 1.0),
            difference_score,
            thumbnail: None,
        }
    }

    /// Attach a thumbnail image to this transition.
    pub fn with_thumbnail(mut self, data: Vec<u8>) -> Self {
        self.thumbnail = Some(data);
        self
    }
}

/// Types of scene transitions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TransitionType {
    /// Hard cut - abrupt frame change.
    HardCut,
    /// Fade in - gradual brightness increase.
    FadeIn,
    /// Fade out - gradual brightness decrease.
    FadeOut,
    /// Dissolve - gradual transition between scenes.
    Dissolve,
    /// Wipe - one scene replacing another with a boundary.
    Wipe,
    /// Unknown transition type.
    Unknown,
}

impl std::fmt::Display for TransitionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionType::HardCut => write!(f, "hard_cut"),
            TransitionType::FadeIn => write!(f, "fade_in"),
            TransitionType::FadeOut => write!(f, "fade_out"),
            TransitionType::Dissolve => write!(f, "dissolve"),
            TransitionType::Wipe => write!(f, "wipe"),
            TransitionType::Unknown => write!(f, "unknown"),
        }
    }
}

/// Represents a detected flashing segment in the video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashingSegment {
    /// Start timestamp in milliseconds.
    pub start_ms: f64,
    /// End timestamp in milliseconds.
    pub end_ms: f64,
    /// Start frame number.
    pub start_frame: u64,
    /// End frame number.
    pub end_frame: u64,
    /// Maximum brightness change rate detected (0.0 - 255.0).
    pub max_brightness_change: f64,
    /// Average flash frequency in Hz.
    pub avg_flash_frequency: f64,
    /// Number of distinct flash events.
    pub flash_count: u32,
    /// Severity classification.
    pub severity: FlashSeverity,
    /// Peak luminance values during the segment.
    pub peak_luminances: Vec<f64>,
}

impl FlashingSegment {
    /// Create a new flashing segment.
    pub fn new(
        start_ms: f64,
        end_ms: f64,
        start_frame: u64,
        end_frame: u64,
        max_brightness_change: f64,
        avg_flash_frequency: f64,
        flash_count: u32,
        severity: FlashSeverity,
        peak_luminances: Vec<f64>,
    ) -> Self {
        Self {
            start_ms,
            end_ms,
            start_frame,
            end_frame,
            max_brightness_change,
            avg_flash_frequency,
            flash_count,
            severity,
            peak_luminances,
        }
    }

    /// Duration of the flashing segment in milliseconds.
    pub fn duration_ms(&self) -> f64 {
        self.end_ms - self.start_ms
    }

    /// Whether this segment exceeds safety thresholds.
    pub fn is_concerning(&self) -> bool {
        matches!(self.severity, FlashSeverity::High | FlashSeverity::Critical)
    }
}

/// Severity classification for flashing content.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FlashSeverity {
    /// Low - minor brightness changes, generally safe.
    Low,
    /// Medium - noticeable flashing, potentially problematic for sensitive viewers.
    Medium,
    /// High - rapid flashing that may trigger photosensitive epilepsy.
    High,
    /// Critical - extreme flashing, immediate safety concern.
    Critical,
}

impl FlashSeverity {
    /// Classify severity based on flash frequency and brightness change.
    pub fn classify(flash_frequency_hz: f64, max_brightness_change: f64) -> Self {
        match (flash_frequency_hz, max_brightness_change) {
            (freq, change) if freq > 20.0 && change > 100.0 => FlashSeverity::Critical,
            (freq, change) if freq > 10.0 && change > 80.0 => FlashSeverity::High,
            (freq, change) if freq > 3.0 && change > 40.0 => FlashSeverity::Medium,
            _ => FlashSeverity::Low,
        }
    }

    /// Returns true if this severity level requires a warning.
    pub fn requires_warning(&self) -> bool {
        matches!(self, FlashSeverity::High | FlashSeverity::Critical)
    }
}

impl std::fmt::Display for FlashSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlashSeverity::Low => write!(f, "low"),
            FlashSeverity::Medium => write!(f, "medium"),
            FlashSeverity::High => write!(f, "high"),
            FlashSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Configuration for the frame extractor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractorConfig {
    /// Target frames per second for extraction (decimate if source fps is higher).
    /// None means extract all frames.
    pub target_fps: Option<f64>,
    /// Maximum resolution (width, height) to scale frames to.
    /// None means no scaling.
    pub max_resolution: Option<(u32, u32)>,
    /// Whether to use GPU acceleration for decoding.
    pub gpu_acceleration: bool,
    /// Scene detection threshold (0.0 - 1.0). Higher = less sensitive.
    pub scene_threshold: f64,
    /// Output pixel format for extracted frames.
    pub output_format: PixelFormat,
    /// Maximum number of frames per batch.
    pub batch_size: usize,
    /// Number of frames to skip between scene detection samples.
    pub scene_sample_interval: usize,
    /// Flash detection sensitivity (0.0 - 1.0).
    pub flash_sensitivity: f64,
    /// Minimum flash frequency to flag (in Hz).
    pub min_flash_frequency: f64,
    /// Maximum brightness change threshold for flash detection.
    pub flash_brightness_threshold: f64,
}

impl ExtractorConfig {
    /// Create a new ExtractorConfig with sensible defaults.
    pub fn new() -> Self {
        Self {
            target_fps: Some(5.0),
            max_resolution: Some((1280, 720)),
            gpu_acceleration: false,
            scene_threshold: 0.35,
            output_format: PixelFormat::RGB24,
            batch_size: 30,
            scene_sample_interval: 5,
            flash_sensitivity: 0.5,
            min_flash_frequency: 3.0,
            flash_brightness_threshold: 40.0,
        }
    }

    /// Create a high-quality extraction config (preserves more detail).
    pub fn high_quality() -> Self {
        Self {
            target_fps: Some(10.0),
            max_resolution: Some((1920, 1080)),
            gpu_acceleration: false,
            scene_threshold: 0.25,
            output_format: PixelFormat::RGB24,
            batch_size: 20,
            scene_sample_interval: 2,
            flash_sensitivity: 0.3,
            min_flash_frequency: 3.0,
            flash_brightness_threshold: 35.0,
        }
    }

    /// Create a fast extraction config (lower quality, higher speed).
    pub fn fast() -> Self {
        Self {
            target_fps: Some(2.0),
            max_resolution: Some((640, 480)),
            gpu_acceleration: false,
            scene_threshold: 0.5,
            output_format: PixelFormat::RGB24,
            batch_size: 50,
            scene_sample_interval: 10,
            flash_sensitivity: 0.7,
            min_flash_frequency: 5.0,
            flash_brightness_threshold: 50.0,
        }
    }

    /// Validate the configuration parameters.
    pub fn validate(&self) -> crate::error::Result<()> {
        if let Some(fps) = self.target_fps {
            if fps <= 0.0 || fps > 120.0 {
                return Err(crate::error::ExtractorError::InvalidConfig {
                    field: "target_fps".to_string(),
                    reason: format!("must be in range (0, 120], got {}", fps),
                });
            }
        }

        if let Some((w, h)) = self.max_resolution {
            if w == 0 || h == 0 || w > 7680 || h > 4320 {
                return Err(crate::error::ExtractorError::InvalidConfig {
                    field: "max_resolution".to_string(),
                    reason: format!(
                        "must be within [1x1, 7680x4320], got {}x{}",
                        w, h
                    ),
                });
            }
        }

        if !(0.0..=1.0).contains(&self.scene_threshold) {
            return Err(crate::error::ExtractorError::InvalidConfig {
                field: "scene_threshold".to_string(),
                reason: format!(
                    "must be in range [0.0, 1.0], got {}",
                    self.scene_threshold
                ),
            });
        }

        if self.batch_size == 0 || self.batch_size > 1000 {
            return Err(crate::error::ExtractorError::InvalidConfig {
                field: "batch_size".to_string(),
                reason: format!(
                    "must be in range [1, 1000], got {}",
                    self.batch_size
                ),
            });
        }

        Ok(())
    }

    /// Builder-style method to set target_fps.
    pub fn with_target_fps(mut self, fps: f64) -> Self {
        self.target_fps = Some(fps);
        self
    }

    /// Builder-style method to set max_resolution.
    pub fn with_max_resolution(mut self, width: u32, height: u32) -> Self {
        self.max_resolution = Some((width, height));
        self
    }

    /// Builder-style method to enable/disable GPU acceleration.
    pub fn with_gpu_acceleration(mut self, enabled: bool) -> Self {
        self.gpu_acceleration = enabled;
        self
    }

    /// Builder-style method to set scene threshold.
    pub fn with_scene_threshold(mut self, threshold: f64) -> Self {
        self.scene_threshold = threshold;
        self
    }

    /// Builder-style method to set batch size.
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_creation() {
        let frame = Frame::new(
            vec![255; 12], // 4 pixels of RGB24
            2,
            2,
            1000.0,
            1,
            PixelFormat::RGB24,
            "/tmp/test.mp4",
        );
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 2);
        assert_eq!(frame.timestamp_ms, 1000.0);
        assert_eq!(frame.frame_number, 1);
    }

    #[test]
    fn test_frame_memory_size() {
        let frame = Frame::new(
            vec![0u8; 1000],
            10,
            10,
            0.0,
            0,
            PixelFormat::RGB24,
            "/tmp/test.mp4",
        );
        assert!(frame.memory_size() >= 1000);
    }

    #[test]
    fn test_frame_average_brightness() {
        let data: Vec<u8> = vec![128u8; 300]; // 100 gray pixels
        let frame = Frame::new(data, 10, 10, 0.0, 0, PixelFormat::RGB24, "/tmp/test.mp4");
        let brightness = frame.average_brightness();
        assert!((brightness - 128.0).abs() < 1.0);
    }

    #[test]
    fn test_frame_histogram() {
        let mut data = Vec::new();
        for i in 0..256u8 {
            data.extend_from_slice(&[i, i, i]); // grayscale gradient
        }
        let frame = Frame::new(data, 16, 16, 0.0, 0, PixelFormat::RGB24, "/tmp/test.mp4");
        let (r_hist, g_hist, b_hist) = frame.rgb_histogram().unwrap();
        for i in 0..256 {
            assert_eq!(r_hist[i], 1);
            assert_eq!(g_hist[i], 1);
            assert_eq!(b_hist[i], 1);
        }
    }

    #[test]
    fn test_frame_batch() {
        let frame = Frame::new(vec![0u8; 100], 10, 10, 0.0, 0, PixelFormat::RGB24, "/tmp/test.mp4");
        let batch = FrameBatch::new(0, vec![frame.clone()], Some(100), false);
        assert_eq!(batch.len(), 1);
        assert!(!batch.is_empty());
        assert!(!batch.is_last);
    }

    #[test]
    fn test_video_metadata() {
        let meta = VideoMetadata::new(
            "/tmp/test.mp4",
            Duration::from_secs(3661),
            1920,
            1080,
            30.0,
            109830,
            "h264",
            5_000_000,
            "yuv420p",
            true,
            100_000_000,
        );
        assert_eq!(meta.megapixels(), 2.0736);
        assert_eq!(meta.aspect_ratio(), "16:9");
        assert_eq!(meta.formatted_duration(), "01:01:01");
    }

    #[test]
    fn test_scene_transition() {
        let transition = SceneTransition::new(
            30,
            1000.0,
            TransitionType::HardCut,
            0.85,
            0.42,
        );
        assert_eq!(transition.transition_type, TransitionType::HardCut);
        assert_eq!(transition.confidence, 0.85);
    }

    #[test]
    fn test_flash_severity_classify() {
        assert_eq!(
            FlashSeverity::classify(25.0, 120.0),
            FlashSeverity::Critical
        );
        assert_eq!(
            FlashSeverity::classify(15.0, 90.0),
            FlashSeverity::High
        );
        assert_eq!(
            FlashSeverity::classify(5.0, 50.0),
            FlashSeverity::Medium
        );
        assert_eq!(
            FlashSeverity::classify(1.0, 10.0),
            FlashSeverity::Low
        );
    }

    #[test]
    fn test_extractor_config_validation() {
        let config = ExtractorConfig::new();
        assert!(config.validate().is_ok());

        let bad_config = ExtractorConfig::new().with_target_fps(0.0);
        assert!(bad_config.validate().is_err());

        let bad_config = ExtractorConfig::new().with_scene_threshold(2.0);
        assert!(bad_config.validate().is_err());
    }

    #[test]
    fn test_extractor_config_builder() {
        let config = ExtractorConfig::new()
            .with_target_fps(15.0)
            .with_max_resolution(640, 480)
            .with_gpu_acceleration(true)
            .with_scene_threshold(0.5)
            .with_batch_size(10);

        assert_eq!(config.target_fps, Some(15.0));
        assert_eq!(config.max_resolution, Some((640, 480)));
        assert!(config.gpu_acceleration);
        assert_eq!(config.scene_threshold, 0.5);
        assert_eq!(config.batch_size, 10);
    }

    #[test]
    fn test_transition_type_display() {
        assert_eq!(TransitionType::HardCut.to_string(), "hard_cut");
        assert_eq!(TransitionType::FadeIn.to_string(), "fade_in");
        assert_eq!(TransitionType::FadeOut.to_string(), "fade_out");
    }

    #[test]
    fn test_flash_severity_display() {
        assert_eq!(FlashSeverity::Low.to_string(), "low");
        assert_eq!(FlashSeverity::Critical.to_string(), "critical");
    }

    #[test]
    fn test_flashing_segment_duration() {
        let segment = FlashingSegment::new(
            1000.0,
            3000.0,
            30,
            90,
            50.0,
            5.0,
            10,
            FlashSeverity::High,
            vec![100.0, 200.0, 50.0],
        );
        assert_eq!(segment.duration_ms(), 2000.0);
        assert!(segment.is_concerning());
    }
}
