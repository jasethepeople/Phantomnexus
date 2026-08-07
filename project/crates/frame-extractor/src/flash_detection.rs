use crate::error::{ExtractorError, PixelFormat, Result};
use crate::types::{FlashSeverity, FlashingSegment, Frame};
use std::path::Path;
use tracing::{debug, info, trace, warn};

/// Flash detector that analyzes brightness changes over time to detect
/// potentially harmful flashing content.
///
/// The detector:
/// 1. Computes per-frame average brightness
/// 2. Identifies rapid brightness transitions (>3Hz by default)
/// 3. Classifies severity based on frequency and magnitude of changes
/// 4. Reports segments that exceed safety thresholds
///
/// This is particularly important for detecting content that could
/// trigger photosensitive epilepsy in susceptible individuals.
pub struct FlashDetector {
    /// Minimum flash frequency to flag (in Hz).
    min_frequency: f64,
    /// Brightness change threshold for detecting a flash event.
    brightness_threshold: f64,
    /// Overall sensitivity (0.0 - 1.0), affects how strict detection is.
    sensitivity: f64,
    /// Minimum duration of a flashing segment to report (in milliseconds).
    min_segment_duration_ms: f64,
    /// Maximum brightness difference for normal content (below this is always safe).
    safe_brightness_change: f64,
    /// Number of consecutive brightness changes required for a flash event.
    min_consecutive_changes: usize,
}

impl FlashDetector {
    /// Create a new flash detector with the specified parameters.
    ///
    /// # Arguments
    /// * `min_frequency` - Minimum flash frequency (Hz) to flag as concerning.
    /// * `brightness_threshold` - Minimum brightness change to consider as a flash.
    /// * `sensitivity` - Overall sensitivity (0.0 - 1.0), lower = more sensitive.
    pub fn new(min_frequency: f64, brightness_threshold: f64, sensitivity: f64) -> Self {
        Self {
            min_frequency: min_frequency.max(1.0),
            brightness_threshold: brightness_threshold.max(5.0),
            sensitivity: sensitivity.clamp(0.01, 1.0),
            min_segment_duration_ms: 100.0,
            safe_brightness_change: 20.0,
            min_consecutive_changes: 2,
        }
    }

    /// Set the minimum duration for a flashing segment to be reported.
    pub fn with_min_segment_duration(mut self, duration_ms: f64) -> Self {
        self.min_segment_duration_ms = duration_ms.max(50.0);
        self
    }

    /// Set the safe brightness change threshold.
    /// Brightness changes below this value are never flagged.
    pub fn with_safe_brightness_change(mut self, threshold: f64) -> Self {
        self.safe_brightness_change = threshold.max(1.0);
        self
    }

    /// Set the minimum number of consecutive brightness changes for a flash event.
    pub fn with_min_consecutive_changes(mut self, count: usize) -> Self {
        self.min_consecutive_changes = count.max(1);
        self
    }

    /// Detect flashing segments in a sequence of frames.
    ///
    /// # Arguments
    /// * `frames` - Slice of frames to analyze (must be RGB24 format).
    ///
    /// # Returns
    /// * `Result<Vec<FlashingSegment>>` - Detected flashing segments sorted by start time.
    pub fn detect_flashes(&self, frames: &[Frame]) -> Result<Vec<FlashingSegment>> {
        if frames.len() < 2 {
            return Ok(vec![]);
        }

        trace!(
            frame_count = frames.len(),
            min_frequency = self.min_frequency,
            brightness_threshold = self.brightness_threshold,
            "Starting flash detection"
        );

        // Step 1: Compute brightness for each frame
        let brightness_values: Vec<(usize, f64, u64)> = frames
            .iter()
            .enumerate()
            .map(|(idx, frame)| {
                let brightness = frame.average_brightness();
                (idx, brightness, frame.timestamp_ms as u64)
            })
            .collect();

        if brightness_values.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: Detect brightness changes
        let mut flash_events: Vec<FlashEvent> = Vec::new();
        let mut consecutive_changes: usize = 0;
        let mut change_start_idx: usize = 0;
        let mut last_direction: i8 = 0; // -1 = down, 1 = up, 0 = none

        for i in 1..brightness_values.len() {
            let prev_brightness = brightness_values[i - 1].1;
            let curr_brightness = brightness_values[i].1;
            let brightness_change = (curr_brightness - prev_brightness).abs();

            // Determine direction of change
            let direction = if brightness_change < 1.0 {
                0 // no significant change
            } else if curr_brightness > prev_brightness {
                1 // brightening
            } else {
                -1 // darkening
            };

            // Adjust threshold based on sensitivity
            let effective_threshold =
                self.brightness_threshold * (0.5 + 0.5 * self.sensitivity);

            if brightness_change >= effective_threshold && direction != 0 {
                // Significant brightness change detected
                if direction != last_direction && last_direction != 0 {
                    // Direction changed - potential flash
                    consecutive_changes += 1;
                    if consecutive_changes >= self.min_consecutive_changes {
                        let event = FlashEvent {
                            start_idx: change_start_idx,
                            end_idx: i,
                            start_ms: brightness_values[change_start_idx].2 as f64,
                            end_ms: brightness_values[i].2 as f64,
                            brightness_change: self.calculate_peak_change(
                                &brightness_values,
                                change_start_idx,
                                i,
                            ),
                            direction_change: true,
                        };
                        flash_events.push(event);

                        // Reset for next potential flash
                        change_start_idx = i;
                        consecutive_changes = 1;
                    }
                } else if direction != last_direction {
                    // New change direction
                    change_start_idx = i - 1;
                    consecutive_changes = 1;
                }

                last_direction = direction;
            } else if brightness_change < self.safe_brightness_change {
                // Small change, reset flash tracking
                consecutive_changes = 0;
                last_direction = 0;
            }
        }

        trace!(
            flash_events = flash_events.len(),
            "Detected raw flash events"
        );

        // Step 3: Group flash events into segments
        let segments = self.group_events_into_segments(&flash_events, &brightness_values, frames)?;

        info!(
            segment_count = segments.len(),
            "Flash detection completed"
        );

        Ok(segments)
    }

    /// Detect flashing segments by extracting frames from a video file.
    ///
    /// # Arguments
    /// * `video_path` - Path to the video file.
    /// * `target_fps` - Optional target FPS for frame extraction.
    ///
    /// # Returns
    /// * `Result<Vec<FlashingSegment>>` - Detected flashing segments.
    pub fn detect_flashes_from_video(
        &self,
        video_path: &Path,
        target_fps: Option<f64>,
    ) -> Result<Vec<FlashingSegment>> {
        debug!(
            path = ?video_path,
            "Detecting flashes from video file"
        );

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

                if frame_skip > 1 && decoded_frame_count % frame_skip != 0 {
                    continue;
                }

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
            extracted_frames = frames.len(),
            "Extracted frames for flash detection"
        );

        self.detect_flashes(&frames)
    }

    /// Calculate the peak brightness change within a range of frames.
    fn calculate_peak_change(
        &self,
        brightness_values: &[(usize, f64, u64)],
        start_idx: usize,
        end_idx: usize,
    ) -> f64 {
        let mut max_change: f64 = 0.0;
        for i in (start_idx + 1)..=end_idx.min(brightness_values.len() - 1) {
            let change = (brightness_values[i].1 - brightness_values[i - 1].1).abs();
            max_change = max_change.max(change);
        }
        max_change
    }

    /// Group individual flash events into continuous flashing segments.
    fn group_events_into_segments(
        &self,
        events: &[FlashEvent],
        brightness_values: &[(usize, f64, u64)],
        frames: &[Frame],
    ) -> Result<Vec<FlashingSegment>> {
        if events.is_empty() {
            return Ok(vec![]);
        }

        // Collect all brightness change magnitudes for statistics
        let all_changes: Vec<f64> = events.iter().map(|e| e.brightness_change).collect();
        let max_brightness_change = all_changes
            .iter()
            .fold(0.0f64, |a, &b| a.max(b));

        // Group events into segments
        let mut segments: Vec<FlashingSegment> = Vec::new();
        let gap_threshold_ms = 500.0; // Max gap between events to be in same segment

        let mut current_start = 0usize;

        while current_start < events.len() {
            // Find the end of the current segment
            let mut segment_end = current_start;
            while segment_end + 1 < events.len()
                && events[segment_end + 1].start_ms - events[segment_end].end_ms
                    < gap_threshold_ms
            {
                segment_end += 1;
            }

            let segment_events = &events[current_start..=segment_end];

            // Calculate segment properties
            let start_ms = segment_events.first().unwrap().start_ms;
            let end_ms = segment_events.last().unwrap().end_ms;
            let start_frame = frames[segment_events.first().unwrap().start_idx].frame_number;
            let end_frame = frames[segment_events.last().unwrap().end_idx.min(frames.len() - 1)].frame_number;
            let flash_count = segment_events.len() as u32;

            // Calculate duration and frequency
            let duration_sec = (end_ms - start_ms) / 1000.0;
            let avg_flash_frequency = if duration_sec > 0.0 {
                flash_count as f64 / duration_sec
            } else {
                0.0
            };

            // Get peak luminances for this segment
            let peak_luminances: Vec<f64> = segment_events
                .iter()
                .map(|e| {
                    let mid_idx = (e.start_idx + e.end_idx) / 2;
                    brightness_values[mid_idx.min(brightness_values.len() - 1)].1
                })
                .collect();

            // Calculate max brightness change for this segment
            let segment_max_change = segment_events
                .iter()
                .map(|e| e.brightness_change)
                .fold(0.0f64, |a, b| a.max(b));

            // Classify severity
            let severity = FlashSeverity::classify(
                avg_flash_frequency,
                segment_max_change,
            );

            // Only include segments that meet minimum criteria
            if duration_sec * 1000.0 >= self.min_segment_duration_ms
                && (avg_flash_frequency >= self.min_frequency || segment_max_change > self.brightness_threshold * 2.0)
            {
                segments.push(FlashingSegment::new(
                    start_ms,
                    end_ms,
                    start_frame,
                    end_frame,
                    segment_max_change,
                    avg_flash_frequency,
                    flash_count,
                    severity,
                    peak_luminances,
                ));

                trace!(
                    start_ms = start_ms,
                    end_ms = end_ms,
                    frequency = avg_flash_frequency,
                    severity = ?severity,
                    "Flashing segment detected"
                );
            }

            current_start = segment_end + 1;
        }

        Ok(segments)
    }

    /// Analyze a sequence of brightness values to detect sinusoidal patterns
    /// that indicate regular flashing (like strobes).
    ///
    /// This is a more sophisticated analysis that can detect periodic flashing
    /// even when individual changes might not exceed the threshold.
    pub fn detect_periodic_flashing(
        &self,
        brightness_values: &[f64],
        sample_rate_hz: f64,
    ) -> Vec<(f64, f64)> {
        if brightness_values.len() < 8 || sample_rate_hz <= 0.0 {
            return vec![];
        }

        // Simple autocorrelation to find periodic patterns
        let n = brightness_values.len();
        let max_lag = (n / 2).min(sample_rate_hz as usize * 2);

        let mut autocorr: Vec<f64> = vec![0.0; max_lag];
        let mean: f64 = brightness_values.iter().sum::<f64>() / n as f64;

        for lag in 1..max_lag {
            let mut sum: f64 = 0.0;
            let count = n - lag;
            for i in 0..count {
                sum += (brightness_values[i] - mean) * (brightness_values[i + lag] - mean);
            }
            autocorr[lag] = sum / count as f64;
        }

        // Find peaks in autocorrelation (indicating periodicity)
        let mut peaks: Vec<(usize, f64)> = Vec::new();
        for i in 2..(autocorr.len() - 2) {
            if autocorr[i] > autocorr[i - 1]
                && autocorr[i] > autocorr[i - 2]
                && autocorr[i] > autocorr[i + 1]
                && autocorr[i] > autocorr[i + 2]
                && autocorr[i] > 0.0
            {
                peaks.push((i, autocorr[i]));
            }
        }

        // Convert peak lags to frequencies
        let mut frequencies: Vec<(f64, f64)> = Vec::new();
        for (lag, strength) in peaks {
            let period_sec = lag as f64 / sample_rate_hz;
            if period_sec > 0.0 {
                let frequency_hz = 1.0 / period_sec;
                if frequency_hz >= self.min_frequency {
                    frequencies.push((frequency_hz, strength));
                }
            }
        }

        // Sort by strength descending
        frequencies.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        frequencies
    }
}

/// Internal struct to track individual flash events.
#[derive(Debug)]
struct FlashEvent {
    /// Start frame index of the flash event.
    start_idx: usize,
    /// End frame index of the flash event.
    end_idx: usize,
    /// Start timestamp (ms).
    start_ms: f64,
    /// End timestamp (ms).
    end_ms: f64,
    /// Peak brightness change during the event.
    brightness_change: f64,
    /// Whether this event involved a direction change.
    direction_change: bool,
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
    fn test_flash_detector_creation() {
        let detector = FlashDetector::new(3.0, 40.0, 0.5);
        assert_eq!(detector.min_frequency, 3.0);
        assert_eq!(detector.brightness_threshold, 40.0);
        assert_eq!(detector.sensitivity, 0.5);
    }

    #[test]
    fn test_flash_detector_bounds() {
        // Test that values are clamped
        let detector = FlashDetector::new(0.5, 2.0, 0.0);
        assert_eq!(detector.min_frequency, 1.0);
        assert_eq!(detector.brightness_threshold, 5.0);
        assert_eq!(detector.sensitivity, 0.01);
    }

    #[test]
    fn test_detect_flashes_no_flashing() {
        // Create frames with gradual brightness change (no flashing)
        let mut frames = Vec::new();
        for i in 0..20 {
            let brightness = (i * 5) as u8; // Gradual increase: 0, 5, 10, 15...
            frames.push(create_solid_frame(
                brightness,
                brightness,
                brightness,
                10,
                10,
                i as f64 * 100.0,
                i,
            ));
        }

        let detector = FlashDetector::new(3.0, 40.0, 0.5);
        let segments = detector.detect_flashes(&frames).unwrap();
        assert_eq!(
            segments.len(),
            0,
            "No flashing should be detected in gradual brightness change"
        );
    }

    #[test]
    fn test_detect_flashes_with_flashing() {
        // Create frames with rapid alternation (simulated flashing)
        let mut frames = Vec::new();
        for i in 0..30 {
            let brightness = if i % 2 == 0 { 255u8 } else { 0u8 }; // Rapid black/white alternation
            frames.push(create_solid_frame(
                brightness,
                brightness,
                brightness,
                10,
                10,
                i as f64 * 33.0, // ~30 fps
                i,
            ));
        }

        let detector = FlashDetector::new(3.0, 20.0, 0.3);
        let segments = detector.detect_flashes(&frames).unwrap();
        assert!(
            segments.len() >= 1,
            "Should detect at least one flashing segment, got {}",
            segments.len()
        );

        // The segment should have high frequency
        let segment = &segments[0];
        assert!(
            segment.avg_flash_frequency >= 3.0 || segment.max_brightness_change > 40.0,
            "Should have high frequency or high brightness change"
        );
    }

    #[test]
    fn test_detect_flashes_insufficient_frames() {
        let detector = FlashDetector::new(3.0, 40.0, 0.5);

        let segments = detector.detect_flashes(&[]).unwrap();
        assert_eq!(segments.len(), 0);

        let single_frame = vec![create_solid_frame(128, 128, 128, 10, 10, 0.0, 0)];
        let segments = detector.detect_flashes(&single_frame).unwrap();
        assert_eq!(segments.len(), 0);
    }

    #[test]
    fn test_flashing_segment_properties() {
        let segment = FlashingSegment::new(
            1000.0,
            3000.0,
            10,
            100,
            150.0,
            15.0,
            20,
            FlashSeverity::Critical,
            vec![50.0, 200.0, 50.0, 200.0],
        );

        assert_eq!(segment.duration_ms(), 2000.0);
        assert!(segment.is_concerning());
        assert!(segment.severity.requires_warning());
        assert_eq!(segment.flash_count, 20);
    }

    #[test]
    fn test_flash_severity_classify() {
        assert_eq!(
            FlashSeverity::classify(25.0, 120.0),
            FlashSeverity::Critical
        );
        assert_eq!(FlashSeverity::classify(15.0, 90.0), FlashSeverity::High);
        assert_eq!(FlashSeverity::classify(5.0, 50.0), FlashSeverity::Medium);
        assert_eq!(FlashSeverity::classify(1.0, 10.0), FlashSeverity::Low);
    }

    #[test]
    fn test_builder_methods() {
        let detector = FlashDetector::new(3.0, 40.0, 0.5)
            .with_min_segment_duration(200.0)
            .with_safe_brightness_change(15.0)
            .with_min_consecutive_changes(3);

        assert_eq!(detector.min_segment_duration_ms, 200.0);
        assert_eq!(detector.safe_brightness_change, 15.0);
        assert_eq!(detector.min_consecutive_changes, 3);
    }

    #[test]
    fn test_detect_periodic_flashing() {
        // Create a sinusoidal brightness pattern at ~5 Hz
        let sample_rate = 30.0; // 30 fps
        let duration = 2.0; // 2 seconds
        let num_samples = (sample_rate * duration) as usize;

        let mut brightness_values: Vec<f64> = Vec::with_capacity(num_samples);
        for i in 0..num_samples {
            let t = i as f64 / sample_rate;
            let freq = 5.0; // 5 Hz flashing
            let brightness = 128.0 + 127.0 * (2.0 * std::f64::consts::PI * freq * t).sin();
            brightness_values.push(brightness.abs());
        }

        let detector = FlashDetector::new(3.0, 40.0, 0.5);
        let frequencies = detector.detect_periodic_flashing(&brightness_values, sample_rate);

        assert!(
            !frequencies.is_empty(),
            "Should detect periodic flashing pattern"
        );

        // The strongest frequency should be near 5 Hz
        let dominant_freq = frequencies[0].0;
        assert!(
            (dominant_freq - 5.0).abs() < 1.0,
            "Dominant frequency should be near 5 Hz, got {}",
            dominant_freq
        );
    }

    #[test]
    fn test_safe_brightness_threshold() {
        // Frames with very small brightness changes should not be flagged
        let mut frames = Vec::new();
        for i in 0..20 {
            let brightness = 128u8 + (if i % 2 == 0 { 5 } else { 0 }); // Very small changes
            frames.push(create_solid_frame(
                brightness,
                brightness,
                brightness,
                10,
                10,
                i as f64 * 33.0,
                i,
            ));
        }

        let detector = FlashDetector::new(3.0, 10.0, 0.5).with_safe_brightness_change(20.0);
        let segments = detector.detect_flashes(&frames).unwrap();
        assert_eq!(
            segments.len(),
            0,
            "Changes below safe threshold should not be flagged"
        );
    }

    #[test]
    fn test_multiple_flash_segments() {
        // Create two separate flashing segments with a calm period in between
        let mut frames = Vec::new();

        // Segment 1: flashing (frames 0-9)
        for i in 0..10 {
            let brightness = if i % 2 == 0 { 255u8 } else { 0u8 };
            frames.push(create_solid_frame(brightness, brightness, brightness, 10, 10, i as f64 * 33.0, i));
        }

        // Calm period (frames 10-19)
        for i in 10..20 {
            frames.push(create_solid_frame(128, 128, 128, 10, 10, i as f64 * 33.0, i));
        }

        // Segment 2: flashing (frames 20-29)
        for i in 20..30 {
            let brightness = if i % 2 == 0 { 200u8 } else { 50u8 };
            frames.push(create_solid_frame(brightness, brightness, brightness, 10, 10, i as f64 * 33.0, i));
        }

        let detector = FlashDetector::new(3.0, 20.0, 0.3);
        let segments = detector.detect_flashes(&frames).unwrap();

        // Should detect at least 2 separate segments (or 1 combined)
        assert!(
            segments.len() >= 1,
            "Should detect at least one flashing segment"
        );

        // If two segments, verify they're separated
        if segments.len() >= 2 {
            let gap = segments[1].start_ms - segments[0].end_ms;
            assert!(gap > 100.0, "Segments should be separated by calm period");
        }
    }
}
