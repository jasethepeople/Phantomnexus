//! # analysis-engine
//!
//! Multi-modal AI analysis engine for the YouTube Sentinel platform.
//!
//! Orchestrates visual, audio, and metadata analysis of video content
//! using rule-based heuristics and placeholder ONNX model integration.
//!
//! ## Module Overview
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `engine` | Central `AnalysisEngine` orchestrator — coordinates all stages |
//! | `visual` | `VisualAnalyzer` — object detection, OCR, scene classification, flashing detection |
//! | `audio` | `AudioAnalyzer` — speech transcription, event detection, copyright checking |
//! | `metadata` | `MetadataAnalyzer` — clickbait detection, profanity filtering, keyword stuffing |
//! | `model_manager` | ONNX model manager with caching and GPU/CPU selection |
//! | `zig_ffi` | FFI bindings to Zig audio fingerprinting library |
//! | `inference_utils` | Shared utilities — NMS, softmax, letterboxing |
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use analysis_engine::engine::{AnalysisEngine, EngineConfig};
//! use sentinel_core::types::{AnalysisJob, PipelineConfig};
//! use sentinel_core::proto::VideoSource;
//!
//! async fn analyze_video() {
//!     let config = EngineConfig::new()
//!         .with_model_dir("/opt/sentinel/models")
//!         .with_gpu(true, 0)
//!         .with_audio(true)
//!         .with_metadata(true);
//!
//!     let engine = AnalysisEngine::new(config).unwrap();
//!
//!     let job = AnalysisJob::new(
//!         VideoSource::new("vid-1", "/tmp/video.mp4"),
//!         PipelineConfig::new("/opt/sentinel/models"),
//!     );
//!
//!     let result = engine.analyze(&job).await.unwrap();
//!     println!("final score: {}", result.final_score);
//! }
//! ```

// ===========================================================================
// Module declarations
// ===========================================================================

pub mod audio;
pub mod engine;
pub mod inference_utils;
pub mod metadata;
pub mod model_manager;
pub mod visual;
pub mod zig_ffi;

// ===========================================================================
// Convenience re-exports
// ===========================================================================

// Engine
pub use engine::{AnalysisEngine, EngineConfig};

// Visual analysis
pub use visual::{SceneClassification, VisualAnalyzer, VisualAnalyzerConfig};

// Audio analysis
pub use audio::{AudioAnalyzer, AudioAnalyzerConfig, AudioBuffer};

// Metadata analysis
pub use metadata::{
    DescriptionAnalysis, MetadataAnalyzer, MetadataAnalyzerConfig, ProfanityFilter,
    ProfanityMatch, ProfanitySeverity, TagsAnalysis, TitleAnalysis,
};

// Model management
pub use model_manager::{ComputeTarget, ModelInfo, ModelManager, ModelSession, SessionId};

// Zig FFI
pub use zig_ffi::{
    compare_fingerprints, compare_fingerprint_against_db, compute_fingerprint,
    compute_spectrogram, create_fingerprint_db, fingerprint_from_file, load_fingerprint_db,
    save_fingerprint_db, FingerprintComparison, FingerprintDbHandle, FingerprintError,
    FingerprintHandle, SpectrogramHandle,
};

// Inference utilities
pub use inference_utils::{
    class_aware_nms, clamp_bbox, compute_letterbox, non_max_suppression,
    reverse_letterbox_coords, sigmoid, sigmoid_in_place, softmax_1d, softmax_2d,
    top_k, DetectionCandidate, LetterboxParams,
};
