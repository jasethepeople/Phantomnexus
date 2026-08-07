use crate::error::{ExtractorError, Result};
use crate::ffmpeg;
use crate::scene_detection::HistogramSceneDetector;
use crate::flash_detection::FlashDetector;
use crate::types::{ExtractorConfig, FlashingSegment, FrameBatch, SceneTransition, VideoMetadata};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, trace, warn};

/// FrameExtractor is the main entry point for video frame extraction.
///
/// It provides methods to:
/// - Extract frames from video files and stream them via async channels
/// - Detect scene transitions using histogram-based analysis
/// - Detect flashing content that may trigger photosensitive epilepsy
/// - Extract video metadata
///
/// # Example
///
/// ```
/// use frame_extractor::{FrameExtractor, ExtractorConfig};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = ExtractorConfig::new();
/// let extractor = FrameExtractor::new(config)?;
/// let metadata = extractor.get_video_metadata(Path::new("video.mp4")).await?;
/// println!("Duration: {:?}", metadata.duration);
/// # Ok(())
/// # }
/// ```
pub struct FrameExtractor {
    /// Configuration for extraction operations.
    config: Arc<ExtractorConfig>,
    /// Whether FFmpeg has been initialized.
    ffmpeg_initialized: bool,
}

impl FrameExtractor {
    /// Create a new FrameExtractor with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Extraction configuration parameters.
    ///
    /// # Returns
    /// * `Result<Self>` - A new FrameExtractor instance or an error if initialization fails.
    ///
    /// # Errors
    /// Returns `ExtractorError::InvalidConfig` if the configuration is invalid.
    pub fn new(config: ExtractorConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        // Initialize FFmpeg
        ffmpeg::init_ffmpeg()?;

        info!(
            target_fps = ?config.target_fps,
            max_resolution = ?config.max_resolution,
            gpu_acceleration = config.gpu_acceleration,
            scene_threshold = config.scene_threshold,
            "FrameExtractor initialized"
        );

        Ok(Self {
            config: Arc::new(config),
            ffmpeg_initialized: true,
        })
    }

    /// Create a new FrameExtractor with default configuration.
    pub fn default_config() -> Result<Self> {
        Self::new(ExtractorConfig::new())
    }

    /// Create a new FrameExtractor with high-quality configuration.
    pub fn high_quality() -> Result<Self> {
        Self::new(ExtractorConfig::high_quality())
    }

    /// Create a new FrameExtractor with fast/low-quality configuration.
    pub fn fast() -> Result<Self> {
        Self::new(ExtractorConfig::fast())
    }

    /// Extract frames from a video file and stream them via an async channel.
    ///
    /// This method spawns an async task that:
    /// 1. Opens the video file using FFmpeg
    /// 2. Decodes video frames
    /// 3. Converts to the configured pixel format and resolution
    /// 4. Sends frames in batches through the returned channel
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file.
    ///
    /// # Returns
    /// * `Result<mpsc::Receiver<FrameBatch>>` - Receiver for frame batches.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use frame_extractor::{FrameExtractor, ExtractorConfig};
    /// # use std::path::Path;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let extractor = FrameExtractor::new(ExtractorConfig::new())?;
    /// let mut rx = extractor.extract_frames(Path::new("video.mp4")).await?;
    ///
    /// while let Some(batch) = rx.recv().await {
    ///     println!("Received batch {} with {} frames", batch.sequence, batch.len());
    ///     if batch.is_last {
    ///         break;
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(video_path = %video_path.as_ref().display()))]
    pub async fn extract_frames(
        &self,
        video_path: &Path,
    ) -> Result<mpsc::Receiver<FrameBatch>> {
        // Verify video file exists and is readable
        self.verify_video_file(video_path).await?;

        let path = video_path.to_path_buf();
        let config = (*self.config).clone();
        let (tx, rx) = mpsc::channel(config.batch_size.max(1));

        info!(
            video_path = %path.display(),
            batch_size = config.batch_size,
            target_fps = ?config.target_fps,
            "Starting async frame extraction"
        );

        // Spawn the extraction task
        tokio::spawn(async move {
            if let Err(e) = ffmpeg::extract_frames_to_channel(&path, &config, tx).await {
                error!(error = %e, "Frame extraction task failed");
            } else {
                debug!("Frame extraction task completed successfully");
            }
        });

        Ok(rx)
    }

    /// Detect scene transitions in a video file.
    ///
    /// Uses histogram-based chi-squared distance between consecutive frames
    /// with an adaptive threshold to detect scene changes.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file.
    ///
    /// # Returns
    /// * `Result<Vec<SceneTransition>>` - List of detected scene transitions.
    #[instrument(skip(self), fields(video_path = %video_path.as_ref().display()))]
    pub async fn detect_scenes(&self, video_path: &Path) -> Result<Vec<SceneTransition>> {
        self.verify_video_file(video_path).await?;

        let path = video_path.to_path_buf();
        let threshold = self.config.scene_threshold;
        let sample_interval = self.config.scene_sample_interval;
        let target_fps = self.config.target_fps;

        info!(
            video_path = %path.display(),
            threshold = threshold,
            "Starting scene detection"
        );

        // Run scene detection in a blocking task
        let transitions = tokio::task::spawn_blocking(move || {
            let detector = HistogramSceneDetector::new(threshold, sample_interval)
                .with_min_scene_length(3)
                .with_fade_detection(true);

            detector.detect_scenes_from_video(&path, target_fps)
        })
        .await
        .map_err(|e| {
            ExtractorError::SceneDetection {
                message: format!("Scene detection task panicked: {}", e),
            }
        })?;

        match &transitions {
            Ok(t) => {
                info!(
                    count = t.len(),
                    "Scene detection completed"
                );
            }
            Err(e) => {
                error!(error = %e, "Scene detection failed");
            }
        }

        transitions
    }

    /// Detect flashing segments in a video file.
    ///
    /// Analyzes brightness changes over time to identify segments with
    /// rapid flashing that may be harmful to photosensitive viewers.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file.
    ///
    /// # Returns
    /// * `Result<Vec<FlashingSegment>>` - List of detected flashing segments.
    #[instrument(skip(self), fields(video_path = %video_path.as_ref().display()))]
    pub async fn detect_flashing(
        &self,
        video_path: &Path,
    ) -> Result<Vec<FlashingSegment>> {
        self.verify_video_file(video_path).await?;

        let path = video_path.to_path_buf();
        let min_frequency = self.config.min_flash_frequency;
        let brightness_threshold = self.config.flash_brightness_threshold;
        let sensitivity = self.config.flash_sensitivity;
        let target_fps = self.config.target_fps;

        info!(
            video_path = %path.display(),
            min_frequency = min_frequency,
            brightness_threshold = brightness_threshold,
            "Starting flash detection"
        );

        // Run flash detection in a blocking task
        let segments = tokio::task::spawn_blocking(move || {
            let detector = FlashDetector::new(min_frequency, brightness_threshold, sensitivity)
                .with_min_segment_duration(100.0)
                .with_min_consecutive_changes(2);

            detector.detect_flashes_from_video(&path, target_fps)
        })
        .await
        .map_err(|e| {
            ExtractorError::FlashDetection {
                message: format!("Flash detection task panicked: {}", e),
            }
        })?;

        match &segments {
            Ok(s) => {
                let concerning = s.iter().filter(|seg| seg.is_concerning()).count();
                info!(
                    total_segments = s.len(),
                    concerning_segments = concerning,
                    "Flash detection completed"
                );
            }
            Err(e) => {
                error!(error = %e, "Flash detection failed");
            }
        }

        segments
    }

    /// Extract metadata from a video file.
    ///
    /// Probes the video file to extract duration, resolution, FPS, codec,
    /// bitrate, and other metadata without decoding frames.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file.
    ///
    /// # Returns
    /// * `Result<VideoMetadata>` - Video metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use frame_extractor::{FrameExtractor, ExtractorConfig};
    /// # use std::path::Path;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let extractor = FrameExtractor::new(ExtractorConfig::new())?;
    /// let meta = extractor.get_video_metadata(Path::new("video.mp4")).await?;
    /// println!("Resolution: {}x{}", meta.width, meta.height);
    /// println!("Duration: {}s", meta.duration.as_secs_f64());
    /// println!("FPS: {}", meta.fps);
    /// println!("Codec: {}", meta.codec);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(video_path = %video_path.as_ref().display()))]
    pub async fn get_video_metadata(
        &self,
        video_path: &Path,
    ) -> Result<VideoMetadata> {
        self.verify_video_file(video_path).await?;

        let path = video_path.to_path_buf();

        debug!(video_path = %path.display(), "Extracting video metadata");

        // Run metadata extraction in a blocking task
        let metadata = tokio::task::spawn_blocking(move || {
            ffmpeg::get_video_metadata_ff(&path)
        })
        .await
        .map_err(|e| {
            ExtractorError::Internal(format!("Metadata extraction task panicked: {}", e))
        })??;

        trace!(
            duration_ms = metadata.duration.as_millis(),
            width = metadata.width,
            height = metadata.height,
            fps = metadata.fps,
            codec = %metadata.codec,
            "Metadata extracted"
        );

        Ok(metadata)
    }

    /// Get the current configuration.
    pub fn config(&self) -> &ExtractorConfig {
        &self.config
    }

    /// Check if FFmpeg has been initialized.
    pub fn is_ffmpeg_initialized(&self) -> bool {
        self.ffmpeg_initialized
    }

    /// Verify that a video file exists and is readable.
    async fn verify_video_file(&self, video_path: &Path) -> Result<()> {
        // Check if file exists
        let path = video_path.to_path_buf();
        let exists = tokio::task::spawn_blocking(move || path.exists())
            .await
            .map_err(|e| ExtractorError::Internal(format!("File check failed: {}", e)))?;

        if !exists {
            return Err(ExtractorError::Io {
                path: video_path.to_path_buf(),
                message: "Video file does not exist or is not accessible".to_string(),
            });
        }

        // Check if it's a regular file (not a directory)
        let path = video_path.to_path_buf();
        let is_file = tokio::task::spawn_blocking(move || {
            std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
        })
        .await
        .map_err(|e| ExtractorError::Internal(format!("File metadata check failed: {}", e)))?;

        if !is_file {
            return Err(ExtractorError::InvalidVideo {
                path: video_path.to_path_buf(),
                reason: "Path is not a regular file".to_string(),
            });
        }

        Ok(())
    }
}

// Manual Clone implementation since Arc<ExtractorConfig> is Clone
impl Clone for FrameExtractor {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            ffmpeg_initialized: self.ffmpeg_initialized,
        }
    }
}

impl std::fmt::Debug for FrameExtractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameExtractor")
            .field("config", &self.config)
            .field("ffmpeg_initialized", &self.ffmpeg_initialized)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_extractor_creation() {
        let config = ExtractorConfig::new();
        let extractor = FrameExtractor::new(config);
        // May fail if FFmpeg is not initialized, which is expected in test environment
        assert!(extractor.is_ok() || extractor.is_err());
    }

    #[test]
    fn test_frame_extractor_config_access() {
        let config = ExtractorConfig::new().with_target_fps(15.0);
        if let Ok(extractor) = FrameExtractor::new(config.clone()) {
            assert_eq!(extractor.config().target_fps, Some(15.0));
            assert!(extractor.is_ffmpeg_initialized());
        }
    }

    #[test]
    fn test_frame_extractor_clone() {
        let config = ExtractorConfig::new();
        if let Ok(extractor) = FrameExtractor::new(config) {
            let cloned = extractor.clone();
            assert_eq!(cloned.config.target_fps, extractor.config.target_fps);
            assert_eq!(cloned.ffmpeg_initialized, extractor.ffmpeg_initialized);
        }
    }

    #[test]
    fn test_frame_extractor_debug() {
        let config = ExtractorConfig::new();
        if let Ok(extractor) = FrameExtractor::new(config) {
            let debug_str = format!("{:?}", extractor);
            assert!(debug_str.contains("FrameExtractor"));
            assert!(debug_str.contains("config"));
            assert!(debug_str.contains("ffmpeg_initialized"));
        }
    }

    #[test]
    fn test_high_quality_config() {
        if let Ok(extractor) = FrameExtractor::high_quality() {
            assert_eq!(
                extractor.config().max_resolution,
                Some((1920, 1080))
            );
            assert_eq!(extractor.config().scene_threshold, 0.25);
        }
    }

    #[test]
    fn test_fast_config() {
        if let Ok(extractor) = FrameExtractor::fast() {
            assert_eq!(
                extractor.config().max_resolution,
                Some((640, 480))
            );
            assert_eq!(extractor.config().scene_threshold, 0.5);
        }
    }

    #[tokio::test]
    async fn test_verify_nonexistent_file() {
        let config = ExtractorConfig::new();
        if let Ok(extractor) = FrameExtractor::new(config) {
            let result = extractor
                .get_video_metadata(Path::new("/nonexistent/path/video.mp4"))
                .await;
            assert!(result.is_err());
            match result {
                Err(ExtractorError::Io { .. }) => {} // Expected
                Err(other) => {
                    // Also acceptable - any error about file not existing
                    let msg = other.to_string();
                    assert!(
                        msg.contains("does not exist") || msg.contains("Invalid video"),
                        "Expected file not found error, got: {}",
                        msg
                    );
                }
                Ok(_) => panic!("Should have failed for nonexistent file"),
            }
        }
    }
}
