//! # grpc-server
//!
//! gRPC server crate for YouTube Sentinel.  Exposes a tonic-based API that
//! orchestrates the `sentinel-core`, `frame-extractor`, `analysis-engine`,
//! `policy-engine`, `synthesis` and `autofix` crates through a unified
//! 10-stage analysis pipeline.
//!
//! ## Architecture
//!
//! ```text
//! Client (gRPC)
//!    │
//!    ├─► AnalysisService      ──► 10-stage pipeline
//!    │                              1. Extract frames
//!    ├─► FrameExtractionService     2. Visual analysis
//!    │                              3. Audio analysis
//!    └─► AudioAnalysisService       4. Metadata analysis
//!                                   5. Synthesis
//!                                   6. Apply policies
//!                                   7. Auto-fix
//!                                   8. Validate fixes
//!                                   9. Finalize
//! ```

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

pub mod analysis_service;
pub mod audio_service;
pub mod frame_service;
pub mod pipeline;
pub mod proto;
pub mod server;

// ---------------------------------------------------------------------------
// Re-exports (public API)
// ---------------------------------------------------------------------------

// Proto types (protobuf-equivalent Rust types).
pub use proto::*;

// Server state and configuration.
pub use server::{SentinelServer, ServerConfig, JobStore, JobRecord};

// Service implementations.
pub use analysis_service::{AnalysisService, SentinelAnalysis};
pub use audio_service::{AudioAnalysisService, AudioAnalysisServiceTrait};
pub use frame_service::{FrameExtractionService, FrameExtraction};

// Pipeline.
pub use pipeline::{AnalysisPipeline, AnalysisResult};
