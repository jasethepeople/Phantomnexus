//! gRPC frame extraction service.
//!
//! Provides a streaming endpoint that returns batches of extracted frames
//! from a video source.

use crate::proto::{ExtractRequest, FrameBatch, FrameInfo, Timestamp};
use crate::server::SentinelServer;
use anyhow::Result;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, instrument, warn};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait FrameExtraction: Send + Sync + 'static {
    type ExtractFramesStream: Stream<Item = Result<FrameBatch, Status>> + Send + 'static;

    async fn extract_frames(
        &self,
        request: Request<ExtractRequest>,
    ) -> Result<Response<Pin<Box<Self::ExtractFramesStream>>>, Status>;
}

// ---------------------------------------------------------------------------
// FrameExtractionService
// ---------------------------------------------------------------------------

/// Service that extracts and streams video frames in batches.
#[derive(Debug, Clone)]
pub struct FrameExtractionService {
    server: SentinelServer,
}

impl FrameExtractionService {
    pub fn new(server: SentinelServer) -> Self {
        Self { server }
    }

    /// Maximum number of frames in a single batch message.
    const BATCH_SIZE: usize = 32;
}

impl FrameExtraction for FrameExtractionService {
    type ExtractFramesStream = Pin<Box<dyn Stream<Item = Result<FrameBatch, Status>> + Send>>;

    #[instrument(skip(self), fields(source_id = %request.get_ref().source.source_id))]
    async fn extract_frames(
        &self,
        request: Request<ExtractRequest>,
    ) -> Result<Response<Self::ExtractFramesStream>, Status> {
        let req = request.into_inner();
        info!(
            source_id = %req.source.source_id,
            fps = req.fps,
            max_resolution = req.max_resolution,
            extract_audio = req.extract_audio,
            "Received extract_frames request"
        );

        // Validate request.
        if req.source.url.is_empty() {
            return Err(Status::invalid_argument("Video URL is empty"));
        }
        if req.fps <= 0.0 {
            return Err(Status::invalid_argument("FPS must be positive"));
        }
        if req.max_resolution == 0 {
            return Err(Status::invalid_argument("max_resolution must be > 0"));
        }

        let server = self.server.clone();
        let (tx, rx) = mpsc::channel::<Result<FrameBatch, Status>>(16);

        // Spawn the extraction work onto a blocking thread.
        tokio::spawn(async move {
            if let Err(e) = Self::stream_frames(&server, &req, tx).await {
                error!(error = %e, "Frame extraction failed");
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::ExtractFramesStream))
    }
}

impl FrameExtractionService {
    /// Core extraction loop – extracts frames and sends them as batches.
    async fn stream_frames(
        server: &SentinelServer,
        req: &ExtractRequest,
        tx: mpsc::Sender<Result<FrameBatch, Status>>,
    ) -> Result<()> {
        let meta = req.source.to_metadata();
        let cfg = frame_extractor::ExtractConfig {
            fps: req.fps,
            max_resolution: req.max_resolution,
            extract_audio: req.extract_audio,
            work_dir: server.config.frame_work_dir.clone(),
        };

        let extractor = Arc::clone(&server.frame_extractor);

        // Blocking extraction.
        let frames = tokio::task::spawn_blocking(move || {
            extractor
                .extract(&meta, &cfg)
                .map_err(|e| anyhow::anyhow!("Frame extraction failed: {}", e))
        })
        .await
        .context("frame extraction panicked")??;

        info!(frame_count = frames.len(), "Frames extracted, streaming batches");

        // Chunk frames into batches and stream.
        let mut batch_index: u32 = 0;
        for chunk in frames.chunks(Self::BATCH_SIZE) {
            let mut frame_infos = Vec::with_capacity(chunk.len());

            for frame in chunk {
                // Encode frame bytes to base64 for protobuf transport.
                use base64::Engine as _;
                let data_base64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);

                frame_infos.push(FrameInfo {
                    frame_index: frame.index,
                    timestamp: Timestamp::from_seconds(frame.timestamp_seconds),
                    data_base64,
                    width: frame.width,
                    height: frame.height,
                });
            }

            let batch = FrameBatch {
                batch_index,
                frames: frame_infos,
            };

            if tx.send(Ok(batch)).await.is_err() {
                warn!("Client disconnected; aborting frame stream");
                return Ok(());
            }

            batch_index += 1;
        }

        info!(batch_count = batch_index, "All frame batches streamed");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tower Service implementation
// ---------------------------------------------------------------------------

use std::future::Future;
use std::task::{Context as TaskContext, Poll};
use tower::Service;

impl Service<http::Request<tonic::body::BoxBody>> for FrameExtractionService {
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
        let svc = self.clone();
        Box::pin(async move {
            let path = req.uri().path();
            let mut parts = req.into_parts();
            let body_bytes = match hyper::body::to_bytes(parts.1).await {
                Ok(b) => b,
                Err(_) => {
                    return Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "2")
                        .header("grpc-message", "failed to read body")
                        .body(tonic::body::BoxBody::default())
                        .unwrap());
                }
            };

            let response = match path {
                "/sentinel.v1.FrameExtraction/ExtractFrames" => {
                    match serde_json::from_slice::<ExtractRequest>(&body_bytes) {
                        Ok(request) => {
                            match svc.extract_frames(Request::new(request)).await {
                                Ok(_) => {
                                    http::Response::builder()
                                        .status(200)
                                        .header("grpc-status", "0")
                                        .header("content-type", "application/grpc")
                                        .body(tonic::body::BoxBody::default())
                                        .unwrap()
                                }
                                Err(status) => {
                                    http::Response::builder()
                                        .status(200)
                                        .header("grpc-status", format!("{}", status.code() as i32))
                                        .header("grpc-message", status.message())
                                        .body(tonic::body::BoxBody::default())
                                        .unwrap()
                                }
                            }
                        }
                        Err(e) => {
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "3")
                                .header("grpc-message", format!("JSON decode: {}", e))
                                .body(tonic::body::BoxBody::default())
                                .unwrap()
                        }
                    }
                }
                _ => {
                    http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("grpc-message", format!("method {} not found", path))
                        .body(tonic::body::BoxBody::default())
                        .unwrap()
                }
            };

            Ok(response)
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::VideoSource;
    use std::collections::HashMap;

    fn dummy_server() -> SentinelServer {
        SentinelServer {
            config: crate::server::ServerConfig::default(),
            job_store: crate::server::JobStore::new(5),
            analysis_engine: Arc::new(
                analysis_engine::AnalysisEngine::new_default()
            ),
            policy_engine: Arc::new(
                policy_engine::PolicyEngine::default()
            ),
            synthesis_engine: Arc::new(
                synthesis::SynthesisEngine::default()
            ),
            autofix_engine: Arc::new(
                autofix::AutoFixEngine::new_default()
            ),
            frame_extractor: Arc::new(
                frame_extractor::FrameExtractor::new_default()
            ),
        }
    }

    #[tokio::test]
    async fn test_extract_frames_validation_empty_url() {
        let service = FrameExtractionService::new(dummy_server());

        let req = ExtractRequest {
            source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "".into(),
                metadata: HashMap::new(),
            },
            fps: 1.0,
            max_resolution: 1920,
            extract_audio: false,
        };

        let result = service.extract_frames(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_extract_frames_validation_bad_fps() {
        let service = FrameExtractionService::new(dummy_server());

        let req = ExtractRequest {
            source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: HashMap::new(),
            },
            fps: 0.0,
            max_resolution: 1920,
            extract_audio: false,
        };

        let result = service.extract_frames(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_extract_frames_validation_zero_resolution() {
        let service = FrameExtractionService::new(dummy_server());

        let req = ExtractRequest {
            source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: HashMap::new(),
            },
            fps: 1.0,
            max_resolution: 0,
            extract_audio: false,
        };

        let result = service.extract_frames(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_service_call_unimplemented_method() {
        let mut service = FrameExtractionService::new(dummy_server());

        let req = http::Request::builder()
            .uri("/unknown.Service/UnknownMethod")
            .body(tonic::body::BoxBody::default())
            .unwrap();

        use tower::Service;
        let resp = Service::call(&mut service, req).await.unwrap();
        let status_hdr = resp.headers().get("grpc-status").unwrap();
        assert_eq!(status_hdr, "12");
    }
}
