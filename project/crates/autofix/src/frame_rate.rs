use std::path::{Path, PathBuf};
use std::process::Stdio;

use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use sentinel_core::FlashingSegment;

use crate::ffmpeg_fixes::MinterpolateParams;
use crate::{AutoFixError, Result};

/// Maximum safe flashing frequency in Hz (WCAG guideline: 3 flashes per second)
const MAX_SAFE_FLASH_RATE_HZ: f64 = 3.0;

/// Engine for frame rate and motion-related adjustments
///
/// Handles flashing segment slowdown (for photosensitive epilepsy prevention)
/// and motion smoothing using frame interpolation.
pub struct FrameRateEngine {
    max_safe_flash_rate: f64,
    default_interpolation_fps: f64,
}

impl FrameRateEngine {
    /// Create a new FrameRateEngine with default settings
    pub fn new() -> Self {
        info!(
            "Initializing FrameRateEngine (safe_flash_rate={}Hz)",
            MAX_SAFE_FLASH_RATE_HZ
        );
        Self {
            max_safe_flash_rate: MAX_SAFE_FLASH_RATE_HZ,
            default_interpolation_fps: 60.0,
        }
    }

    /// Create with custom safe flash rate
    pub fn with_safe_rate(hz: f64) -> Self {
        Self {
            max_safe_flash_rate: hz.max(1.0).min(10.0),
            default_interpolation_fps: 60.0,
        }
    }

    /// Adjust a flashing segment to a safe frame rate
    ///
    /// Uses FFmpeg's setpts filter to slow down segments with rapid flashing
    /// that could trigger photosensitive epilepsy. The method intelligently
    /// calculates the slowdown factor needed to bring the flash rate below
    /// the safe threshold.
    ///
    /// For segments that are too fast, the video is slowed using setpts
    /// and optionally minterpolate is applied to smooth the result.
    #[instrument(skip(self, segment))]
    pub async fn adjust_flashing_segment(
        &self,
        video_path: &Path,
        segment: &FlashingSegment,
        target_hz: f64,
    ) -> Result<PathBuf> {
        let target_hz = target_hz.max(1.0).min(self.max_safe_flash_rate);
        let start = segment.start_time;
        let end = segment.end_time;
        let original_hz = segment.frequency_hz;
        let segment_duration = end - start;

        info!(
            "Adjusting flashing segment [{:.3}s - {:.3}s]: {}Hz -> {}Hz",
            start, end, original_hz, target_hz
        );

        if start >= end || start < 0.0 {
            return Err(AutoFixError::InvalidTimeRange { start, end });
        }

        if original_hz <= target_hz {
            warn!(
                "Flash rate {:.1}Hz already below target {:.1}Hz, returning original",
                original_hz, target_hz
            );
            return Ok(video_path.to_path_buf());
        }

        // Calculate slowdown factor
        let slowdown_factor = original_hz / target_hz;
        let setpts_expr = format!("{:.6}*PTS", slowdown_factor);

        // The adjusted segment will be longer
        let adjusted_duration = segment_duration * slowdown_factor;

        debug!(
            "Slowdown factor: {:.2}x, adjusted duration: {:.2}s (from {:.2}s)",
            slowdown_factor, adjusted_duration, segment_duration
        );

        // Generate output path
        let output_path = self.temp_output(video_path, "flash_adjust")?;

        // Strategy: extract the segment, slow it down with setpts,
        // optionally apply motion smoothing, then concatenate back

        // Get video info for proper handling
        let video_info = self.probe_video_info(video_path).await?;

        // Build FFmpeg command
        let filter_complex = self.build_flash_filter(
            start,
            end,
            slowdown_factor,
            &video_info,
        );

        debug!("Flash adjustment filter: {}", filter_complex);

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "[outv]".to_string(),
            "-map".to_string(), "[outa]".to_string(),
            "-c:v".to_string(), "libx264".to_string(),
            "-preset".to_string(), "medium".to_string(),
            "-crf".to_string(), "23".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await?;

        // Now stitch the adjusted segment back into the original
        let final_output = if start > 0.0 {
            self.stitch_with_original(video_path, &output_path, start, end, adjusted_duration)
                .await?
        } else {
            output_path
        };

        info!(
            "Flash adjustment complete: {} -> {}",
            video_path.display(),
            final_output.display()
        );
        Ok(final_output)
    }

    /// Apply motion smoothing to a segment using frame interpolation
    ///
    /// Uses FFmpeg's minterpolate filter to generate intermediate frames
    /// and create smoother motion. The target FPS determines the level
    /// of interpolation.
    #[instrument(skip(self))]
    pub async fn smooth_motion(
        &self,
        video_path: &Path,
        start: f64,
        end: f64,
        target_fps: f64,
    ) -> Result<PathBuf> {
        let target_fps = target_fps.max(24.0).min(120.0);
        info!(
            "Smoothing motion [{:.3}s - {:.3}s] to {:.1f}fps",
            start, end, target_fps
        );

        if start >= end || start < 0.0 {
            return Err(AutoFixError::InvalidTimeRange { start, end });
        }

        let output_path = self.temp_output(video_path, "motion_smooth")?;
        let segment_duration = end - start;

        // Build minterpolate parameters
        let interp_params = MinterpolateParams {
            fps: target_fps,
            mi_mode: crate::ffmpeg_fixes::MinterpolateMode::MotionCompensated,
            mc_mode: "aobmc".to_string(),
            me_mode: "bidir".to_string(),
            me: "esa".to_string(),
            vsbmc: true,
            mb_size: 16,
        };

        let minterpolate_filter = interp_params.to_filter_string();

        // Build the complete filter graph
        let filter_complex = format!(
            "[0:v]trim=start={start:.3}:end={end:.3},setpts=PTS-STARTPTS,\
             {minterpolate}[v_smooth];\
             \
             [0:a]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS[a_seg];\
             \
             [v_smooth][a_seg]concat=n=1:v=1:a=1[outv][outa]",
            start = start,
            end = end,
            minterpolate = minterpolate_filter,
        );

        debug!("Motion smoothing filter: {}", filter_complex);

        let args = vec![
            "-y".to_string(),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "[outv]".to_string(),
            "-map".to_string(), "[outa]".to_string(),
            "-c:v".to_string(), "libx264".to_string(),
            "-preset".to_string(), "medium".to_string(),
            "-crf".to_string(), "23".to_string(),
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&args).await?;

        // Stitch back if needed
        let final_output = if start > 0.0 {
            self.stitch_with_original(video_path, &output_path, start, end, segment_duration)
                .await?
        } else {
            output_path
        };

        info!(
            "Motion smoothing complete: {} -> {}",
            video_path.display(),
            final_output.display()
        );
        Ok(final_output)
    }

    /// Batch process multiple flashing segments
    #[instrument(skip(self, segments))]
    pub async fn adjust_all_flashing_segments(
        &self,
        video_path: &Path,
        segments: &[FlashingSegment],
        target_hz: f64,
    ) -> Result<PathBuf> {
        info!("Adjusting {} flashing segments", segments.len());

        if segments.is_empty() {
            return Ok(video_path.to_path_buf());
        }

        let mut current_path = video_path.to_path_buf();
        let mut time_offset = 0.0;

        for (i, segment) in segments.iter().enumerate() {
            // Adjust time for accumulated offset from previous slowdowns
            let adjusted_start = segment.start_time + time_offset;
            let adjusted_end = segment.end_time + time_offset;

            let adjusted_segment = FlashingSegment {
                start_time: adjusted_start,
                end_time: adjusted_end,
                ..segment.clone()
            };

            let result = self
                .adjust_flashing_segment(&current_path, &adjusted_segment, target_hz)
                .await?;

            // Calculate time added by this slowdown
            let original_duration = segment.end_time - segment.start_time;
            let slowdown = segment.frequency_hz / target_hz.max(1.0);
            let added_time = original_duration * (slowdown - 1.0);
            time_offset += added_time;

            current_path = result;
            debug!("Segment {} processed, cumulative offset: +{:.2}s", i, time_offset);
        }

        info!(
            "All {} flashing segments adjusted, total time added: +{:.2}s",
            segments.len(),
            time_offset
        );
        Ok(current_path)
    }

    /// Calculate the required slowdown factor for a given flash rate
    pub fn calculate_slowdown_factor(&self, original_hz: f64, target_hz: f64) -> f64 {
        if original_hz <= target_hz {
            1.0
        } else {
            (original_hz / target_hz).max(1.0)
        }
    }

    /// Estimate the processing time for flashing adjustments
    pub fn estimate_processing_time(&self, segment_duration: f64, slowdown_factor: f64) -> f64 {
        // Processing time roughly scales with output duration
        let output_duration = segment_duration * slowdown_factor;
        // Base overhead + per-second processing
        2.0 + output_duration * 1.5
    }

    // ---- Internal helpers ----

    /// Build the FFmpeg filter for flash rate adjustment
    fn build_flash_filter(
        &self,
        start: f64,
        end: f64,
        slowdown_factor: f64,
        video_info: &VideoInfo,
    ) -> String {
        let segment_duration = end - start;
        let setpts_expr = format!("{:.6}*PTS", slowdown_factor);

        // For audio, we also need to slow down to maintain sync
        // Use atempo (which only supports 0.5-2.0 range)
        // For larger slowdowns, chain multiple atempo filters
        let atempo_value = 1.0 / slowdown_factor;
        let audio_filter = if atempo_value >= 0.5 && atempo_value <= 2.0 {
            format!("atempo={:.4}", atempo_value)
        } else if atempo_value < 0.5 {
            // Chain multiple atempo filters for larger slowdown
            let chain_count = (0.5 / atempo_value).ceil() as usize;
            let per_filter = atempo_value.powf(1.0 / chain_count as f64);
            let filters: Vec<String> = (0..chain_count)
                .map(|_| format!("atempo={:.4}", per_filter))
                .collect();
            filters.join(",")
        } else {
            // atempo > 2.0, chain multiple
            let chain_count = (atempo_value / 2.0).ceil() as usize;
            let per_filter = atempo_value.powf(1.0 / chain_count as f64);
            let filters: Vec<String> = (0..chain_count)
                .map(|_| format!("atempo={:.4}", per_filter))
                .collect();
            filters.join(",")
        };

        // Build complete filter graph
        // We split the video, process the flashing segment, and concatenate
        format!(
            "[0:v]trim=start={start:.3}:end={end:.3},setpts=PTS-STARTPTS,\
             setpts={setpts},\
             fade=t=in:st=0:d=0.1,\
             fade=t=out:st={fade_out:.3}:d=0.1[v_slow];\
             \
             [0:a]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS,\
             {audio_filter},\
             afade=t=in:st=0:d=0.1,\
             afade=t=out:st={fade_out:.3}:d=0.1[a_slow];\
             \
             [0:v]trim=start=0:end={start:.3},setpts=PTS-STARTPTS[v_before];\
             [0:v]trim=start={end:.3},setpts=PTS-STARTPTS[v_after];\
             \
             [0:a]atrim=start=0:end={start:.3},asetpts=PTS-STARTPTS[a_before];\
             [0:a]atrim=start={end:.3},asetpts=PTS-STARTPTS[a_after];\
             \
             [v_before][a_before][v_slow][a_slow][v_after][a_after]\
             concat=n=3:v=1:a=1[outv][outa]",
            start = start,
            end = end,
            setpts = setpts_expr,
            fade_out = segment_duration * slowdown_factor - 0.1,
            audio_filter = audio_filter,
        )
    }

    /// Stitch a processed segment back into the original video
    async fn stitch_with_original(
        &self,
        _original: &Path,
        segment: &Path,
        _start: f64,
        _end: f64,
        _adjusted_duration: f64,
    ) -> Result<PathBuf> {
        // For simplicity, return the processed segment
        // In a full implementation, this would concatenate:
        // original[0..start] + processed_segment + original[end..]
        Ok(segment.to_path_buf())
    }

    /// Probe video information
    async fn probe_video_info(&self, video_path: &Path) -> Result<VideoInfo> {
        let output = tokio::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height,r_frame_rate,duration",
                "-of", "json",
                video_path.to_str().unwrap_or(""),
            ])
            .output()
            .await
            .map_err(|e| {
                AutoFixError::FfmpegError(format!("ffprobe failed: {}", e))
            })?;

        let info: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(AutoFixError::Serialization)?;

        let stream = info.get("streams")
            .and_then(|s| s.as_array())
            .and_then(|a| a.first())
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let width = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(1920) as u32;
        let height = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(1080) as u32;
        let fps = stream.get("r_frame_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                s.split_once('/')
                    .and_then(|(n, d)| {
                        let num: f64 = n.parse().ok()?;
                        let den: f64 = d.parse().ok()?;
                        Some(num / den)
                    })
            })
            .unwrap_or(30.0);
        let duration = stream.get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);

        Ok(VideoInfo {
            width,
            height,
            fps,
            duration,
        })
    }

    /// Generate a temporary output path
    fn temp_output(&self, source: &Path, suffix: &str) -> Result<PathBuf> {
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("video");
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let temp = std::env::temp_dir().join(format!("{}_{}_{}.{}", stem, suffix, Uuid::new_v4(), ext));
        Ok(temp)
    }

    /// Run FFmpeg with given arguments
    async fn run_ffmpeg(&self, args: &[String]) -> Result<()> {
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        debug!("Running FFmpeg with {} args", args.len());

        let output = tokio::process::Command::new("ffmpeg")
            .args(&args_refs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AutoFixError::FfmpegNotFound
                } else {
                    AutoFixError::FfmpegError(format!("FFmpeg execution failed: {}", e))
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AutoFixError::FfmpegError(format!(
                "FFmpeg exited with code {:?}: {}",
                output.status.code(),
                stderr.chars().take(500).collect::<String>()
            )));
        }

        debug!("FFmpeg completed successfully");
        Ok(())
    }
}

impl Default for FrameRateEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Video metadata for filter construction
#[derive(Debug, Clone)]
struct VideoInfo {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_rate_engine_creation() {
        let engine = FrameRateEngine::new();
        assert_eq!(engine.max_safe_flash_rate, MAX_SAFE_FLASH_RATE_HZ);
        assert_eq!(engine.default_interpolation_fps, 60.0);
    }

    #[test]
    fn test_frame_rate_engine_custom_rate() {
        let engine = FrameRateEngine::with_safe_rate(5.0);
        assert_eq!(engine.max_safe_flash_rate, 5.0);
    }

    #[test]
    fn test_custom_rate_clamping() {
        let engine_high = FrameRateEngine::with_safe_rate(15.0);
        assert_eq!(engine_high.max_safe_flash_rate, 10.0); // clamped

        let engine_low = FrameRateEngine::with_safe_rate(0.5);
        assert_eq!(engine_low.max_safe_flash_rate, 1.0); // clamped
    }

    #[test]
    fn test_calculate_slowdown_factor() {
        let engine = FrameRateEngine::new();

        // No slowdown needed
        assert_eq!(engine.calculate_slowdown_factor(2.0, 3.0), 1.0);

        // 2x slowdown needed
        assert_eq!(engine.calculate_slowdown_factor(6.0, 3.0), 2.0);

        // 3x slowdown
        assert_eq!(engine.calculate_slowdown_factor(9.0, 3.0), 3.0);
    }

    #[test]
    fn test_estimate_processing_time() {
        let engine = FrameRateEngine::new();

        let time = engine.estimate_processing_time(5.0, 2.0);
        assert!(time > 2.0); // Base overhead
        // 5.0s * 2.0 = 10s output, 10 * 1.5 = 15 + 2 = 17
        assert_eq!(time, 17.0);
    }

    #[test]
    fn test_build_flash_filter_contains_setpts() {
        let engine = FrameRateEngine::new();
        let video_info = VideoInfo {
            width: 1920,
            height: 1080,
            fps: 30.0,
            duration: 60.0,
        };

        let filter = engine.build_flash_filter(5.0, 10.0, 2.0, &video_info);

        assert!(filter.contains("setpts="));
        assert!(filter.contains("atempo="));
        assert!(filter.contains("concat="));
        assert!(filter.contains("[outv]"));
        assert!(filter.contains("[outa]"));
    }

    #[test]
    fn test_build_flash_filter_atempo_chaining() {
        let engine = FrameRateEngine::new();
        let video_info = VideoInfo {
            width: 1920,
            height: 1080,
            fps: 30.0,
            duration: 60.0,
        };

        // Large slowdown (4x) means atempo = 0.25 which requires chaining
        let filter = engine.build_flash_filter(5.0, 10.0, 4.0, &video_info);
        assert!(filter.contains("atempo="));
        // Should have multiple atempo filters chained
        let atempo_count = filter.matches("atempo=").count();
        assert!(atempo_count >= 2, "Large slowdown should chain atempo filters");
    }

    #[test]
    fn test_flash_segment_below_threshold() {
        let engine = FrameRateEngine::new();
        let segment = FlashingSegment {
            start_time: 5.0,
            end_time: 10.0,
            frequency_hz: 2.0, // Already below 3Hz
            intensity: 0.8,
            frames_affected: vec![150, 151, 152],
        };

        // Since original_hz (2.0) <= target_hz (3.0), should return original path
        // (this is tested at the apply level, here we just verify logic)
        let slowdown = engine.calculate_slowdown_factor(segment.frequency_hz, 3.0);
        assert_eq!(slowdown, 1.0);
    }

    #[test]
    fn test_invalid_time_range() {
        // Verify that InvalidTimeRange is properly constructed
        let err = AutoFixError::InvalidTimeRange { start: 10.0, end: 5.0 };
        let msg = format!("{}", err);
        assert!(msg.contains("10"));
        assert!(msg.contains("5"));
    }
}
