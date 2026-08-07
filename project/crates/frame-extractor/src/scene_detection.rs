use crate::error::{ExtractorError, PixelFormat, Result};
use crate::types::{Frame, SceneTransition, TransitionType};
use std::path::Path;
use tracing::{debug, info, trace, warn};

/// Scene detector that uses histogram-based frame difference analysis.
///
/// Detects scene transitions by computing per-channel RGB histograms
/// and measuring the chi-squared distance between consecutive frames.
/// Uses an adaptive threshold based on the mean and standard deviation
/// of differences across the video.
pub struct HistogramSceneDetector {
    /// Base threshold for scene detection (0.0 - 1.0).
    base_threshold: f64,
    /// Number of frames to skip between samples.
    sample_interval: usize,
    /// Minimum scene length in frames (to avoid false positives on rapid motion).
    min_scene_length: usize,
    /// Whether to compute fade detection.
    detect_fades: bool,
    /// Histogram bin count (256 for 8-bit per channel).
    bin_count: usize,
}

impl HistogramSceneDetector {
    /// Create a new scene detector with the given threshold.
    ///
    /// # Arguments
    /// * `threshold` - Base detection threshold (0.0 - 1.0). Higher values make detection less sensitive.
    /// * `sample_interval` - Number of frames to skip between histogram samples.
    pub fn new(threshold: f64, sample_interval: usize) -> Self {
        Self {
            base_threshold: threshold.clamp(0.05, 0.95),
            sample_interval: sample_interval.max(1),
            min_scene_length: 5,
            detect_fades: true,
            bin_count: 256,
        }
    }

    /// Set the minimum scene length (in frames) to avoid false positives.
    pub fn with_min_scene_length(mut self, frames: usize) -> Self {
        self.min_scene_length = frames;
        self
    }

    /// Enable or disable fade detection.
    pub fn with_fade_detection(mut self, enabled: bool) -> Self {
        self.detect_fades = enabled;
        self
    }

    /// Detect scene transitions in a sequence of frames.
    ///
    /// This method takes frames (which should be sampled at the configured interval)
    /// and returns a list of detected scene transitions.
    ///
    /// # Arguments
    /// * `frames` - Slice of frames to analyze (should be RGB24 format).
    ///
    /// # Returns
    /// * `Result<Vec<SceneTransition>>` - Detected scene transitions sorted by frame number.
    pub fn detect_scenes(&self, frames: &[Frame]) -> Result<Vec<SceneTransition>> {
        if frames.len() < 2 {
            return Ok(vec![]);
        }

        trace!(
            frame_count = frames.len(),
            threshold = self.base_threshold,
            "Starting scene detection"
        );

        // Compute histograms and differences
        let mut differences: Vec<FrameDifference> = Vec::new();

        // First, compute all histograms
        let histograms: Vec<([u32; 256], [u32; 256], [u32; 256])> = frames
            .iter()
            .map(|f| f.rgb_histogram())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ExtractorError::SceneDetection {
                message: "Failed to compute histograms - frames may not be RGB24 format".to_string(),
            })?;

        // Compute frame-to-frame differences
        for i in 1..histograms.len() {
            let prev_hist = &histograms[i - 1];
            let curr_hist = &histograms[i];

            let chi2_r = chi_squared_distance(&prev_hist.0, &curr_hist.0);
            let chi2_g = chi_squared_distance(&prev_hist.1, &curr_hist.1);
            let chi2_b = chi_squared_distance(&prev_hist.2, &curr_hist.2);

            // Combine channel differences (weighted average)
            let combined_diff = (chi2_r * 0.299 + chi2_g * 0.587 + chi2_b * 0.114) / self.bin_count as f64;

            // Normalize to 0.0 - 1.0 range
            let normalized_diff = (combined_diff / (1.0 + combined_diff)).min(1.0);

            differences.push(FrameDifference {
                frame_index: i,
                difference: normalized_diff,
                timestamp_ms: frames[i].timestamp_ms,
                frame_number: frames[i].frame_number,
            });
        }

        if differences.is_empty() {
            return Ok(vec![]);
        }

        // Compute adaptive threshold using mean + k * std_dev
        let adaptive_threshold = self.compute_adaptive_threshold(&differences);

        trace!(
            adaptive_threshold = adaptive_threshold,
            base_threshold = self.base_threshold,
            "Computed adaptive threshold"
        );

        // Use the higher of base threshold and adaptive threshold
        let effective_threshold = self.base_threshold.max(adaptive_threshold);

        // Detect transitions
        let mut transitions: Vec<SceneTransition> = Vec::new();
        let mut last_transition_idx: i64 = -1;

        for diff in &differences {
            let idx = diff.frame_index as i64;
            let frame_gap = idx - last_transition_idx;

            if frame_gap < self.min_scene_length as i64 {
                continue;
            }

            if diff.difference >= effective_threshold {
                let transition_type = if self.detect_fades {
                    self.classify_transition_type(&differences, diff.frame_index)
                } else {
                    TransitionType::HardCut
                };

                let confidence = diff.difference.min(1.0);

                transitions.push(SceneTransition::new(
                    diff.frame_number,
                    diff.timestamp_ms,
                    transition_type,
                    confidence,
                    diff.difference,
                ));

                last_transition_idx = idx;

                trace!(
                    frame_number = diff.frame_number,
                    difference = diff.difference,
                    transition_type = ?transition_type,
                    "Scene transition detected"
                );
            }
        }

        info!(
            transition_count = transitions.len(),
            effective_threshold = effective_threshold,
            "Scene detection completed"
        );

        Ok(transitions)
    }

    /// Detect scene transitions by extracting frames directly from a video file.
    ///
    /// This is a convenience method that extracts frames at the configured sample interval
    /// and then runs scene detection on them.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file.
    /// * `target_fps` - Optional target FPS for frame extraction (None = use native FPS).
    ///
    /// # Returns
    /// * `Result<Vec<SceneTransition>>` - Detected scene transitions.
    pub fn detect_scenes_from_video(
        &self,
        video_path: &Path,
        target_fps: Option<f64>,
    ) -> Result<Vec<SceneTransition>> {
        debug!(
            path = ?video_path,
            "Detecting scenes from video file"
        );

        // Open video with FFmpeg and extract frames
        let mut input = ffmpeg_next::format::input(&video_path).map_err(|e| {
            ExtractorError::InvalidVideo {
                path: video_path.to_path_buf(),
                reason: format!("Failed to open video: {}", e),
            }
        })?;

        let stream = input
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| ExtractorError::NoVideoStream {
                path: video_path.to_path_buf(),
            })?;

        let stream_index = stream.index();
        let time_base = stream.time_base();
        let avg_frame_rate = stream.avg_frame_rate();

        let source_fps = if avg_frame_rate.denominator() > 0 {
            avg_frame_rate.numerator() as f64 / avg_frame_rate.denominator() as f64
        } else {
            30.0
        };

        // Create decoder
        let context = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| ExtractorError::FFmpeg {
                message: format!("Failed to create codec context: {}", e),
                code: None,
            })?;

        let mut decoder = context.decoder().video().map_err(|e| ExtractorError::FFmpeg {
            message: format!("Failed to create video decoder: {}", e),
            code: None,
        })?;

        let source_width = decoder.width();
        let source_height = decoder.height();
        let source_format = decoder.format();

        // Create scaler to RGB24
        let mut scaler = ffmpeg_next::software::scaling::Context::get(
            source_format,
            source_width,
            source_height,
            ffmpeg_next::format::Pixel::RGB24,
            source_width,
            source_height,
            ffmpeg_next::software::scaling::Flags::FAST_BILINEAR,
        )
        .map_err(|e| ExtractorError::Scaling {
            reason: format!("Failed to create scaler: {}", e),
        })?;

        // Calculate frame skip for target FPS
        let frame_skip = if let Some(target) = target_fps {
            if target < source_fps {
                (source_fps / target).round() as usize
            } else {
                1
            }
        } else {
            1
        };

        let mut frames: Vec<Frame> = Vec::new();
        let mut decoded_frame_count: usize = 0;
        let mut ffmpeg_frame = ffmpeg_next::frame::Video::empty();
        let mut scaled_frame = ffmpeg_next::frame::Video::empty();

        for (stream, packet) in input.packets() {
            if stream.index() != stream_index {
                continue;
            }

            if decoder.send_packet(&packet).is_err() {
                continue;
            }

            while decoder.receive_frame(&mut ffmpeg_frame).is_ok() {
                decoded_frame_count += 1;

                // Skip frames based on target FPS
                if frame_skip > 1 && decoded_frame_count % frame_skip != 0 {
                    continue;
                }

                // Skip frames based on sample interval
                let should_sample =
                    (decoded_frame_count / frame_skip.max(1)) % self.sample_interval == 0;
                if !should_sample {
                    continue;
                }

                // Scale to RGB24
                if scaler.run(&ffmpeg_frame, &mut scaled_frame).is_ok() {
                    let timestamp_ms = ffmpeg_frame.timestamp().map_or_else(
                        || frames.len() as f64 * (1000.0 / source_fps),
                        |ts| {
                            ts as f64
                                * f64::from(time_base.numerator())
                                / f64::from(time_base.denominator())
                                * 1000.0
                        },
                    );

                    let frame_data = scaled_frame_to_vec(&scaled_frame);

                    frames.push(Frame::new(
                        frame_data,
                        source_width as u32,
                        source_height as u32,
                        timestamp_ms,
                        decoded_frame_count as u64,
                        PixelFormat::RGB24,
                        video_path.to_string_lossy().as_ref(),
                    ));
                }
            }
        }

        info!(
            sampled_frames = frames.len(),
            total_decoded = decoded_frame_count,
            "Extracted frames for scene detection"
        );

        self.detect_scenes(&frames)
    }

    /// Compute an adaptive threshold based on mean and standard deviation
    /// of frame differences.
    ///
    /// Uses the formula: threshold = mean + k * std_dev
    /// where k is tuned to be sensitive to outliers (scene changes).
    fn compute_adaptive_threshold(&self, differences: &[FrameDifference]) -> f64 {
        if differences.is_empty() {
            return self.base_threshold;
        }

        let n = differences.len() as f64;

        // Compute mean
        let mean: f64 = differences.iter().map(|d| d.difference).sum::<f64>() / n;

        // Compute standard deviation
        let variance: f64 = differences
            .iter()
            .map(|d| {
                let diff = d.difference - mean;
                diff * diff
            })
            .sum::<f64>()
            / n;

        let std_dev = variance.sqrt();

        // Use mean + 2.5 * std_dev as adaptive threshold
        // This captures significant outliers while filtering noise
        let k = 2.5;
        let adaptive_threshold = mean + k * std_dev;

        // Clamp to reasonable range
        adaptive_threshold.clamp(0.05, 0.95)
    }

    /// Classify the type of transition based on the difference pattern.
    ///
    /// Analyzes the surrounding frames to determine if the transition
    /// is a hard cut, fade, dissolve, etc.
    fn classify_transition_type(
        &self,
        differences: &[FrameDifference],
        frame_index: usize,
    ) -> TransitionType {
        // Look at surrounding differences
        let window_size = 3usize;
        let start = frame_index.saturating_sub(window_size);
        let end = (frame_index + window_size + 1).min(differences.len());

        if end - start < 3 {
            return TransitionType::HardCut;
        }

        let window = &differences[start..end];

        // Check for hard cut: single spike with low surrounding differences
        let local_max = window.iter().map(|d| d.difference).fold(0.0, f64::max);
        let local_avg = window.iter().map(|d| d.difference).sum::<f64>() / window.len() as f64;

        // If the local max is much higher than the average, it's a hard cut
        if local_max > local_avg * 3.0 {
            return TransitionType::HardCut;
        }

        // Check for fade: gradual increase or decrease in differences
        // Look at the pattern around the transition
        if frame_index >= 2 && frame_index < differences.len() - 2 {
            let pre_diffs: Vec<f64> =
                differences[(frame_index - 2)..frame_index]
                    .iter()
                    .map(|d| d.difference)
                    .collect();
            let post_diffs: Vec<f64> = differences[frame_index..(frame_index + 2)]
                .iter()
                .map(|d| d.difference)
                .collect();

            let pre_avg = pre_diffs.iter().sum::<f64>() / pre_diffs.len() as f64;
            let post_avg = post_diffs.iter().sum::<f64>() / post_diffs.len() as f64;

            if pre_avg < 0.1 && post_avg > 0.3 {
                return TransitionType::FadeIn;
            }

            if pre_avg > 0.3 && post_avg < 0.1 {
                return TransitionType::FadeOut;
            }
        }

        // Check for dissolve: gradual change over several frames
        let variance = {
            let avg = local_avg;
            let var = window
                .iter()
                .map(|d| (d.difference - avg) * (d.difference - avg))
                .sum::<f64>()
                / window.len() as f64;
            var.sqrt()
        };

        if variance < 0.05 && local_avg > 0.15 {
            return TransitionType::Dissolve;
        }

        // Default to hard cut
        TransitionType::HardCut
    }

    /// Compute chi-squared distance between two histograms.
    ///
    /// The chi-squared distance measures the difference between two
    /// probability distributions and is effective for detecting
    /// significant changes in frame content.
    fn chi_squared_distance(hist1: &[u32; 256], hist2: &[u32; 256]) -> f64 {
        let total1: u64 = hist1.iter().map(|&v| v as u64).sum();
        let total2: u64 = hist2.iter().map(|&v| v as u64).sum();

        if total1 == 0 || total2 == 0 {
            return 0.0;
        }

        let mut distance: f64 = 0.0;
        for i in 0..256 {
            let p1 = hist1[i] as f64 / total1 as f64;
            let p2 = hist2[i] as f64 / total2 as f64;

            let diff = p1 - p2;
            let sum = p1 + p2;

            if sum > 1e-10 {
                distance += (diff * diff) / sum;
            }
        }

        distance
    }
}

/// Internal struct to track frame differences.
#[derive(Debug)]
struct FrameDifference {
    frame_index: usize,
    difference: f64,
    timestamp_ms: f64,
    frame_number: u64,
}

/// Convert an FFmpeg scaled frame to Vec<u8> (RGB24 only).
fn scaled_frame_to_vec(frame: &ffmpeg_next::frame::Video) -> Vec<u8> {
    let width = frame.width() as usize;
    let height = frame.height() as usize;
    let stride = frame.stride(0);
    let data = frame.data(0);

    let mut result = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        let row_start = row * stride;
        result.extend_from_slice(&data[row_start..row_start + width * 3]);
    }
    result
}

/// Convenience function for chi-squared distance (used by tests and public API).
fn chi_squared_distance(hist1: &[u32; 256], hist2: &[u32; 256]) -> f64 {
    let total1: u64 = hist1.iter().map(|&v| v as u64).sum();
    let total2: u64 = hist2.iter().map(|&v| v as u64).sum();

    if total1 == 0 || total2 == 0 {
        return 0.0;
    }

    let mut distance: f64 = 0.0;
    for i in 0..256 {
        let p1 = hist1[i] as f64 / total1 as f64;
        let p2 = hist2[i] as f64 / total2 as f64;

        let diff = p1 - p2;
        let sum = p1 + p2;

        if sum > 1e-10 {
            distance += (diff * diff) / sum;
        }
    }

    distance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(
        data: Vec<u8>,
        width: u32,
        height: u32,
        timestamp_ms: f64,
        frame_number: u64,
    ) -> Frame {
        Frame::new(
            data,
            width,
            height,
            timestamp_ms,
            frame_number,
            PixelFormat::RGB24,
            "/tmp/test.mp4",
        )
    }

    fn create_solid_frame(r: u8, g: u8, b: u8, width: u32, height: u32, ts: f64, num: u64) -> Frame {
        let pixel_count = (width * height) as usize;
        let mut data = Vec::with_capacity(pixel_count * 3);
        for _ in 0..pixel_count {
            data.extend_from_slice(&[r, g, b]);
        }
        create_test_frame(data, width, height, ts, num)
    }

    #[test]
    fn test_chi_squared_distance_identical() {
        let hist1 = [100u32; 256];
        let hist2 = [100u32; 256];
        let dist = chi_squared_distance(&hist1, &hist2);
        assert!((dist - 0.0).abs() < 1e-10, "Distance between identical histograms should be 0, got {}", dist);
    }

    #[test]
    fn test_chi_squared_distance_different() {
        let mut hist1 = [0u32; 256];
        let mut hist2 = [0u32; 256];
        hist1[0] = 256;
        hist2[255] = 256;

        let dist = chi_squared_distance(&hist1, &hist2);
        assert!(dist > 0.0, "Distance should be positive");
        assert!(dist <= 2.0, "Chi-squared distance should be <= 2.0");
    }

    #[test]
    fn test_detect_scenes_no_transitions() {
        // Create 10 identical frames - no transitions
        let mut frames = Vec::new();
        for i in 0..10 {
            frames.push(create_solid_frame(128, 128, 128, 10, 10, i as f64 * 1000.0, i));
        }

        let detector = HistogramSceneDetector::new(0.3, 1);
        let transitions = detector.detect_scenes(&frames).unwrap();
        assert_eq!(transitions.len(), 0, "No transitions expected in identical frames");
    }

    #[test]
    fn test_detect_scenes_with_transition() {
        // Create frames with a clear scene change at frame 5
        let mut frames = Vec::new();
        for i in 0..5 {
            frames.push(create_solid_frame(0, 0, 0, 10, 10, i as f64 * 1000.0, i)); // black
        }
        for i in 5..10 {
            frames.push(create_solid_frame(255, 255, 255, 10, 10, i as f64 * 1000.0, i)); // white
        }

        let detector = HistogramSceneDetector::new(0.1, 1);
        let transitions = detector.detect_scenes(&frames).unwrap();
        assert!(
            transitions.len() >= 1,
            "Should detect at least one transition, got {}",
            transitions.len()
        );

        // The transition should be near frame 5
        let first_transition = &transitions[0];
        assert!(
            (first_transition.frame_number as i64 - 5i64).abs() <= 1,
            "Transition should be near frame 5, got frame {}",
            first_transition.frame_number
        );
    }

    #[test]
    fn test_detect_scenes_insufficient_frames() {
        let detector = HistogramSceneDetector::new(0.3, 1);
        let transitions = detector.detect_scenes(&[]).unwrap();
        assert_eq!(transitions.len(), 0);

        let frames = vec![create_solid_frame(128, 128, 128, 10, 10, 0.0, 0)];
        let transitions = detector.detect_scenes(&frames).unwrap();
        assert_eq!(transitions.len(), 0);
    }

    #[test]
    fn test_adaptive_threshold_computation() {
        let detector = HistogramSceneDetector::new(0.5, 1);

        let differences = vec![
            FrameDifference {
                frame_index: 1,
                difference: 0.05,
                timestamp_ms: 1000.0,
                frame_number: 1,
            },
            FrameDifference {
                frame_index: 2,
                difference: 0.06,
                timestamp_ms: 2000.0,
                frame_number: 2,
            },
            FrameDifference {
                frame_index: 3,
                difference: 0.05,
                timestamp_ms: 3000.0,
                frame_number: 3,
            },
            FrameDifference {
                frame_index: 4,
                difference: 0.07,
                timestamp_ms: 4000.0,
                frame_number: 4,
            },
            FrameDifference {
                frame_index: 5,
                difference: 0.90, // outlier - scene change
                timestamp_ms: 5000.0,
                frame_number: 5,
            },
            FrameDifference {
                frame_index: 6,
                difference: 0.06,
                timestamp_ms: 6000.0,
                frame_number: 6,
            },
        ];

        let threshold = detector.compute_adaptive_threshold(&differences);
        // The threshold should be reasonable: above the noise floor but below the outlier
        assert!(threshold > 0.05, "Threshold should be above noise floor");
        assert!(threshold < 0.90, "Threshold should be below the clear outlier");
    }

    #[test]
    fn test_transition_type_classification() {
        let detector = HistogramSceneDetector::new(0.3, 1);

        // Simulate a hard cut pattern
        let differences = vec![
            FrameDifference { frame_index: 1, difference: 0.01, timestamp_ms: 1000.0, frame_number: 1 },
            FrameDifference { frame_index: 2, difference: 0.02, timestamp_ms: 2000.0, frame_number: 2 },
            FrameDifference { frame_index: 3, difference: 0.85, timestamp_ms: 3000.0, frame_number: 3 }, // spike
            FrameDifference { frame_index: 4, difference: 0.01, timestamp_ms: 4000.0, frame_number: 4 },
            FrameDifference { frame_index: 5, difference: 0.02, timestamp_ms: 5000.0, frame_number: 5 },
        ];

        let transition_type = detector.classify_transition_type(&differences, 2);
        assert_eq!(transition_type, TransitionType::HardCut, "Should classify as hard cut");
    }

    #[test]
    fn test_min_scene_length_respected() {
        // Create frames with transitions very close together
        let mut frames = Vec::new();

        // Scene 1: black
        for i in 0..3 {
            frames.push(create_solid_frame(0, 0, 0, 10, 10, i as f64 * 1000.0, i));
        }
        // Scene 2: white
        for i in 3..4 {
            frames.push(create_solid_frame(255, 255, 255, 10, 10, i as f64 * 1000.0, i));
        }
        // Scene 3: red (very close to scene 2)
        for i in 4..10 {
            frames.push(create_solid_frame(255, 0, 0, 10, 10, i as f64 * 1000.0, i));
        }

        let detector = HistogramSceneDetector::new(0.1, 1).with_min_scene_length(5);
        let transitions = detector.detect_scenes(&frames).unwrap();

        // With min_scene_length=5, the close transitions should be filtered out
        assert!(
            transitions.len() <= 2,
            "Should have at most 2 transitions with min_scene_length=5, got {}",
            transitions.len()
        );
    }

    #[test]
    fn test_builder_methods() {
        let detector = HistogramSceneDetector::new(0.5, 5)
            .with_min_scene_length(10)
            .with_fade_detection(false);

        assert_eq!(detector.base_threshold, 0.5);
        assert_eq!(detector.sample_interval, 5);
        assert_eq!(detector.min_scene_length, 10);
        assert!(!detector.detect_fades);
    }

    #[test]
    fn test_detector_with_sample_interval() {
        let mut frames = Vec::new();
        for i in 0..20 {
            frames.push(create_solid_frame(128, 128, 128, 10, 10, i as f64 * 100.0, i));
        }

        // detector with interval 5 should still work, just sample fewer frames internally
        let detector = HistogramSceneDetector::new(0.3, 5);
        let transitions = detector.detect_scenes(&frames).unwrap();
        assert_eq!(transitions.len(), 0);
    }
}
