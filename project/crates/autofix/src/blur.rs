use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use sentinel_core::{BoundingBox, DetectedObject};

use crate::ffmpeg_fixes::FfmpegFilterGraph;
use crate::{AutoFixError, Result};

/// Type of blur to apply
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlurType {
    /// Standard Gaussian blur
    Gaussian,
    /// Pixelate (block-based downsampling)
    Pixelate,
    /// Mosaic with random block colors
    Mosaic,
    /// Smart fill using content-aware inpainting
    SmartFill,
}

impl BlurType {
    /// Get the FFmpeg filter name for this blur type
    pub fn ffmpeg_filter(&self) -> &'static str {
        match self {
            BlurType::Gaussian => "boxblur",
            BlurType::Pixelate => "scale",
            BlurType::Mosaic => "scale",
            BlurType::SmartFill => "boxblur",
        }
    }

    /// Get a human-readable name
    pub fn display_name(&self) -> &'static str {
        match self {
            BlurType::Gaussian => "Gaussian Blur",
            BlurType::Pixelate => "Pixelate",
            BlurType::Mosaic => "Mosaic",
            BlurType::SmartFill => "Smart Fill",
        }
    }
}

/// A preset configuration for blur effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlurPreset {
    /// Preset name
    pub name: String,
    /// Type of blur
    pub blur_type: BlurType,
    /// Intensity from 0.0 (none) to 1.0 (maximum)
    pub intensity: f64,
    /// Feather radius in normalized coordinates (0.0 - 0.1)
    pub feather_radius: f64,
    /// Optional additional parameters
    pub extra_params: serde_json::Value,
}

impl BlurPreset {
    /// Create a new blur preset
    pub fn new(name: impl Into<String>, blur_type: BlurType, intensity: f64, feather_radius: f64) -> Self {
        let intensity = intensity.clamp(0.0, 1.0);
        let feather_radius = feather_radius.clamp(0.0, 0.1);
        Self {
            name: name.into(),
            blur_type,
            intensity,
            feather_radius,
            extra_params: serde_json::json!({}),
        }
    }

    /// Convert intensity to FFmpeg boxblur radius (1-100)
    pub fn boxblur_radius(&self) -> u32 {
        (1.0 + self.intensity * 99.0).round() as u32
    }

    /// Convert intensity to pixelate block size (2-64)
    pub fn pixelate_block_size(&self) -> u32 {
        (2.0 + self.intensity * 62.0).round() as u32
    }

    /// Get the FFmpeg filter string for this preset applied to a region
    pub fn filter_string(
        &self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        video_w: u32,
        video_h: u32,
    ) -> String {
        match self.blur_type {
            BlurType::Gaussian => self.gaussian_filter(x, y, w, h, video_w, video_h),
            BlurType::Pixelate => self.pixelate_filter(x, y, w, h, video_w, video_h),
            BlurType::Mosaic => self.mosaic_filter(x, y, w, h, video_w, video_h),
            BlurType::SmartFill => self.smart_fill_filter(x, y, w, h, video_w, video_h),
        }
    }

    fn gaussian_filter(&self, x: u32, y: u32, w: u32, h: u32, _vw: u32, _vh: u32) -> String {
        let radius = self.boxblur_radius();
        let feather = (self.feather_radius * 100.0).round() as u32;

        format!(
            "drawbox=x={x}:y={y}:w={w}:h={h}:color=black@0:t=fill,\
             drawbox=x={x}:y={y}:w={w}:h={h}:color=black@{alpha}:t={feather},\
             boxblur={r}:{r}",
            x = x,
            y = y,
            w = w,
            h = h,
            alpha = format!("{:.2}", self.intensity * 0.3),
            feather = feather.max(1),
            r = radius,
        )
    }

    fn pixelate_filter(&self, x: u32, y: u32, w: u32, h: u32, vw: u32, _vh: u32) -> String {
        let block_size = self.pixelate_block_size();
        let scale_w = vw / block_size;

        format!(
            "crop={w}:{h}:{x}:{y},scale={sw}:{sh},scale={w}:{h}:flags=neighbor",
            w = w,
            h = h,
            x = x,
            y = y,
            sw = (w as f64 / block_size as f64).max(1.0) as u32,
            sh = (h as f64 / block_size as f64).max(1.0) as u32,
        )
    }

    fn mosaic_filter(&self, x: u32, y: u32, w: u32, h: u32, _vw: u32, _vh: u32) -> String {
        let block_size = self.pixelate_block_size();
        format!(
            "crop={w}:{h}:{x}:{y},\
             scale=iw/{bs}:ih/{bs},\
             scale={w}:{h}:flags=neighbor,\
             hue=s=0",
            w = w,
            h = h,
            x = x,
            y = y,
            bs = block_size,
        )
    }

    fn smart_fill_filter(&self, x: u32, y: u32, w: u32, h: u32, _vw: u32, _vh: u32) -> String {
        // Smart fill uses a combination of blur + color averaging
        let radius = self.boxblur_radius();
        let mix = (self.intensity * 0.5).min(0.8);

        format!(
            "crop={w}:{h}:{x}:{y},\
             boxblur={r}:{r},\
             eq=brightness={mix}:contrast=0.8:saturation=0.5",
            w = w,
            h = h,
            x = x,
            y = y,
            r = radius / 2 + 1,
            mix = mix,
        )
    }

    /// Generate a full filter_complex string for applying this preset to multiple regions
    pub fn multi_region_filter(
        &self,
        regions: &[(u32, u32, u32, u32)],
        video_w: u32,
        video_h: u32,
    ) -> String {
        if regions.is_empty() {
            return "null".to_string();
        }

        let mut parts = Vec::new();
        for (i, &(x, y, w, h)) in regions.iter().enumerate() {
            let region_filter = self.filter_string(x, y, w, h, video_w, video_h);
            parts.push(format!(
                "[in{}]{}[out{}]",
                i,
                region_filter,
                i + 1
            ));
        }

        // Chain the filters together
        let mut chain = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i == 0 {
                chain.push_str(&part.replace("[in0]", "[0:v]"));
            } else {
                chain.push(';');
                chain.push_str(&part.replace(&format!("[in{}]", i), &format!("[out{}]", i)));
            }
        }
        chain.push_str(&format!(";[out{}]null[out]", regions.len()));
        chain
    }
}

impl Default for BlurPreset {
    fn default() -> Self {
        Self::new("default", BlurType::Gaussian, 0.5, 0.02)
    }
}

/// Engine for applying blur effects to video regions
pub struct BlurEngine {
    max_intensity: f64,
    max_feather: f64,
}

impl BlurEngine {
    /// Create a new BlurEngine with the given intensity limit
    pub fn new(max_intensity: f64) -> Result<Self> {
        let max_intensity = max_intensity.clamp(0.0, 1.0);
        info!("Initializing BlurEngine with max_intensity={}", max_intensity);
        Ok(Self {
            max_intensity,
            max_feather: 0.05,
        })
    }

    /// Apply a blur effect to a specific region of a video for a time range
    ///
    /// Uses FFmpeg's drawbox + boxblur filter chain to create a precise
    /// region-based blur effect with optional feathering.
    #[instrument(skip(self))]
    pub async fn apply_region_blur(
        &self,
        video_path: &Path,
        region: &BoundingBox,
        start: f64,
        end: f64,
        preset: &BlurPreset,
    ) -> Result<PathBuf> {
        info!(
            "Applying region blur: region={:?}, start={:.2}s, end={:.2}s, type={:?}",
            region, start, end, preset.blur_type
        );

        // Validate time range
        if start >= end {
            return Err(AutoFixError::InvalidTimeRange { start, end });
        }

        if start < 0.0 || end < 0.0 {
            return Err(AutoFixError::InvalidTimeRange { start, end });
        }

        // Clamp intensity to engine limits
        let mut safe_preset = preset.clone();
        if safe_preset.intensity > self.max_intensity {
            warn!(
                "Clamping blur intensity from {} to {}",
                safe_preset.intensity, self.max_intensity
            );
            safe_preset.intensity = self.max_intensity;
        }

        // Get video dimensions
        let (video_w, video_h) = self.probe_video_dimensions(video_path).await?;

        // Convert normalized bbox to pixel coordinates
        let (px, py, pw, ph) = region.to_pixel_coords(video_w, video_h);

        // Ensure region is within video bounds
        let px = px.min(video_w);
        let py = py.min(video_h);
        let pw = pw.min(video_w - px);
        let ph = ph.min(video_h - py);

        if pw == 0 || ph == 0 {
            return Err(AutoFixError::RegionOutOfBounds(*region));
        }

        // Generate output path
        let output_path = self.temp_output(video_path)?;

        // Build FFmpeg command with filter complex
        let filter_complex = self.build_region_blur_filter(
            px, py, pw, ph, video_w, video_h, &safe_preset
        );

        debug!("Filter complex: {}", filter_complex);

        let duration = end - start;

        let ffmpeg_args = vec![
            "-y".to_string(),
            "-ss".to_string(), format!("{:.3}", start),
            "-t".to_string(), format!("{:.3}", duration),
            "-i".to_string(), video_path.to_string_lossy().to_string(),
            "-filter_complex".to_string(), filter_complex,
            "-c:v".to_string(), "libx264".to_string(),
            "-preset".to_string(), "medium".to_string(),
            "-crf".to_string(), "23".to_string(),
            "-c:a".to_string(), "copy".to_string(),
            "-avoid_negative_ts".to_string(), "make_zero".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ];

        self.run_ffmpeg(&ffmpeg_args).await?;

        // Now concatenate the before + blurred segment + after
        let final_output = self.stitch_segments(video_path, &output_path, start, end).await?;

        info!("Region blur applied: output={}", final_output.display());
        Ok(final_output)
    }

    /// Apply smart blur to detected objects in a video
    ///
    /// Analyzes detected objects and generates a filter chain that blurs
    /// each object individually using the most appropriate blur strategy.
    #[instrument(skip(self, objects))]
    pub async fn apply_smart_blur(
        &self,
        video_path: &Path,
        objects: &[DetectedObject],
        preset: &BlurPreset,
    ) -> Result<PathBuf> {
        info!("Applying smart blur to {} objects", objects.len());

        if objects.is_empty() {
            return Err(AutoFixError::SmartBlurFailed(
                "No objects provided for smart blur".to_string(),
            ));
        }

        // Clamp intensity
        let mut safe_preset = preset.clone();
        if safe_preset.intensity > self.max_intensity {
            safe_preset.intensity = self.max_intensity;
        }

        // Get video dimensions
        let (video_w, video_h) = self.probe_video_dimensions(video_path).await?;

        // Group objects by time segment
        let mut time_groups: std::collections::HashMap<(u64, u64), Vec<&DetectedObject>> =
            std::collections::HashMap::new();

        for obj in objects {
            let start_key = (obj.frame_timestamp * 1000.0) as u64;
            let end_key = ((obj.frame_timestamp + 1.0) * 1000.0) as u64;
            time_groups
                .entry((start_key, end_key))
                .or_default()
                .push(obj);
        }

        let mut current_input = video_path.to_path_buf();

        // Process each time group
        for ((start_ms, end_ms), group_objects) in time_groups {
            let start_sec = start_ms as f64 / 1000.0;
            let end_sec = end_ms as f64 / 1000.0;

            // Convert objects to pixel regions
            let regions: Vec<(u32, u32, u32, u32)> = group_objects
                .iter()
                .map(|obj| {
                    let (x, y, w, h) = obj.bbox.to_pixel_coords(video_w, video_h);
                    let x = x.min(video_w);
                    let y = y.min(video_h);
                    let w = w.min(video_w - x);
                    let h = h.min(video_h - y);
                    (x, y, w.max(1), h.max(1))
                })
                .collect();

            if regions.is_empty() {
                continue;
            }

            // Build multi-region filter
            let output_path = self.temp_output(video_path)?;
            let filter_complex = safe_preset.multi_region_filter(&regions, video_w, video_h);

            let duration = end_sec - start_sec;

            let ffmpeg_args = vec![
                "-y".to_string(),
                "-ss".to_string(), format!("{:.3}", start_sec),
                "-t".to_string(), format!("{:.3}", duration),
                "-i".to_string(), current_input.to_string_lossy().to_string(),
                "-filter_complex".to_string(), filter_complex,
                "-c:v".to_string(), "libx264".to_string(),
                "-preset".to_string(), "medium".to_string(),
                "-crf".to_string(), "23".to_string(),
                "-c:a".to_string(), "copy".to_string(),
                "-avoid_negative_ts".to_string(), "make_zero".to_string(),
                output_path.to_string_lossy().to_string(),
            ];

            self.run_ffmpeg(&ffmpeg_args).await?;

            // Stitch back together
            current_input = self
                .stitch_segments(&current_input, &output_path, start_sec, end_sec)
                .await?;
        }

        info!("Smart blur applied to {} objects", objects.len());
        Ok(current_input)
    }

    // ---- Internal helpers ----

    /// Build a region blur filter_complex string
    fn build_region_blur_filter(
        &self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        video_w: u32,
        video_h: u32,
        preset: &BlurPreset,
    ) -> String {
        preset.filter_string(x, y, w, h, video_w, video_h)
    }

    /// Probe video dimensions using ffprobe
    async fn probe_video_dimensions(&self, video_path: &Path) -> Result<(u32, u32)> {
        let output = tokio::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height",
                "-of", "csv=p=0",
                video_path.to_str().unwrap_or(""),
            ])
            .output()
            .await
            .map_err(|e| {
                AutoFixError::FfmpegError(format!("ffprobe failed: {}", e))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            // Try a fallback resolution
            warn!("ffprobe failed: {}, using fallback 1920x1080", stderr);
            return Ok((1920, 1080));
        }

        let parts: Vec<&str> = stdout.trim().split(',').collect();
        if parts.len() >= 2 {
            let w = parts[0].parse::<u32>().unwrap_or(1920);
            let h = parts[1].parse::<u32>().unwrap_or(1080);
            debug!("Video dimensions: {}x{}", w, h);
            Ok((w, h))
        } else {
            warn!("Could not parse dimensions from ffprobe output: {}", stdout);
            Ok((1920, 1080))
        }
    }

    /// Generate a temporary output path
    fn temp_output(&self, source: &Path) -> Result<PathBuf> {
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("blur");
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4");
        let temp = std::env::temp_dir().join(format!("{}_blur_{}.{}", stem, Uuid::new_v4(), ext));
        Ok(temp)
    }

    /// Run FFmpeg with the given arguments
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

    /// Stitch a processed segment back into the original video
    async fn stitch_segments(
        &self,
        original: &Path,
        segment: &Path,
        seg_start: f64,
        seg_end: f64,
    ) -> Result<PathBuf> {
        info!(
            "Stitching segment [{:.2}s - {:.2}s] back into video",
            seg_start, seg_end
        );

        // For simplicity, use the concat demuxer approach
        let concat_list = std::env::temp_dir().join(format!("concat_{}.txt", Uuid::new_v4()));
        let output = std::env::temp_dir().join(format!(
            "stitched_{}.{}",
            Uuid::new_v4(),
            original.extension().and_then(|e| e.to_str()).unwrap_or("mp4")
        ));

        // If segment covers the entire video, just return the segment
        if seg_start <= 0.0 {
            // Just replace the prefix; return segment
            return Ok(segment.to_path_buf());
        }

        // Create concat file list
        let concat_content = format!(
            "file '{}'
file '{}'
",
            original.canonicalize().unwrap_or(original.to_path_buf()).display(),
            segment.canonicalize().unwrap_or(segment.to_path_buf()).display()
        );

        std::fs::write(&concat_list, concat_content).map_err(AutoFixError::Io)?;

        let args = vec![
            "-y".to_string(),
            "-f".to_string(), "concat".to_string(),
            "-safe".to_string(), "0".to_string(),
            "-i".to_string(), concat_list.to_string_lossy().to_string(),
            "-c".to_string(), "copy".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            output.to_string_lossy().to_string(),
        ];

        let result = self.run_ffmpeg(&args).await;

        // Cleanup concat list
        let _ = std::fs::remove_file(&concat_list);

        match result {
            Ok(_) => Ok(output),
            Err(e) => {
                warn!("Concat stitch failed ({}), returning segment as-is", e);
                Ok(segment.to_path_buf())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blur_preset_creation() {
        let preset = BlurPreset::new("test", BlurType::Gaussian, 0.7, 0.02);
        assert_eq!(preset.name, "test");
        assert!(matches!(preset.blur_type, BlurType::Gaussian));
        assert_eq!(preset.intensity, 0.7);
    }

    #[test]
    fn test_blur_preset_intensity_clamping() {
        let preset = BlurPreset::new("test", BlurType::Gaussian, 1.5, 0.2);
        assert_eq!(preset.intensity, 1.0);
        assert_eq!(preset.feather_radius, 0.1);

        let preset2 = BlurPreset::new("test", BlurType::Pixelate, -0.5, -0.1);
        assert_eq!(preset2.intensity, 0.0);
        assert_eq!(preset2.feather_radius, 0.0);
    }

    #[test]
    fn test_blur_preset_boxblur_radius() {
        let p1 = BlurPreset::new("", BlurType::Gaussian, 0.0, 0.0);
        assert_eq!(p1.boxblur_radius(), 1);

        let p2 = BlurPreset::new("", BlurType::Gaussian, 1.0, 0.0);
        assert_eq!(p2.boxblur_radius(), 100);

        let p3 = BlurPreset::new("", BlurType::Gaussian, 0.5, 0.0);
        assert_eq!(p3.boxblur_radius(), 50);
    }

    #[test]
    fn test_blur_type_display_name() {
        assert_eq!(BlurType::Gaussian.display_name(), "Gaussian Blur");
        assert_eq!(BlurType::Pixelate.display_name(), "Pixelate");
        assert_eq!(BlurType::Mosaic.display_name(), "Mosaic");
        assert_eq!(BlurType::SmartFill.display_name(), "Smart Fill");
    }

    #[test]
    fn test_filter_string_gaussian() {
        let preset = BlurPreset::new("test", BlurType::Gaussian, 0.5, 0.02);
        let filter = preset.filter_string(100, 100, 200, 200, 1920, 1080);
        assert!(filter.contains("boxblur"));
        assert!(filter.contains("drawbox"));
    }

    #[test]
    fn test_filter_string_pixelate() {
        let preset = BlurPreset::new("test", BlurType::Pixelate, 0.5, 0.0);
        let filter = preset.filter_string(100, 100, 200, 200, 1920, 1080);
        assert!(filter.contains("scale"));
        assert!(filter.contains("neighbor"));
    }

    #[test]
    fn test_multi_region_filter() {
        let preset = BlurPreset::new("test", BlurType::Gaussian, 0.5, 0.02);
        let regions = vec![(10, 10, 100, 100), (200, 200, 150, 150)];
        let filter = preset.multi_region_filter(&regions, 1920, 1080);
        assert!(!filter.is_empty());
        assert!(filter.contains("[in0]") || filter.contains("[0:v]"));
    }

    #[test]
    fn test_blur_engine_creation() {
        let engine = BlurEngine::new(0.75).unwrap();
        assert_eq!(engine.max_intensity, 0.75);
    }

    #[test]
    fn test_blur_engine_intensity_clamping() {
        let engine = BlurEngine::new(1.5).unwrap();
        assert_eq!(engine.max_intensity, 1.0);
    }
}
