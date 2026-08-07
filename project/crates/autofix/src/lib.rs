//! # autofix - Content-Aware Precision Remediation Engine
//!
//! The `autofix` crate provides intelligent, content-aware fixes for YouTube policy
//! violations detected by the Sentinel analysis pipeline. It maps violation types to
//! appropriate remediation strategies, generates precise FFmpeg commands, and validates
//! that fixes preserve quality while resolving compliance issues.
//!
//! ## Architecture
//!
//! - `AutoFixEngine`: Central orchestrator that maps violations to fixes
//! - `BlurEngine`: Region-based and smart blur using FFmpeg filter graphs
//! - `AudioFixEngine`: Audio replacement, ducking, and crossfade
//! - `MetadataFixEngine`: Title/description/tag rewriting with policy-aware NLP
//! - `FrameRateEngine`: Flashing segment slowdown and motion smoothing
//! - `FixValidator`: Post-fix quality and safety validation
//! - `ffmpeg_fixes`: Low-level FFmpeg command/filter generation utilities

use std::path::PathBuf;
use thiserror::Error;

pub mod audio_fix;
pub mod blur;
pub mod engine;
pub mod ffmpeg_fixes;
pub mod frame_rate;
pub mod metadata_fix;
pub mod validator;

pub use audio_fix::{AudioFixEngine, AudioReplacement};
pub use blur::{BlurEngine, BlurPreset, BlurType};
pub use engine::{AutoFix, AutoFixEngine, FixConfig, FixResult, QualityPreset, ValidationReport};
pub use ffmpeg_fixes::FfmpegFilterGraph;
pub use frame_rate::FrameRateEngine;
pub use metadata_fix::MetadataFixEngine;
pub use validator::{FixValidator, QualityMetrics};

/// Re-export core types for convenience
pub use sentinel_core::{
    AnalysisResult, AudioIssue, AudioIssueType, BoundingBox, ContextType, DetectedObject,
    FlashingSegment, MetadataIssue, MetadataIssueType, Severity, Violation,
};

/// Errors that can occur during auto-fix operations
#[derive(Error, Debug)]
pub enum AutoFixError {
    #[error("FFmpeg execution failed: {0}")]
    FfmpegError(String),

    #[error("FFmpeg not found in PATH")]
    FfmpegNotFound,

    #[error("Invalid video file: {0}")]
    InvalidVideo(String),

    #[error("Invalid time range: start={start}, end={end}")]
    InvalidTimeRange { start: f64, end: f64 },

    #[error("Region out of bounds: {0:?}")]
    RegionOutOfBounds(BoundingBox),

    #[error("Blur preset not found: {0}")]
    BlurPresetNotFound(String),

    #[error("Audio replacement failed: {0}")]
    AudioReplacementFailed(String),

    #[error("Metadata processing error: {0}")]
    MetadataError(String),

    #[error("Frame rate adjustment failed: {0}")]
    FrameRateError(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Image processing error: {0}")]
    ImageError(String),

    #[error("Max fixes exceeded: requested {requested}, max {max}")]
    MaxFixesExceeded { requested: usize, max: usize },

    #[error("Quality preservation check failed: {0}")]
    QualityPreservationFailed(String),

    #[error("No applicable fix for violation: {0}")]
    NoApplicableFix(String),

    #[error("Temp file error: {0}")]
    TempFileError(String),

    #[error("Crossfade generation failed: {0}")]
    CrossfadeError(String),

    #[error("Smart blur failed: {0}")]
    SmartBlurFailed(String),
}

/// Result type alias for autofix operations
pub type Result<T> = std::result::Result<T, AutoFixError>;

/// Presets for preservation priority during fixes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreservationPriority {
    /// Prioritize fixing all violations, accept quality loss
    Aggressive,
    /// Balance between fix completeness and quality preservation
    Balanced,
    /// Prioritize quality preservation, only fix critical issues
    Conservative,
}

impl Default for PreservationPriority {
    fn default() -> Self {
        PreservationPriority::Balanced
    }
}

impl std::str::FromStr for PreservationPriority {
    type Err = AutoFixError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "aggressive" => Ok(PreservationPriority::Aggressive),
            "balanced" => Ok(PreservationPriority::Balanced),
            "conservative" => Ok(PreservationPriority::Conservative),
            _ => Err(AutoFixError::MetadataError(format!(
                "Unknown preservation priority: {}",
                s
            ))),
        }
    }
}
