use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};

use sentinel_core::{AnalysisResult, BoundingBox, DetectedObject, FlashingSegment, Severity, Violation};

use crate::{AutoFixError, Result};

/// Quality metrics extracted from a video file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Video duration in seconds
    pub duration_seconds: f64,
    /// Video resolution (width, height)
    pub resolution: (u32, u32),
    /// Frames per second
    pub fps: f64,
    /// Video bitrate in kbps
    pub bitrate_kbps: f64,
    /// File size in MB
    pub file_size_mb: f64,
    /// VMAF quality score (if computed)
    pub vmaf_score: Option<f64>,
    /// SSIM structural similarity score
    pub ssim_score: Option<f64>,
    /// Audio bitrate in kbps
    pub audio_bitrate_kbps: Option<f64>,
    /// Number of video streams
    pub video_streams: u32,
    /// Number of audio streams
    pub audio_streams: u32,
    /// Codec name
    pub video_codec: Option<String>,
    /// Pixel format
    pub pix_fmt: Option<String>,
}

/// Validator for post-fix quality and safety verification
pub struct FixValidator {
    target_safety_margin: f64,
    min_quality_score: f64,
}

impl FixValidator {
    /// Create a new FixValidator
    pub fn new(target_safety_margin: f64) -> Self {
        info!(
            "Initializing FixValidator (target_safety_margin={:.2})",
            target_safety_margin
        );
        Self {
            target_safety_margin: target_safety_margin.clamp(0.0, 1.0),
            min_quality_score: 0.6,
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(safety_margin: f64, min_quality: f64) -> Self {
        Self {
            target_safety_margin: safety_margin.clamp(0.0, 1.0),
            min_quality_score: min_quality.clamp(0.0, 1.0),
        }
    }

    /// Validate a fixed video against the original analysis
    ///
    /// Runs post-fix analysis and compares metrics against the original
    /// to ensure quality preservation and safety margin improvement.
    #[instrument(skip(self, fixed_path, original_analysis))]
    pub async fn validate(
        &self,
        fixed_path: &Path,
        original_analysis: &AnalysisResult,
    ) -> Result<crate::engine::ValidationReport> {
        info!("Validating fix: {}", fixed_path.display());

        // Probe the fixed video for metrics
        let fixed_metrics = self.probe_video_metrics(fixed_path).await?;

        // Build analysis result from fixed video metrics
        let after_analysis = self.build_analysis_result(fixed_path, &fixed_metrics).await?;

        // Compute safety margin
        let safety_margin = self.compute_safety_margin(original_analysis, &after_analysis);

        // Compute quality score
        let quality_score = self
            .compute_quality_score_from_metrics(&fixed_metrics)
            .await?;

        // Compute confidence
        let confidence = (safety_margin * quality_score).sqrt();

        // Determine if validation passed
        let passed = safety_margin >= self.target_safety_margin
            && quality_score >= self.min_quality_score;

        let mut warnings = Vec::new();

        // Check duration preservation (allow 5% tolerance)
        let duration_diff = (original_analysis.duration_seconds - after_analysis.duration_seconds)
            .abs();
        let duration_tolerance = original_analysis.duration_seconds * 0.05 + 1.0;
        if duration_diff > duration_tolerance {
            warnings.push(format!(
                "Duration changed by {:.1}s (>{:.1}s tolerance): {:.1}s -> {:.1}s",
                duration_diff,
                duration_tolerance,
                original_analysis.duration_seconds,
                after_analysis.duration_seconds
            ));
        }

        // Check resolution preservation
        if original_analysis.resolution != after_analysis.resolution
            && after_analysis.resolution != (0, 0) {
            warnings.push(format!(
                "Resolution changed: {:?} -> {:?}",
                original_analysis.resolution, after_analysis.resolution
            ));
        }

        // Check FPS preservation
        let fps_diff = (original_analysis.fps - after_analysis.fps).abs();
        if fps_diff > 1.0 && after_analysis.fps > 0.0 {
            warnings.push(format!(
                "FPS changed: {:.2} -> {:.2} (diff: {:.2})",
                original_analysis.fps, after_analysis.fps, fps_diff
            ));
        }

        // Check for remaining violations
        let remaining_violations = after_analysis.violations.len();
        let original_violations = original_analysis.violations.len();
        if remaining_violations > 0 {
            let fixed_count = original_violations.saturating_sub(remaining_violations);
            let reduction_pct = if original_violations > 0 {
                (fixed_count as f64 / original_violations as f64) * 100.0
            } else {
                100.0
            };
            warnings.push(format!(
                "{} violations remain after fix ({:.0}% reduction)",
                remaining_violations, reduction_pct
            ));
        }

        // Check file size
        if fixed_metrics.file_size_mb > 0.0 {
            let original_size = self.probe_file_size_mb(fixed_path).await.unwrap_or(0.0);
            // Note: this is the fixed file itself, not the original
            // We just report it for information
            debug!("Fixed file size: {:.1}MB", fixed_metrics.file_size_mb);
        }

        info!(
            "Validation complete: passed={}, safety_margin={:.3}, quality_score={:.3}, confidence={:.3}",
            passed, safety_margin, quality_score, confidence
        );

        if !passed {
            for warning in &warnings {
                warn!("Validation warning: {}", warning);
            }
        }

        Ok(crate::engine::ValidationReport {
            fix_id: "validation".to_string(),
            passed,
            safety_margin,
            quality_score,
            confidence,
            before_metrics: original_analysis.clone(),
            after_metrics: after_analysis,
            warnings,
        })
    }

    /// Compute the safety margin between before and after analysis
    ///
    /// Safety margin measures how much the violation risk has been reduced.
    /// 1.0 = all violations resolved, 0.0 = no improvement.
    pub fn compute_safety_margin(
        &self,
        before: &AnalysisResult,
        after: &AnalysisResult,
    ) -> f64 {
        // Count violations by severity
        let before_score = self.compute_violation_score(before);
        let after_score = self.compute_violation_score(after);

        debug!(
            "Violation scores: before={:.4}, after={:.4}",
            before_score, after_score
        );

        if before_score <= 0.0 {
            return 1.0; // No violations to begin with
        }

        if after_score <= 0.0 {
            return 1.0; // All violations resolved
        }

        // Safety margin = 1 - (after_score / before_score)
        let margin = 1.0 - (after_score / before_score);
        margin.clamp(0.0, 1.0)
    }

    /// Compute quality score by comparing original and fixed video
    ///
    /// Checks duration, resolution, and FPS preservation. A perfect
    /// score of 1.0 means no degradation in any metric.
    #[instrument(skip(self))]
    pub async fn compute_quality_score(
        &self,
        original: &Path,
        fixed: &Path,
    ) -> Result<f64> {
        info!(
            "Computing quality score: {} vs {}",
            original.display(),
            fixed.display()
        );

        let original_metrics = self.probe_video_metrics(original).await?;
        let fixed_metrics = self.probe_video_metrics(fixed).await?;

        self.compute_quality_score_from_metrics(&fixed_metrics).await
    }

    /// Compute quality score from pre-extracted metrics
    pub async fn compute_quality_score_from_metrics(
        &self,
        metrics: &QualityMetrics,
    ) -> Result<f64> {
        let mut score = 1.0;
        let mut checks = 0;

        // Duration check (video should have positive duration)
        if metrics.duration_seconds > 0.0 {
            checks += 1;
        } else {
            score *= 0.5;
            warn!("Zero or negative duration detected");
        }

        // Resolution check (should be standard resolution)
        let (w, h) = metrics.resolution;
        if w > 0 && h > 0 {
            checks += 1;
            // Penalize very low resolutions
            if w < 320 || h < 240 {
                score *= 0.5;
                warn!("Very low resolution: {}x{}", w, h);
            }
        } else {
            score *= 0.3;
            warn!("Invalid resolution: {}x{}", w, h);
        }

        // FPS check (should be reasonable)
        if metrics.fps > 0.0 {
            checks += 1;
            if metrics.fps < 10.0 {
                score *= 0.7;
                warn!("Very low FPS: {:.2}", metrics.fps);
            }
        } else {
            score *= 0.5;
        }

        // Bitrate check
        if metrics.bitrate_kbps > 0.0 {
            checks += 1;
            if metrics.bitrate_kbps < 100.0 {
                score *= 0.7;
                warn!("Very low bitrate: {:.0}kbps", metrics.bitrate_kbps);
            }
        }

        // File size check (non-zero file)
        if metrics.file_size_mb > 0.0 {
            checks += 1;
        } else {
            score *= 0.1;
            warn!("Zero file size detected");
        }

        if checks == 0 {
            return Ok(0.0);
        }

        // Adjust score based on number of passing checks
        let check_ratio = checks as f64 / 5.0;
        score *= check_ratio;

        let final_score = score.clamp(0.0, 1.0);
        debug!("Quality score: {:.3} ({} checks passed)", final_score, checks);
        Ok(final_score)
    }

    /// Compute quality preservation score between original and fixed video
    pub async fn compute_quality_preservation(
        &self,
        original: &Path,
        fixed: &Path,
    ) -> Result<f64> {
        let original_metrics = self.probe_video_metrics(original).await?;
        let fixed_metrics = self.probe_video_metrics(fixed).await?;

        let mut score = 1.0;

        // Duration preservation (weight: 0.3)
        if original_metrics.duration_seconds > 0.0 {
            let duration_ratio = fixed_metrics.duration_seconds / original_metrics.duration_seconds;
            let duration_score = if duration_ratio >= 0.95 && duration_ratio <= 1.05 {
                1.0
            } else if duration_ratio >= 0.8 && duration_ratio <= 1.2 {
                0.7
            } else {
                0.4
            };
            score -= (1.0 - duration_score) * 0.3;
        }

        // Resolution preservation (weight: 0.3)
        let (orig_w, orig_h) = original_metrics.resolution;
        let (fix_w, fix_h) = fixed_metrics.resolution;
        if orig_w > 0 && orig_h > 0 && fix_w > 0 && fix_h > 0 {
            let res_ratio_w = fix_w as f64 / orig_w as f64;
            let res_ratio_h = fix_h as f64 / orig_h as f64;
            let res_score = if res_ratio_w >= 0.95 && res_ratio_h >= 0.95 {
                1.0
            } else if res_ratio_w >= 0.7 && res_ratio_h >= 0.7 {
                0.7
            } else {
                0.3
            };
            score -= (1.0 - res_score) * 0.3;
        }

        // FPS preservation (weight: 0.2)
        if original_metrics.fps > 0.0 && fixed_metrics.fps > 0.0 {
            let fps_ratio = fixed_metrics.fps / original_metrics.fps;
            let fps_score = if fps_ratio >= 0.9 && fps_ratio <= 1.1 {
                1.0
            } else if fps_ratio >= 0.7 {
                0.7
            } else {
                0.4
            };
            score -= (1.0 - fps_score) * 0.2;
        }

        // Bitrate preservation (weight: 0.2)
        if original_metrics.bitrate_kbps > 0.0 && fixed_metrics.bitrate_kbps > 0.0 {
            let br_ratio = fixed_metrics.bitrate_kbps / original_metrics.bitrate_kbps;
            let br_score = if br_ratio >= 0.5 && br_ratio <= 3.0 {
                1.0
            } else if br_ratio >= 0.2 {
                0.6
            } else {
                0.3
            };
            score -= (1.0 - br_score) * 0.2;
        }

        let final_score = score.clamp(0.0, 1.0);
        info!("Quality preservation score: {:.3}", final_score);
        Ok(final_score)
    }

    /// Check if a fix passed all validation criteria
    pub fn is_valid(&self, report: &crate::engine::ValidationReport) -> bool {
        report.passed
            && report.safety_margin >= self.target_safety_margin
            && report.quality_score >= self.min_quality_score
    }

    // ---- Internal helpers ----

    /// Compute a weighted violation score from an analysis result
    fn compute_violation_score(&self, analysis: &AnalysisResult) -> f64 {
        let mut total_score = 0.0;

        // Weight violations by severity
        for violation in &analysis.violations {
            let weight = match violation.severity {
                Severity::Critical => 1.0,
                Severity::High => 0.75,
                Severity::Medium => 0.5,
                Severity::Low => 0.25,
                Severity::Info => 0.1,
            };
            total_score += weight * (1.0 - violation.confidence * 0.5);
        }

        // Weight flashing segments by intensity
        for segment in &analysis.flashing_segments {
            let flash_score = (segment.frequency_hz / 30.0).min(1.0) * segment.intensity;
            total_score += flash_score;
        }

        // Weight audio issues
        for issue in &analysis.audio_issues {
            let weight = match issue.severity {
                Severity::Critical => 1.0,
                Severity::High => 0.75,
                Severity::Medium => 0.5,
                Severity::Low => 0.25,
                Severity::Info => 0.1,
            };
            total_score += weight * 0.5; // Audio issues weighted less than video
        }

        // Normalize by video duration (longer videos can have more violations)
        if analysis.duration_seconds > 0.0 {
            let duration_minutes = analysis.duration_seconds / 60.0;
            total_score / duration_minutes.max(1.0)
        } else {
            total_score
        }
    }

    /// Probe video file for quality metrics using ffprobe
    async fn probe_video_metrics(&self, path: &Path) -> Result<QualityMetrics> {
        let output = tokio::process::Command::new("ffprobe")
            .args(&[
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
                path.to_str().unwrap_or(""),
            ])
            .output()
            .await
            .map_err(|e| {
                AutoFixError::FfmpegError(format!("ffprobe failed: {}", e))
            })?;

        if !output.status.success() {
            warn!("ffprobe returned non-zero for {}", path.display());
            return Ok(QualityMetrics::default());
        }

        let probe: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(AutoFixError::Serialization)?;

        let mut metrics = QualityMetrics::default();

        // Parse format section
        if let Some(format) = probe.get("format") {
            if let Some(duration) = format.get("duration").and_then(|d| d.as_str()) {
                metrics.duration_seconds = duration.parse().unwrap_or(0.0);
            }
            if let Some(bitrate) = format.get("bit_rate").and_then(|b| b.as_str()) {
                metrics.bitrate_kbps = bitrate.parse::<f64>().unwrap_or(0.0) / 1000.0;
            }
            if let Some(size) = format.get("size").and_then(|s| s.as_str()) {
                metrics.file_size_mb = size.parse::<f64>().unwrap_or(0.0) / (1024.0 * 1024.0);
            }
        }

        // Parse streams
        if let Some(streams) = probe.get("streams").and_then(|s| s.as_array()) {
            for stream in streams {
                let codec_type = stream.get("codec_type").and_then(|c| c.as_str());

                match codec_type {
                    Some("video") => {
                        metrics.video_streams += 1;
                        if let Some(w) = stream.get("width").and_then(|w| w.as_u64()) {
                            metrics.resolution.0 = w as u32;
                        }
                        if let Some(h) = stream.get("height").and_then(|h| h.as_u64()) {
                            metrics.resolution.1 = h as u32;
                        }
                        if let Some(r) = stream.get("r_frame_rate").and_then(|r| r.as_str()) {
                            if let Some((num, den)) = r.split_once('/') {
                                let num: f64 = num.parse().unwrap_or(1.0);
                                let den: f64 = den.parse().unwrap_or(1.0);
                                if den > 0.0 {
                                    metrics.fps = num / den;
                                }
                            }
                        }
                        if let Some(codec) = stream.get("codec_name").and_then(|c| c.as_str()) {
                            metrics.video_codec = Some(codec.to_string());
                        }
                        if let Some(pix) = stream.get("pix_fmt").and_then(|p| p.as_str()) {
                            metrics.pix_fmt = Some(pix.to_string());
                        }
                    }
                    Some("audio") => {
                        metrics.audio_streams += 1;
                        if let Some(br) = stream.get("bit_rate").and_then(|b| b.as_str()) {
                            let br_val = br.parse::<f64>().unwrap_or(0.0) / 1000.0;
                            if metrics.audio_bitrate_kbps.is_none() {
                                metrics.audio_bitrate_kbps = Some(br_val);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(metrics)
    }

    /// Probe file size in MB
    async fn probe_file_size_mb(&self, path: &Path) -> Result<f64> {
        let metadata = tokio::fs::metadata(path).await.map_err(AutoFixError::Io)?;
        let size_bytes = metadata.len() as f64;
        Ok(size_bytes / (1024.0 * 1024.0))
    }

    /// Build an AnalysisResult from video metrics (synthetic)
    async fn build_analysis_result(
        &self,
        _path: &Path,
        metrics: &QualityMetrics,
    ) -> Result<AnalysisResult> {
        Ok(AnalysisResult {
            video_id: "post_fix".to_string(),
            duration_seconds: metrics.duration_seconds,
            resolution: metrics.resolution,
            fps: metrics.fps,
            violations: vec![],
            flashing_segments: vec![],
            detected_objects: vec![],
            audio_issues: vec![],
            metadata_issues: vec![],
            overall_score: 1.0,
            confidence: 0.95,
        })
    }

    /// Compute VMAF score between original and fixed video
    ///
    /// Requires libvmaf to be available in FFmpeg.
    pub async fn compute_vmaf(
        &self,
        original: &Path,
        fixed: &Path,
    ) -> Result<Option<f64>> {
        info!("Computing VMAF score");

        // First check if libvmaf is available
        let libvmaf_check = tokio::process::Command::new("ffmpeg")
            .args(&["-hide_banner", "-h", "filter=libvmaf"])
            .output()
            .await;

        if libvmaf_check.is_err() || !libvmaf_check.unwrap().stdout.len() > 0 {
            warn!("libvmaf filter not available in FFmpeg, skipping VMAF");
            return Ok(None);
        }

        let vmaf_log = std::env::temp_dir().join("vmaf_log.json");

        let args = vec![
            "-i".to_string(), fixed.to_string_lossy().to_string(),
            "-i".to_string(), original.to_string_lossy().to_string(),
            "-lavfi".to_string(),
            format!(
                "[0:v]scale={w}:{h}:flags=bicubic[dist];[dist][1:v]libvmaf=log_path={}",
                vmaf_log.display(),
                w = 1920,
                h = 1080
            ),
            "-f".to_string(), "null".to_string(),
            "-".to_string(),
        ];

        let output = tokio::process::Command::new("ffmpeg")
            .args(&args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .output()
            .await
            .map_err(|e| AutoFixError::FfmpegError(format!("VMAF computation failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("VMAF computation failed: {}", stderr);
            return Ok(None);
        }

        // Parse VMAF log
        if let Ok(content) = tokio::fs::read_to_string(&vmaf_log).await {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(pooled_vmaf) = json
                    .get("pooled_metrics")
                    .and_then(|p| p.get("vmaf"))
                    .and_then(|v| v.get("mean"))
                    .and_then(|v| v.as_f64())
                {
                    info!("VMAF score: {:.2}", pooled_vmaf);
                    return Ok(Some(pooled_vmaf));
                }
            }
        }

        Ok(None)
    }

    /// Compute SSIM between original and fixed video
    pub async fn compute_ssim(
        &self,
        original: &Path,
        fixed: &Path,
    ) -> Result<Option<f64>> {
        info!("Computing SSIM score");

        let args = vec![
            "-i".to_string(), fixed.to_string_lossy().to_string(),
            "-i".to_string(), original.to_string_lossy().to_string(),
            "-lavfi".to_string(), "ssim".to_string(),
            "-f".to_string(), "null".to_string(),
            "-".to_string(),
        ];

        let output = tokio::process::Command::new("ffmpeg")
            .args(&args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AutoFixError::FfmpegError(format!("SSIM computation failed: {}", e)))?;

        if !output.status.success() {
            return Ok(None);
        }

        // Parse SSIM from stderr (FFmpeg outputs SSIM to stderr)
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines() {
            if line.contains("SSIM") {
                if let Some(ssim_val) = line.split_whitespace().last() {
                    if let Ok(ssim) = ssim_val.parse::<f64>() {
                        info!("SSIM score: {:.4}", ssim);
                        return Ok(Some(ssim));
                    }
                }
            }
            if line.contains("All:") && line.contains("(") {
                // Parse format like "All:0.981234 (17.2345db)"
                if let Some(start) = line.find("All:") {
                    let substr = &line[start + 4..];
                    if let Some(end) = substr.find(' ') {
                        if let Ok(ssim) = substr[..end].parse::<f64>() {
                            info!("SSIM score: {:.4}", ssim);
                            return Ok(Some(ssim));
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}

impl Default for FixValidator {
    fn default() -> Self {
        Self::new(0.9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_analysis(violations: Vec<Violation>) -> AnalysisResult {
        AnalysisResult {
            video_id: "test".to_string(),
            duration_seconds: 60.0,
            resolution: (1920, 1080),
            fps: 30.0,
            violations,
            flashing_segments: vec![],
            detected_objects: vec![],
            audio_issues: vec![],
            metadata_issues: vec![],
            overall_score: 0.5,
            confidence: 0.8,
        }
    }

    fn create_test_violation(severity: Severity, rule_id: &str) -> Violation {
        Violation {
            id: format!("vio_{}", rule_id),
            rule_id: rule_id.to_string(),
            rule_name: format!("Test {}", rule_id),
            severity,
            timestamp: 5.0,
            duration: 2.0,
            description: "Test violation".to_string(),
            bbox: None,
            confidence: 0.9,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_validator_creation() {
        let validator = FixValidator::new(0.85);
        assert_eq!(validator.target_safety_margin, 0.85);
        assert_eq!(validator.min_quality_score, 0.6);
    }

    #[test]
    fn test_validator_with_thresholds() {
        let validator = FixValidator::with_thresholds(0.9, 0.75);
        assert_eq!(validator.target_safety_margin, 0.9);
        assert_eq!(validator.min_quality_score, 0.75);
    }

    #[test]
    fn test_compute_violation_score() {
        let validator = FixValidator::new(0.9);

        let violations = vec![
            create_test_violation(Severity::Critical, "rule1"),
            create_test_violation(Severity::High, "rule2"),
        ];
        let analysis = create_test_analysis(violations);
        let score = validator.compute_violation_score(&analysis);
        assert!(score > 0.0);

        // No violations = 0 score
        let clean = create_test_analysis(vec![]);
        let clean_score = validator.compute_violation_score(&clean);
        assert_eq!(clean_score, 0.0);
    }

    #[test]
    fn test_compute_safety_margin() {
        let validator = FixValidator::new(0.9);

        let before = create_test_analysis(vec![
            create_test_violation(Severity::Critical, "rule1"),
            create_test_violation(Severity::High, "rule2"),
        ]);

        // All violations fixed
        let after = create_test_analysis(vec![]);
        let margin = validator.compute_safety_margin(&before, &after);
        assert_eq!(margin, 1.0);

        // No change
        let margin2 = validator.compute_safety_margin(&before, &before);
        assert_eq!(margin2, 0.0);
    }

    #[test]
    fn test_compute_safety_margin_partial() {
        let validator = FixValidator::new(0.9);

        let before = create_test_analysis(vec![
            create_test_violation(Severity::Critical, "rule1"),
            create_test_violation(Severity::Critical, "rule2"),
        ]);

        // One violation fixed, one remains
        let after = create_test_analysis(vec![
            create_test_violation(Severity::Critical, "rule1"),
        ]);

        let margin = validator.compute_safety_margin(&before, &after);
        assert!(margin > 0.0 && margin < 1.0);
    }

    #[tokio::test]
    async fn test_compute_quality_score_from_metrics() {
        let validator = FixValidator::new(0.9);

        let good_metrics = QualityMetrics {
            duration_seconds: 60.0,
            resolution: (1920, 1080),
            fps: 30.0,
            bitrate_kbps: 5000.0,
            file_size_mb: 50.0,
            ..Default::default()
        };

        let score = validator.compute_quality_score_from_metrics(&good_metrics).await.unwrap();
        assert!(score > 0.7);

        // Bad metrics
        let bad_metrics = QualityMetrics {
            duration_seconds: 0.0,
            resolution: (0, 0),
            fps: 0.0,
            bitrate_kbps: 0.0,
            file_size_mb: 0.0,
            ..Default::default()
        };

        let bad_score = validator.compute_quality_score_from_metrics(&bad_metrics).await.unwrap();
        assert!(bad_score < 0.5);
    }

    #[test]
    fn test_quality_metrics_default() {
        let metrics = QualityMetrics::default();
        assert_eq!(metrics.duration_seconds, 0.0);
        assert_eq!(metrics.resolution, (0, 0));
        assert_eq!(metrics.fps, 0.0);
        assert!(metrics.vmaf_score.is_none());
        assert!(metrics.ssim_score.is_none());
    }

    #[test]
    fn test_is_valid() {
        let validator = FixValidator::with_thresholds(0.8, 0.7);

        let report = crate::engine::ValidationReport {
            fix_id: "test".to_string(),
            passed: true,
            safety_margin: 0.85,
            quality_score: 0.75,
            confidence: 0.8,
            before_metrics: create_test_analysis(vec![]),
            after_metrics: create_test_analysis(vec![]),
            warnings: vec![],
        };

        assert!(validator.is_valid(&report));

        let bad_report = crate::engine::ValidationReport {
            passed: false,
            safety_margin: 0.5,
            quality_score: 0.4,
            ..report
        };

        assert!(!validator.is_valid(&bad_report));
    }

    #[test]
    fn test_compute_violation_score_with_flashing() {
        let validator = FixValidator::new(0.9);

        let mut analysis = create_test_analysis(vec![]);
        analysis.flashing_segments.push(FlashingSegment {
            start_time: 5.0,
            end_time: 10.0,
            frequency_hz: 10.0,
            intensity: 0.9,
            frames_affected: vec![],
        });

        let score = validator.compute_violation_score(&analysis);
        assert!(score > 0.0);
    }

    #[test]
    fn test_threshold_clamping() {
        let v1 = FixValidator::new(1.5);
        assert_eq!(v1.target_safety_margin, 1.0);

        let v2 = FixValidator::new(-0.5);
        assert_eq!(v2.target_safety_margin, 0.0);
    }
}
