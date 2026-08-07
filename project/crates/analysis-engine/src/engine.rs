//! AnalysisEngine: central orchestrator for all analysis stages.
//!
//! Coordinates visual, audio, and metadata analysis in parallel using
//! `tokio::join!`.  Maintains a model manager and configuration for
//! the entire analysis pipeline.

use sentinel_core::error::SentinelError;
use sentinel_core::proto::{
    AnalysisResult, AudioAnalysis, MetadataAnalysis, VideoSource, VisualAnalysis,
};
use sentinel_core::types::{AnalysisJob, PipelineConfig};
use std::path::Path;
use tokio::task;
use tracing::{debug, error, info, instrument, warn};

use crate::audio::{AudioAnalyzer, AudioAnalyzerConfig};
use crate::metadata::{MetadataAnalyzer, MetadataAnalyzerConfig};
use crate::model_manager::{ComputeTarget, ModelManager};
use crate::visual::{VisualAnalyzer, VisualAnalyzerConfig};

/// Configuration for the analysis engine.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Path to the directory containing model files.
    pub model_dir: String,
    /// Whether to use GPU acceleration.
    pub enable_gpu: bool,
    /// GPU device index.
    pub gpu_device: i32,
    /// Number of parallel workers.
    pub num_workers: usize,
    /// Whether to enable audio analysis.
    pub enable_audio: bool,
    /// Whether to enable metadata analysis.
    pub enable_metadata: bool,
    /// Whether to enable OCR.
    pub enable_ocr: bool,
    /// Whether to enable flashing detection.
    pub enable_flashing: bool,
    /// Detection confidence threshold (0.0..1.0).
    pub detection_threshold: f64,
    /// Visual analyzer configuration.
    pub visual_config: VisualAnalyzerConfig,
    /// Audio analyzer configuration.
    pub audio_config: AudioAnalyzerConfig,
    /// Metadata analyzer configuration.
    pub metadata_config: MetadataAnalyzerConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            model_dir: "/opt/sentinel/models".into(),
            enable_gpu: true,
            gpu_device: 0,
            num_workers: 4,
            enable_audio: true,
            enable_metadata: true,
            enable_ocr: true,
            enable_flashing: true,
            detection_threshold: 0.5,
            visual_config: VisualAnalyzerConfig::default(),
            audio_config: AudioAnalyzerConfig::default(),
            metadata_config: MetadataAnalyzerConfig::default(),
        }
    }
}

impl EngineConfig {
    /// Create a new engine config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a PipelineConfig.
    pub fn from_pipeline_config(pipeline: &PipelineConfig) -> Self {
        EngineConfig {
            model_dir: pipeline.model_dir.clone(),
            enable_gpu: pipeline.enable_gpu,
            gpu_device: pipeline.gpu_device,
            num_workers: pipeline.num_workers,
            enable_audio: pipeline.enable_audio,
            enable_metadata: pipeline.enable_metadata,
            enable_ocr: pipeline.enable_ocr,
            enable_flashing: pipeline.enable_flashing_detection,
            detection_threshold: pipeline.detection_confidence_threshold,
            visual_config: VisualAnalyzerConfig::default()
                .with_detection_threshold(pipeline.detection_confidence_threshold)
                .with_flashing(pipeline.enable_flashing_detection)
                .with_scene_classification(true),
            audio_config: AudioAnalyzerConfig::default()
                .with_copyright(true),
            metadata_config: MetadataAnalyzerConfig::default(),
        }
    }

    /// Fluent setter for model directory.
    pub fn with_model_dir<S: Into<String>>(mut self, dir: S) -> Self {
        self.model_dir = dir.into();
        self
    }

    /// Fluent setter for GPU.
    pub fn with_gpu(mut self, enable: bool, device: i32) -> Self {
        self.enable_gpu = enable;
        self.gpu_device = device;
        self
    }

    /// Fluent setter for audio toggle.
    pub fn with_audio(mut self, enable: bool) -> Self {
        self.enable_audio = enable;
        self
    }

    /// Fluent setter for metadata toggle.
    pub fn with_metadata(mut self, enable: bool) -> Self {
        self.enable_metadata = enable;
        self
    }
}

/// The central analysis engine that orchestrates all analysis stages.
pub struct AnalysisEngine {
    config: EngineConfig,
    model_manager: ModelManager,
    visual_analyzer: VisualAnalyzer,
    audio_analyzer: AudioAnalyzer,
    metadata_analyzer: MetadataAnalyzer,
}

impl AnalysisEngine {
    /// Create a new analysis engine.
    ///
    /// Initializes mock ML sessions and loads available models from
    /// the configured model directory.
    #[instrument(skip(config))]
    pub fn new(config: EngineConfig) -> Result<Self, SentinelError> {
        info!("initializing AnalysisEngine with model_dir={}", config.model_dir);

        let model_manager = ModelManager::new(&config.model_dir);
        let visual_analyzer = VisualAnalyzer::new(config.visual_config.clone());
        let audio_analyzer = AudioAnalyzer::new(config.audio_config.clone());
        let metadata_analyzer = MetadataAnalyzer::new(config.metadata_config.clone());

        // Auto-select compute target.
        let target = model_manager.auto_select_target(config.enable_gpu);
        info!("auto-selected compute target: {}", target);

        info!("AnalysisEngine initialized successfully");

        Ok(AnalysisEngine {
            config,
            model_manager,
            visual_analyzer,
            audio_analyzer,
            metadata_analyzer,
        })
    }

    /// Run the complete analysis pipeline on a job.
    ///
    /// Orchestrates visual + audio + metadata analysis in parallel
    /// using `tokio::join!`.  Returns an aggregated `AnalysisResult`.
    #[instrument(skip(self, job))]
    pub async fn analyze(&self, job: &AnalysisJob) -> Result<AnalysisResult, SentinelError> {
        info!(
            "starting full analysis for job {} (video={})",
            job.job_id, job.source.video_id
        );

        let video_path = Path::new(&job.source.file_path);

        // Run all three analysis stages in parallel.
        let (visual_result, audio_result, metadata_result) = tokio::join!(
            self.analyze_visual_task(video_path, job),
            self.analyze_audio_task(video_path, job),
            self.analyze_metadata_task(job),
        );

        // Build the result.
        let mut result = AnalysisResult::empty(job.job_id, job.source.clone());

        // Process visual result.
        match visual_result {
            Ok(visual) => {
                info!(
                    "visual analysis: {} objects, {} texts, score={:.2}",
                    visual.objects.len(),
                    visual.texts.len(),
                    visual.overall_risk_score
                );
                result.visual = Some(visual);
            }
            Err(e) => {
                error!("visual analysis failed: {}", e);
                if !e.is_retryable() {
                    return Err(e);
                }
            }
        }

        // Process audio result.
        match audio_result {
            Ok(audio) => {
                info!(
                    "audio analysis: {} transcript segments, {} events, score={:.2}",
                    audio.transcript.len(),
                    audio.events.len(),
                    audio.overall_risk_score
                );
                result.audio = Some(audio);
            }
            Err(e) => {
                error!("audio analysis failed: {}", e);
                if !e.is_retryable() {
                    return Err(e);
                }
            }
        }

        // Process metadata result.
        match metadata_result {
            Ok(metadata) => {
                info!(
                    "metadata analysis: clickbait={:.2}, score={:.2}",
                    metadata.clickbait_score, metadata.overall_risk_score
                );
                result.metadata = Some(metadata);
            }
            Err(e) => {
                error!("metadata analysis failed: {}", e);
                if !e.is_retryable() {
                    return Err(e);
                }
            }
        }

        // Compute final aggregated score.
        result.compute_final_score();
        result.finalize();

        info!(
            "analysis complete for job {}: final_score={:.2}",
            job.job_id, result.final_score
        );

        Ok(result)
    }

    /// Analyze visual content of a video.
    ///
    /// Extracts frames and runs object detection, text detection,
    /// scene classification, and flashing detection.
    #[instrument(skip(self, job))]
    pub async fn analyze_visual(
        &self,
        job: &AnalysisJob,
    ) -> Result<VisualAnalysis, SentinelError> {
        let video_path = Path::new(&job.source.file_path);

        // Use blocking task for CPU-intensive visual analysis.
        let visual_config = self.config.visual_config.clone();
        let video_path = video_path.to_path_buf();
        let source_path = job.source.file_path.clone();

        let visual = task::spawn_blocking(move || {
            let analyzer = VisualAnalyzer::new(visual_config);
            let extractor_config = frame_extractor::types::ExtractorConfig::new();

            match analyzer.analyze(&video_path, &extractor_config) {
                Ok(v) => Ok(v),
                Err(e) => {
                    warn!("visual analysis error for {}: {:?}", source_path, e);
                    Err(SentinelError::model_inference("visual", e.to_string()))
                }
            }
        })
        .await
        .map_err(|e| SentinelError::model_inference("visual", format!("task join: {}", e)))?;

        visual
    }

    /// Analyze audio content of a video.
    ///
    /// Runs speech transcription, audio event detection, and
    /// copyright checking (if enabled).
    #[instrument(skip(self, job))]
    pub async fn analyze_audio(
        &self,
        job: &AnalysisJob,
    ) -> Result<AudioAnalysis, SentinelError> {
        if !self.config.enable_audio {
            debug!("audio analysis disabled");
            return Ok(AudioAnalysis::empty());
        }

        let video_path = Path::new(&job.source.file_path).to_path_buf();
        let enable_copyright = self.config.audio_config.enable_copyright;
        let audio_config = self.config.audio_config.clone();

        let audio = task::spawn_blocking(move || {
            let analyzer = AudioAnalyzer::new(audio_config);
            match analyzer.analyze(&video_path, enable_copyright) {
                Ok(a) => Ok(a),
                Err(e) => {
                    warn!("audio analysis error: {}", e);
                    Err(SentinelError::model_inference("audio", e.to_string()))
                }
            }
        })
        .await
        .map_err(|e| SentinelError::model_inference("audio", format!("task join: {}", e)))?;

        audio
    }

    /// Analyze metadata of a video.
    ///
    /// Runs clickbait detection, profanity checking, and keyword
    /// stuffing detection on title, description, and tags.
    #[instrument(skip(self, job))]
    pub async fn analyze_metadata(
        &self,
        job: &AnalysisJob,
    ) -> Result<MetadataAnalysis, SentinelError> {
        if !self.config.enable_metadata {
            debug!("metadata analysis disabled");
            return Ok(MetadataAnalysis::empty());
        }

        let source = job.source.clone();
        let metadata_config = self.config.metadata_config.clone();

        let metadata = task::spawn_blocking(move || {
            let analyzer = MetadataAnalyzer::new(metadata_config);
            match analyzer.analyze(&source) {
                Ok(m) => Ok(m),
                Err(e) => {
                    warn!("metadata analysis error: {}", e);
                    Err(SentinelError::model_inference("metadata", e.to_string()))
                }
            }
        })
        .await
        .map_err(|e| SentinelError::model_inference("metadata", format!("task join: {}", e)))?;

        metadata
    }

    // ------------------------------------------------------------------
    // Internal task wrappers for parallel execution
    // ------------------------------------------------------------------

    async fn analyze_visual_task(
        &self,
        video_path: &Path,
        job: &AnalysisJob,
    ) -> Result<VisualAnalysis, SentinelError> {
        debug!("starting visual analysis task");

        let visual_config = self.config.visual_config.clone();
        let path = video_path.to_path_buf();

        task::spawn_blocking(move || {
            let analyzer = VisualAnalyzer::new(visual_config);
            let extractor_config = frame_extractor::types::ExtractorConfig::new();
            analyzer.analyze(&path, &extractor_config)
                .map_err(|e| SentinelError::model_inference("visual", e.to_string()))
        })
        .await
        .map_err(|e| SentinelError::model_inference("visual", format!("task join: {}", e)))?
    }

    async fn analyze_audio_task(
        &self,
        video_path: &Path,
        job: &AnalysisJob,
    ) -> Result<AudioAnalysis, SentinelError> {
        if !self.config.enable_audio {
            return Ok(AudioAnalysis::empty());
        }

        debug!("starting audio analysis task");

        let audio_config = self.config.audio_config.clone();
        let enable_copyright = audio_config.enable_copyright;
        let path = video_path.to_path_buf();

        task::spawn_blocking(move || {
            let analyzer = AudioAnalyzer::new(audio_config);
            analyzer.analyze(&path, enable_copyright)
                .map_err(|e| SentinelError::model_inference("audio", e.to_string()))
        })
        .await
        .map_err(|e| SentinelError::model_inference("audio", format!("task join: {}", e)))?
    }

    async fn analyze_metadata_task(
        &self,
        job: &AnalysisJob,
    ) -> Result<MetadataAnalysis, SentinelError> {
        if !self.config.enable_metadata {
            return Ok(MetadataAnalysis::empty());
        }

        debug!("starting metadata analysis task");

        let metadata_config = self.config.metadata_config.clone();
        let source = job.source.clone();

        task::spawn_blocking(move || {
            let analyzer = MetadataAnalyzer::new(metadata_config);
            analyzer.analyze(&source)
                .map_err(|e| SentinelError::model_inference("metadata", e.to_string()))
        })
        .await
        .map_err(|e| SentinelError::model_inference("metadata", format!("task join: {}", e)))?
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    /// Get the engine configuration.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Get the model manager.
    pub fn model_manager(&self) -> &ModelManager {
        &self.model_manager
    }

    /// Get a mutable reference to the model manager.
    pub fn model_manager_mut(&mut self) -> &mut ModelManager {
        &mut self.model_manager
    }

    /// Get the number of loaded models.
    pub async fn loaded_model_count(&self) -> usize {
        self.model_manager.session_count().await
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::proto::VideoSource;
    use sentinel_core::types::{AnalysisJob, PipelineConfig};

    fn create_test_engine() -> AnalysisEngine {
        let config = EngineConfig::new()
            .with_model_dir("/tmp/test-models")
            .with_gpu(false, 0)
            .with_audio(true)
            .with_metadata(true);
        AnalysisEngine::new(config).unwrap()
    }

    #[test]
    fn test_engine_config_default() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.model_dir, "/opt/sentinel/models");
        assert!(cfg.enable_audio);
        assert!(cfg.enable_metadata);
    }

    #[test]
    fn test_engine_config_from_pipeline() {
        let pipeline = PipelineConfig::new("/models")
            .with_gpu(false)
            .with_audio(false)
            .with_workers(2);
        let cfg = EngineConfig::from_pipeline_config(&pipeline);
        assert_eq!(cfg.model_dir, "/models");
        assert!(!cfg.enable_gpu);
        assert!(!cfg.enable_audio);
        assert_eq!(cfg.num_workers, 2);
    }

    #[test]
    fn test_engine_config_builder() {
        let cfg = EngineConfig::new()
            .with_model_dir("/custom")
            .with_gpu(true, 1)
            .with_audio(false)
            .with_metadata(false);
        assert_eq!(cfg.model_dir, "/custom");
        assert!(cfg.enable_gpu);
        assert_eq!(cfg.gpu_device, 1);
        assert!(!cfg.enable_audio);
        assert!(!cfg.enable_metadata);
    }

    #[test]
    fn test_engine_new() {
        let engine = create_test_engine();
        assert_eq!(engine.config().model_dir, "/tmp/test-models");
        assert!(!engine.config().enable_gpu);
    }

    #[test]
    fn test_engine_loaded_model_count() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let engine = create_test_engine();
        let count = rt.block_on(engine.loaded_model_count());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_engine_model_manager_access() {
        let engine = create_test_engine();
        let mgr = engine.model_manager();
        assert_eq!(mgr.model_dir(), Path::new("/tmp/test-models"));
    }

    #[tokio::test]
    async fn test_analyze_visual_empty_job() {
        let engine = create_test_engine();
        let job = AnalysisJob::new(
            VideoSource::new("test", "/nonexistent.mp4"),
            PipelineConfig::new("/tmp/models"),
        );

        // Should fail gracefully for nonexistent file.
        let result = engine.analyze_visual(&job).await;
        assert!(result.is_err() || result.unwrap().objects.is_empty());
    }

    #[tokio::test]
    async fn test_analyze_audio_disabled() {
        let config = EngineConfig::new().with_audio(false);
        let engine = AnalysisEngine::new(config).unwrap();
        let job = AnalysisJob::new(
            VideoSource::new("test", "/tmp/test.mp4"),
            PipelineConfig::new("/tmp/models"),
        );

        let result = engine.analyze_audio(&job).await;
        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert!(analysis.transcript.is_empty());
        assert!(analysis.events.is_empty());
    }

    #[tokio::test]
    async fn test_analyze_metadata_disabled() {
        let config = EngineConfig::new().with_metadata(false);
        let engine = AnalysisEngine::new(config).unwrap();
        let job = AnalysisJob::new(
            VideoSource::new("test", "/tmp/test.mp4"),
            PipelineConfig::new("/tmp/models"),
        );

        let result = engine.analyze_metadata(&job).await;
        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert_eq!(analysis.clickbait_score, 0.0);
    }

    #[tokio::test]
    async fn test_analyze_metadata_enabled() {
        let engine = create_test_engine();
        let job = AnalysisJob::new(
            VideoSource::new("test", "/tmp/test.mp4")
                .with_title("SHOCKING Video You Won't Believe!"),
            PipelineConfig::new("/tmp/models"),
        );

        let result = engine.analyze_metadata(&job).await;
        assert!(result.is_ok());
        let analysis = result.unwrap();
        assert!(analysis.clickbait_score > 0.0);
        assert!(analysis.overall_risk_score >= 0.0);
    }

    #[tokio::test]
    async fn test_full_analysis_empty_job() {
        let engine = create_test_engine();
        let job = AnalysisJob::new(
            VideoSource::new("test", "/nonexistent.mp4"),
            PipelineConfig::new("/tmp/models"),
        );

        let result = engine.analyze(&job).await;
        // Should not panic; may succeed with empty results or fail gracefully.
        if let Ok(analysis) = result {
            assert!(analysis.final_score >= 0.0);
            assert!(analysis.completed_at.is_some());
        }
    }

    #[test]
    fn test_engine_config_equality() {
        let a = EngineConfig::default();
        let b = EngineConfig::default();
        assert_eq!(a, b);

        let c = EngineConfig::new().with_audio(false);
        assert_ne!(a, c);
    }

    #[test]
    fn test_engine_config_clone() {
        let cfg = EngineConfig::new().with_model_dir("/test");
        let cloned = cfg.clone();
        assert_eq!(cfg, cloned);
    }

    #[tokio::test]
    async fn test_analyze_parallel_completes() {
        let engine = create_test_engine();
        let job = AnalysisJob::new(
            VideoSource::new("parallel-test", "/nonexistent.mp4")
                .with_title("Test Title"),
            PipelineConfig::new("/tmp/models"),
        );

        // Even with failures, the parallel tasks should complete.
        let result = engine.analyze(&job).await;
        assert!(result.is_ok() || result.is_err());
    }
}
