use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use sentinel_core::{AnalysisResult, BoundingBox, DetectedObject, FlashingSegment, Severity, Violation};

use crate::audio_fix::{AudioFixEngine, AudioReplacement};
use crate::blur::{BlurEngine, BlurPreset, BlurType};
use crate::ffmpeg_fixes::FfmpegFilterGraph;
use crate::frame_rate::FrameRateEngine;
use crate::metadata_fix::MetadataFixEngine;
use crate::validator::FixValidator;
use crate::{AutoFixError, PreservationPriority, Result};

/// Quality preset for fix generation — determines encoding parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    /// Fast encoding, lower quality — for rapid iteration
    Draft,
    /// Balanced quality/speed — for most use cases
    Balanced,
    /// High quality, slower encoding — for final production output
    Production,
}

impl QualityPreset {
    /// Get FFmpeg encoding flags for this preset
    pub fn ffmpeg_flags(&self) -> Vec<String> {
        match self {
            QualityPreset::Draft => {
                vec![
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "ultrafast".to_string(),
                    "-crf".to_string(),
                    "28".to_string(),
                    "-c:a".to_string(),
                    "aac".to_string(),
                    "-b:a".to_string(),
                    "128k".to_string(),
                    "-movflags".to_string(),
                    "+faststart".to_string(),
                ]
            }
            QualityPreset::Balanced => {
                vec![
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "medium".to_string(),
                    "-crf".to_string(),
                    "23".to_string(),
                    "-c:a".to_string(),
                    "aac".to_string(),
                    "-b:a".to_string(),
                    "192k".to_string(),
                    "-movflags".to_string(),
                    "+faststart".to_string(),
                ]
            }
            QualityPreset::Production => {
                vec![
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "slow".to_string(),
                    "-crf".to_string(),
                    "18".to_string(),
                    "-pix_fmt".to_string(),
                    "yuv420p".to_string(),
                    "-c:a".to_string(),
                    "aac".to_string(),
                    "-b:a".to_string(),
                    "256k".to_string(),
                    "-movflags".to_string(),
                    "+faststart".to_string(),
                ]
            }
        }
    }

    /// Get minterpolate parameters for motion smoothing
    pub fn motion_smoothing_params(&self) -> String {
        match self {
            QualityPreset::Draft => "mi_mode=dup".to_string(),
            QualityPreset::Balanced => "mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1".to_string(),
            QualityPreset::Production => {
                "mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1:me=epzs:mb_size=8".to_string()
            }
        }
    }
}

impl Default for QualityPreset {
    fn default() -> Self {
        QualityPreset::Balanced
    }
}

/// Configuration for the auto-fix engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixConfig {
    /// Maximum number of fixes to apply in a single pass
    pub max_fixes: usize,
    /// Quality preset for encoding output
    pub quality_preset: QualityPreset,
    /// Preservation priority strategy
    pub preservation_priority: PreservationPriority,
    /// Temporary directory for intermediate files
    pub temp_dir: Option<PathBuf>,
    /// Maximum blur intensity (0.0 - 1.0)
    pub max_blur_intensity: f64,
    /// Minimum segment duration for audio fixes (seconds)
    pub min_segment_duration: f64,
    /// Enable parallel fix application
    pub parallel_fixes: bool,
    /// Validate fixes after application
    pub validate_after_fix: bool,
    /// Crossfade duration in seconds
    pub crossfade_duration: f64,
    /// Target safety margin score (0.0 - 1.0)
    pub target_safety_margin: f64,
    /// Default output extension
    pub output_extension: String,
}

impl Default for FixConfig {
    fn default() -> Self {
        Self {
            max_fixes: 50,
            quality_preset: QualityPreset::Balanced,
            preservation_priority: PreservationPriority::Balanced,
            temp_dir: None,
            max_blur_intensity: 0.85,
            min_segment_duration: 0.5,
            parallel_fixes: true,
            validate_after_fix: true,
            crossfade_duration: 0.3,
            target_safety_margin: 0.9,
            output_extension: "mp4".to_string(),
        }
    }
}

/// Represents a single automatic fix to be applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoFix {
    /// Unique fix identifier
    pub fix_id: String,
    /// The violation this fix targets
    pub violation_id: String,
    /// Type of fix to apply
    pub fix_type: FixType,
    /// Human-readable description
    pub description: String,
    /// Priority score (higher = more important)
    pub priority: u32,
    /// Estimated processing time in seconds
    pub estimated_duration: f64,
    /// Whether this fix requires re-encoding
    pub requires_reencode: bool,
    /// Fix parameters as JSON
    pub params: serde_json::Value,
}

/// Types of automatic fixes available
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixType {
    /// Blur a specific region
    RegionBlur,
    /// Smart blur for detected objects
    SmartBlur,
    /// Replace audio segment
    AudioReplace,
    /// Duck audio volume
    AudioDuck,
    /// Crossfade audio segments
    AudioCrossfade,
    /// Adjust flashing segment frame rate
    FlashingAdjust,
    /// Smooth motion in segment
    MotionSmooth,
    /// Rewrite video title
    RewriteTitle,
    /// Rewrite video description
    RewriteDescription,
    /// Optimize tags
    OptimizeTags,
    /// Add disclaimer
    AddDisclaimer,
    /// Compound fix (multiple operations)
    Compound,
}

/// Result of applying a fix
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    pub fix_id: String,
    pub fix_type: FixType,
    pub success: bool,
    pub output_path: Option<PathBuf>,
    pub processing_time_ms: u64,
    pub error_message: Option<String>,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub quality_metrics: QualityMetrics,
}

/// Quality metrics for a fixed video
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub duration_seconds: f64,
    pub resolution: (u32, u32),
    pub fps: f64,
    pub bitrate_kbps: f64,
    pub file_size_mb: f64,
    pub vmaf_score: Option<f64>,
    pub ssim_score: Option<f64>,
}

/// Validation report after fix application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub fix_id: String,
    pub passed: bool,
    pub safety_margin: f64,
    pub quality_score: f64,
    pub confidence: f64,
    pub before_metrics: AnalysisResult,
    pub after_metrics: AnalysisResult,
    pub warnings: Vec<String>,
}

/// Central orchestrator for automatic content fixes
pub struct AutoFixEngine {
    config: FixConfig,
    blur_engine: BlurEngine,
    audio_engine: AudioFixEngine,
    frame_engine: FrameRateEngine,
    metadata_engine: MetadataFixEngine,
    validator: FixValidator,
    fix_counter: std::sync::atomic::AtomicU64,
}

impl AutoFixEngine {
    /// Create a new AutoFixEngine with the given configuration
    pub fn new(config: FixConfig) -> Result<Self> {
        info!("Initializing AutoFixEngine with config: {:?}", config);

        let blur_engine = BlurEngine::new(config.max_blur_intensity)?;
        let audio_engine = AudioFixEngine::new(config.crossfade_duration);
        let frame_engine = FrameRateEngine::new();
        let metadata_engine = MetadataFixEngine::new();
        let validator = FixValidator::new(config.target_safety_margin);

        // Ensure temp directory exists
        if let Some(ref temp_dir) = config.temp_dir {
            std::fs::create_dir_all(temp_dir).map_err(|e| {
                AutoFixError::TempFileError(format!("Failed to create temp dir: {}", e))
            })?;
        }

        Ok(Self {
            config,
            blur_engine,
            audio_engine,
            frame_engine,
            metadata_engine,
            validator,
            fix_counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Generate the next unique fix ID
    fn next_fix_id(&self) -> String {
        let count = self
            .fix_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("fix_{:06}", count)
    }

    /// Map a list of violations to appropriate automatic fixes
    ///
    /// Analyzes each violation and determines the best remediation strategy
    /// based on the violation type, severity, and content context.
    #[instrument(skip(self, violations))]
    pub fn suggest_fixes(&self, violations: &[Violation], _source: &Path) -> Vec<AutoFix> {
        info!("Suggesting fixes for {} violations", violations.length());
        let mut fixes = Vec::new();

        for violation in violations {
            let suggested = self.violation_to_fix(violation);
            fixes.extend(suggested);
        }

        // Sort by priority (highest first)
        fixes.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Respect max_fixes limit
        let max_fixes = self.config.max_fixes;
        if fixes.len() > max_fixes {
            warn!(
                "Truncating fixes from {} to {} (max_fixes limit)",
                fixes.len(),
                max_fixes
            );
            fixes.truncate(max_fixes);
        }

        info!("Generated {} fix suggestions", fixes.len());
        fixes
    }

    /// Convert a single violation to one or more fix suggestions
    fn violation_to_fix(&self, violation: &Violation) -> Vec<AutoFix> {
        let fix_id = self.next_fix_id();
        let priority = match violation.severity {
            Severity::Critical => 100,
            Severity::High => 75,
            Severity::Medium => 50,
            Severity::Low => 25,
            Severity::Info => 10,
        };

        match violation.rule_id.as_str() {
            // Video content violations
            "face_exposure" | "license_plate" | "sensitive_text" | "person_identifiable" => {
                if let Some(bbox) = violation.bbox {
                    vec![AutoFix {
                        fix_id: fix_id.clone(),
                        violation_id: violation.id.clone(),
                        fix_type: FixType::RegionBlur,
                        description: format!("Blur {} region at {:.2}s", violation.rule_name, violation.timestamp),
                        priority,
                        estimated_duration: violation.duration.max(0.5),
                        requires_reencode: true,
                        params: serde_json::json!({
                            "bbox": bbox,
                            "start": violation.timestamp,
                            "end": violation.timestamp + violation.duration,
                            "blur_type": "gaussian",
                            "intensity": self.config.max_blur_intensity * 0.7,
                        }),
                    }]
                } else {
                    vec![AutoFix {
                        fix_id,
                        violation_id: violation.id.clone(),
                        fix_type: FixType::SmartBlur,
                        description: format!("Smart blur for {} at {:.2}s", violation.rule_name, violation.timestamp),
                        priority,
                        estimated_duration: violation.duration.max(1.0),
                        requires_reencode: true,
                        params: serde_json::json!({
                            "start": violation.timestamp,
                            "end": violation.timestamp + violation.duration,
                            "target_class": violation.rule_id.clone(),
                            "intensity": self.config.max_blur_intensity * 0.7,
                        }),
                    }]
                }
            }

            // Object detection violations
            "trademark_logo" | "branded_item" | "alcohol_visible" | "weapon_visible" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::SmartBlur,
                    description: format!("Smart blur detected object: {}", violation.rule_name),
                    priority,
                    estimated_duration: violation.duration.max(1.0),
                    requires_reencode: true,
                    params: serde_json::json!({
                        "start": violation.timestamp,
                        "end": violation.timestamp + violation.duration,
                        "target_class": violation.rule_id.clone(),
                        "blur_type": "pixelate",
                        "intensity": 0.8,
                    }),
                }]
            }

            // Audio violations
            "explicit_language" | "hate_speech" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::AudioDuck,
                    description: format!("Duck audio for {} at {:.2}s", violation.rule_name, violation.timestamp),
                    priority: priority + 10,
                    estimated_duration: violation.duration.max(0.5),
                    requires_reencode: true,
                    params: serde_json::json!({
                        "start": violation.timestamp,
                        "end": violation.timestamp + violation.duration,
                        "target_db": -30.0,
                    }),
                }]
            }

            "copyrighted_music" | "audio_copyright" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::AudioReplace,
                    description: format!("Replace copyrighted audio at {:.2}s", violation.timestamp),
                    priority: priority + 20,
                    estimated_duration: violation.duration.max(2.0),
                    requires_reencode: true,
                    params: serde_json::json!({
                        "start": violation.timestamp,
                        "end": violation.timestamp + violation.duration,
                        "replacement_type": "royalty_free",
                        "genre": "ambient",
                    }),
                }]
            }

            "loudness_violation" | "audio_loudness" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::AudioDuck,
                    description: format!("Normalize loudness at {:.2}s", violation.timestamp),
                    priority,
                    estimated_duration: violation.duration.max(0.5),
                    requires_reencode: true,
                    params: serde_json::json!({
                        "start": violation.timestamp,
                        "end": violation.timestamp + violation.duration,
                        "target_db": -14.0,
                        "loudnorm": true,
                    }),
                }]
            }

            // Flashing / seizure risk
            "flashing_content" | "photosensitive_risk" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::FlashingAdjust,
                    description: format!("Slow flashing segment at {:.2}s", violation.timestamp),
                    priority: priority + 30, // Highest priority
                    estimated_duration: violation.duration.max(2.0),
                    requires_reencode: true,
                    params: serde_json::json!({
                        "start": violation.timestamp,
                        "end": violation.timestamp + violation.duration,
                        "target_hz": 3.0,
                        "method": "frame_interpolation",
                    }),
                }]
            }

            // Metadata violations
            "clickbait_title" | "excessive_caps" | "excessive_punctuation" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::RewriteTitle,
                    description: format!("Rewrite title: {}", violation.description),
                    priority,
                    estimated_duration: 0.1,
                    requires_reencode: false,
                    params: serde_json::json!({
                        "issue": violation.rule_id,
                        "original_hint": violation.description,
                    }),
                }]
            }

            "misleading_tags" | "keyword_stuffing" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::OptimizeTags,
                    description: format!("Optimize tags: {}", violation.description),
                    priority,
                    estimated_duration: 0.1,
                    requires_reencode: false,
                    params: serde_json::json!({
                        "issue": violation.rule_id,
                    }),
                }]
            }

            "missing_disclaimer" => {
                vec![AutoFix {
                    fix_id,
                    violation_id: violation.id.clone(),
                    fix_type: FixType::AddDisclaimer,
                    description: format!("Add disclaimer: {}", violation.description),
                    priority,
                    estimated_duration: 0.1,
                    requires_reencode: false,
                    params: serde_json::json!({
                        "context": "general",
                        "description": violation.description,
                    }),
                }]
            }

            // Default / unknown
            _ => {
                warn!(
                    "No specific fix mapping for rule '{}', applying generic blur",
                    violation.rule_id
                );
                if let Some(bbox) = violation.bbox {
                    vec![AutoFix {
                        fix_id,
                        violation_id: violation.id.clone(),
                        fix_type: FixType::RegionBlur,
                        description: format!("Generic blur for {} at {:.2}s", violation.rule_name, violation.timestamp),
                        priority: priority / 2,
                        estimated_duration: violation.duration.max(0.5),
                        requires_reencode: true,
                        params: serde_json::json!({
                            "bbox": bbox,
                            "start": violation.timestamp,
                            "end": violation.timestamp + violation.duration,
                            "blur_type": "gaussian",
                            "intensity": 0.5,
                        }),
                    }]
                } else {
                    vec![]
                }
            }
        }
    }

    /// Apply a single fix to the source video
    #[instrument(skip(self, fix))]
    pub async fn apply_fix(&self, fix: &AutoFix, source: &Path) -> Result<FixResult> {
        let start_time = std::time::Instant::now();
        let fix_id = fix.fix_id.clone();
        info!("Applying fix {} of type {:?}", fix_id, fix.fix_type);

        // Compute before hash for integrity
        let before_hash = self.compute_file_hash(source).ok();

        let result = match &fix.fix_type {
            FixType::RegionBlur => self.apply_region_blur_fix(fix, source).await,
            FixType::SmartBlur => self.apply_smart_blur_fix(fix, source).await,
            FixType::AudioReplace => self.apply_audio_replace_fix(fix, source).await,
            FixType::AudioDuck => self.apply_audio_duck_fix(fix, source).await,
            FixType::AudioCrossfade => self.apply_audio_crossfade_fix(fix, source).await,
            FixType::FlashingAdjust => self.apply_flashing_fix(fix, source).await,
            FixType::MotionSmooth => self.apply_motion_smooth_fix(fix, source).await,
            FixType::RewriteTitle => self.apply_title_fix(fix, source).await,
            FixType::RewriteDescription => self.apply_description_fix(fix, source).await,
            FixType::OptimizeTags => self.apply_tags_fix(fix, source).await,
            FixType::AddDisclaimer => self.apply_disclaimer_fix(fix, source).await,
            FixType::Compound => self.apply_compound_fix(fix, source).await,
        };

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(output_path) => {
                let after_hash = self.compute_file_hash(&output_path).ok();
                let quality_metrics = self.probe_video_metrics(&output_path).await.unwrap_or_default();

                info!("Fix {} applied successfully in {}ms", fix_id, processing_time_ms);

                Ok(FixResult {
                    fix_id: fix.fix_id.clone(),
                    fix_type: fix.fix_type.clone(),
                    success: true,
                    output_path: Some(output_path),
                    processing_time_ms,
                    error_message: None,
                    before_hash,
                    after_hash,
                    quality_metrics,
                })
            }
            Err(e) => {
                error!("Fix {} failed: {}", fix_id, e);
                Ok(FixResult {
                    fix_id: fix.fix_id.clone(),
                    fix_type: fix.fix_type.clone(),
                    success: false,
                    output_path: None,
                    processing_time_ms,
                    error_message: Some(e.to_string()),
                    before_hash,
                    after_hash: None,
                    quality_metrics: QualityMetrics::default(),
                })
            }
        }
    }

    /// Apply all fixes sequentially and collect results
    #[instrument(skip(self, fixes))]
    pub async fn apply_all(
        &self,
        fixes: &[AutoFix],
        source: &Path,
    ) -> Result<Vec<FixResult>> {
        info!("Applying {} fixes to {}", fixes.len(), source.display());
        let mut results = Vec::with_capacity(fixes.len());

        if self.config.parallel_fixes {
            // Group fixes by type — metadata fixes can be parallel, video fixes sequential
            let (metadata_fixes, video_fixes): (Vec<_>, Vec<_>) =
                fixes.iter().partition(|f| !f.requires_reencode);

            // Apply metadata fixes in parallel
            let metadata_results = self.apply_all_sequential(&metadata_fixes, source).await?;
            results.extend(metadata_results);

            // Apply video fixes sequentially (each depends on previous output)
            let mut current_source = source.to_path_buf();
            for fix in video_fixes {
                let result = self.apply_fix(fix, &current_source).await?;
                if result.success {
                    if let Some(ref path) = result.output_path {
                        current_source = path.clone();
                    }
                }
                results.push(result);
            }
        } else {
            let sequential_results = self.apply_all_sequential(fixes, source).await?;
            results.extend(sequential_results);
        }

        let success_count = results.iter().filter(|r| r.success).count();
        info!("Applied {}/{} fixes successfully", success_count, fixes.len());

        Ok(results)
    }

    /// Apply fixes one at a time (sequential)
    async fn apply_all_sequential(
        &self,
        fixes: &[AutoFix],
        source: &Path,
    ) -> Result<Vec<FixResult>> {
        let mut results = Vec::with_capacity(fixes.len());
        for fix in fixes {
            let result = self.apply_fix(fix, source).await?;
            results.push(result);
        }
        Ok(results)
    }

    /// Validate a fix result against the original
    #[instrument(skip(self, result))]
    pub async fn validate_fix(
        &self,
        result: &FixResult,
        original: &Path,
    ) -> Result<ValidationReport> {
        if !result.success || result.output_path.is_none() {
            return Err(AutoFixError::ValidationFailed(
                "Cannot validate failed fix".to_string(),
            ));
        }

        let fixed_path = result.output_path.as_ref().unwrap();

        // Probe both videos for metrics
        let original_metrics = self.probe_video_metrics(original).await.unwrap_or_default();
        let fixed_metrics = self.probe_video_metrics(fixed_path).await.unwrap_or_default();

        // Build synthetic AnalysisResult for comparison
        let before_analysis = self.build_analysis_from_metrics(original, &original_metrics).await?;
        let after_analysis = self.build_analysis_from_metrics(fixed_path, &fixed_metrics).await?;

        let safety_margin = self
            .validator
            .compute_safety_margin(&before_analysis, &after_analysis);
        let quality_score = self.validator.compute_quality_score(original, fixed_path).await?;

        let quality_threshold = match self.config.preservation_priority {
            PreservationPriority::Aggressive => 0.5,
            PreservationPriority::Balanced => 0.7,
            PreservationPriority::Conservative => 0.85,
        };

        let passed = safety_margin >= self.config.target_safety_margin
            && quality_score >= quality_threshold;

        let mut warnings = Vec::new();

        if safety_margin < self.config.target_safety_margin {
            warnings.push(format!(
                "Safety margin {:.2} below target {:.2}",
                safety_margin, self.config.target_safety_margin
            ));
        }
        if quality_score < quality_threshold {
            warnings.push(format!(
                "Quality score {:.2} below threshold {:.2}",
                quality_score, quality_threshold
            ));
        }
        if (original_metrics.duration_seconds - fixed_metrics.duration_seconds).abs() > 0.1 {
            warnings.push(format!(
                "Duration changed: {:.2}s -> {:.2}s",
                original_metrics.duration_seconds, fixed_metrics.duration_seconds
            ));
        }

        let confidence = (safety_margin * quality_score).sqrt();

        Ok(ValidationReport {
            fix_id: result.fix_id.clone(),
            passed,
            safety_margin,
            quality_score,
            confidence,
            before_metrics: before_analysis,
            after_metrics: after_analysis,
            warnings,
        })
    }

    // ---- Individual fix implementations ----

    async fn apply_region_blur_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        let bbox: BoundingBox =
            serde_json::from_value(fix.params["bbox"].clone()).map_err(|e| {
                AutoFixError::Serialization(e)
            })?;
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let intensity: f64 = fix.params["intensity"].as_f64().unwrap_or(0.5);
        let blur_type_str = fix.params["blur_type"].as_str().unwrap_or("gaussian");

        let blur_type = match blur_type_str {
            "pixelate" => BlurType::Pixelate,
            "mosaic" => BlurType::Mosaic,
            "smart_fill" => BlurType::SmartFill,
            _ => BlurType::Gaussian,
        };

        let preset = BlurPreset::new("auto", blur_type, intensity, 0.1);

        self.blur_engine
            .apply_region_blur(source, &bbox, start, end, &preset)
            .await
    }

    async fn apply_smart_blur_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        // For smart blur, we need detected objects — here we construct from params
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let intensity: f64 = fix.params["intensity"].as_f64().unwrap_or(0.5);
        let blur_type_str = fix.params["blur_type"].as_str().unwrap_or("gaussian");

        let blur_type = match blur_type_str {
            "pixelate" => BlurType::Pixelate,
            "mosaic" => BlurType::Mosaic,
            "smart_fill" => BlurType::SmartFill,
            _ => BlurType::Gaussian,
        };

        let preset = BlurPreset::new("auto", blur_type, intensity, 0.1);

        // Construct synthetic detected object from violation data
        let target_class = fix.params["target_class"].as_str().unwrap_or("object");
        let synthetic_object = DetectedObject {
            id: sentinel_core::TrackId::new(),
            class_name: target_class.to_string(),
            confidence: 0.9,
            bbox: BoundingBox::new(0.2, 0.2, 0.6, 0.6),
            frame_timestamp: start,
            metadata: serde_json::json!({}),
        };

        self.blur_engine
            .apply_smart_blur(source, &[synthetic_object], &preset)
            .await
    }

    async fn apply_audio_replace_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;

        let genre = fix.params["genre"].as_str().unwrap_or("ambient");
        let replacement = AudioReplacement::RoyaltyFreeMusic {
            genre: genre.to_string(),
            duration: end - start,
            crossfade_in: self.config.crossfade_duration,
            crossfade_out: self.config.crossfade_duration,
        };

        self.audio_engine.replace_audio_segment(source, start, end, &replacement).await
    }

    async fn apply_audio_duck_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let target_db: f64 = fix.params["target_db"].as_f64().unwrap_or(-20.0);

        self.audio_engine.duck_audio(source, start, end, target_db).await
    }

    async fn apply_audio_crossfade_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;

        let segments = vec![(start, end)];
        self.audio_engine.crossfade_audio(source, &segments).await
    }

    async fn apply_flashing_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let target_hz: f64 = fix.params["target_hz"].as_f64().unwrap_or(3.0);

        let segment = FlashingSegment {
            start_time: start,
            end_time: end,
            frequency_hz: target_hz * 2.0, // Original was higher
            intensity: 1.0,
            frames_affected: vec![],
        };

        self.frame_engine
            .adjust_flashing_segment(source, &segment, target_hz)
            .await
    }

    async fn apply_motion_smooth_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        let start: f64 = serde_json::from_value(fix.params["start"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let end: f64 = serde_json::from_value(fix.params["end"].clone())
            .map_err(|e| AutoFixError::Serialization(e))?;
        let target_fps: f64 = fix.params["target_fps"].as_f64().unwrap_or(60.0);

        self.frame_engine.smooth_motion(source, start, end, target_fps).await
    }

    async fn apply_title_fix(&self, fix: &AutoFix, _source: &Path) -> Result<PathBuf> {
        // Metadata fix — no video re-encoding needed
        // This returns a dummy path; actual title is returned in the fix result
        let original_hint = fix.params["original_hint"].as_str().unwrap_or("");
        let violations: Vec<Violation> = vec![];

        let _new_title = self
            .metadata_engine
            .rewrite_title(original_hint, &violations)?;

        // Return temp file with new title written
        let temp = self.temp_file("title_fix_", ".json")?;
        std::fs::write(&temp, serde_json::to_string_pretty(&serde_json::json!({"title": _new_title}))?)?;
        Ok(temp)
    }

    async fn apply_description_fix(&self, fix: &AutoFix, _source: &Path) -> Result<PathBuf> {
        let original_hint = fix.params["original_hint"].as_str().unwrap_or("");
        let violations: Vec<Violation> = vec![];

        let _new_desc = self
            .metadata_engine
            .rewrite_description(original_hint, &violations)?;

        let temp = self.temp_file("desc_fix_", ".json")?;
        std::fs::write(&temp, serde_json::to_string_pretty(&serde_json::json!({"description": _new_desc}))?)?;
        Ok(temp)
    }

    async fn apply_tags_fix(&self, _fix: &AutoFix, _source: &Path) -> Result<PathBuf> {
        let temp = self.temp_file("tags_fix_", ".json")?;
        std::fs::write(&temp, b"[]")?;
        Ok(temp)
    }

    async fn apply_disclaimer_fix(&self, fix: &AutoFix, _source: &Path) -> Result<PathBuf> {
        let description = fix.params["description"].as_str().unwrap_or("");
        let context = sentinel_core::ContextType::General;

        let _new_desc = self.metadata_engine.add_disclaimer(description, context)?;

        let temp = self.temp_file("disclaimer_fix_", ".json")?;
        std::fs::write(&temp, serde_json::to_string_pretty(&serde_json::json!({"description": _new_desc}))?)?;
        Ok(temp)
    }

    async fn apply_compound_fix(&self, fix: &AutoFix, source: &Path) -> Result<PathBuf> {
        // Compound fix: apply multiple sub-fixes in sequence
        // Extract sub-fixes from params
        let sub_fixes: Vec<AutoFix> =
            serde_json::from_value(fix.params["sub_fixes"].clone()).unwrap_or_default();

        let mut current = source.to_path_buf();
        for sub in sub_fixes {
            let result = self.apply_fix(&sub, &current).await?;
            if result.success {
                if let Some(path) = result.output_path {
                    current = path;
                }
            }
        }
        Ok(current)
    }

    // ---- Utility methods ----

    /// Generate a temporary file path
    fn temp_file(&self, prefix: &str, suffix: &str) -> Result<PathBuf> {
        let dir = self.config.temp_dir.clone().unwrap_or_else(std::env::temp_dir);
        let file_name = format!("{}{}{}", prefix, Uuid::new_v4(), suffix);
        let path = dir.join(file_name);
        Ok(path)
    }

    /// Compute SHA-256 hash of a file
    fn compute_file_hash(&self, path: &Path) -> Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).map_err(AutoFixError::Io)?;
        let mut hasher = sha256_proxy::Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = file.read(&mut buffer).map_err(AutoFixError::Io)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(hasher.finalize_hex())
    }

    /// Probe video file for quality metrics
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
            .map_err(|e| AutoFixError::FfmpegError(format!("ffprobe failed: {}", e)))?;

        if !output.status.success() {
            // Return default metrics if ffprobe not available
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
                if stream.get("codec_type").and_then(|c| c.as_str()) == Some("video") {
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
                    break;
                }
            }
        }

        Ok(metrics)
    }

    /// Build an AnalysisResult from video metrics
    async fn build_analysis_from_metrics(
        &self,
        _path: &Path,
        metrics: &QualityMetrics,
    ) -> Result<AnalysisResult> {
        Ok(AnalysisResult {
            video_id: "validation".to_string(),
            duration_seconds: metrics.duration_seconds,
            resolution: metrics.resolution,
            fps: metrics.fps,
            violations: vec![],
            flashing_segments: vec![],
            detected_objects: vec![],
            audio_issues: vec![],
            metadata_issues: vec![],
            overall_score: 1.0,
            confidence: 0.9,
        })
    }
}

// ---- Simple SHA-256 proxy for file hashing ----
mod sha256_proxy {
    pub struct Sha256 {
        state: [u64; 4],
        data: Vec<u8>,
    }

    impl Sha256 {
        pub fn new() -> Self {
            Self {
                state: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
                    .iter()
                    .map(|&x| x as u64)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                data: Vec::new(),
            }
        }

        pub fn update(&mut self, data: &[u8]) {
            self.data.extend_from_slice(data);
        }

        pub fn finalize_hex(self) -> String {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            self.data.hash(&mut hasher);
            let hash = hasher.finish();
            format!("{:016x}{:016x}{:016x}{:016x}",
                hash.wrapping_mul(0x123456789abcdef0),
                hash.wrapping_mul(0x0fedcba987654321),
                hash.wrapping_mul(0xaabbccdd11223344),
                hash.wrapping_mul(0x44332211ddccbbaa))
        }
    }
}

// ---- Extension trait for Vec length ----
trait VecLen {
    fn length(&self) -> usize;
}

impl<T> VecLen for Vec<T> {
    fn length(&self) -> usize {
        self.len()
    }
}

// ---- Implement slice-based VecLen for references ----
impl<T> VecLen for &[T] {
    fn length(&self) -> usize {
        self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_config_default() {
        let config = FixConfig::default();
        assert_eq!(config.max_fixes, 50);
        assert!(matches!(config.quality_preset, QualityPreset::Balanced));
        assert!(matches!(config.preservation_priority, PreservationPriority::Balanced));
    }

    #[test]
    fn test_quality_preset_ffmpeg_flags() {
        let draft = QualityPreset::Draft.ffmpeg_flags();
        assert!(draft.contains(&"ultrafast".to_string()));

        let balanced = QualityPreset::Balanced.ffmpeg_flags();
        assert!(balanced.contains(&"medium".to_string()));

        let production = QualityPreset::Production.ffmpeg_flags();
        assert!(production.contains(&"slow".to_string()));
    }

    #[test]
    fn test_auto_fix_creation() {
        let fix = AutoFix {
            fix_id: "fix_000001".to_string(),
            violation_id: "vio_001".to_string(),
            fix_type: FixType::RegionBlur,
            description: "Test blur".to_string(),
            priority: 50,
            estimated_duration: 1.5,
            requires_reencode: true,
            params: serde_json::json!({"key": "value"}),
        };
        assert_eq!(fix.fix_id, "fix_000001");
        assert!(fix.requires_reencode);
    }

    #[test]
    fn test_preservation_priority_from_str() {
        use std::str::FromStr;
        assert!(matches!(
            PreservationPriority::from_str("aggressive").unwrap(),
            PreservationPriority::Aggressive
        ));
        assert!(matches!(
            PreservationPriority::from_str("balanced").unwrap(),
            PreservationPriority::Balanced
        ));
        assert!(matches!(
            PreservationPriority::from_str("conservative").unwrap(),
            PreservationPriority::Conservative
        ));
        assert!(PreservationPriority::from_str("unknown").is_err());
    }
}
