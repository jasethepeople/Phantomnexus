//! gRPC audio analysis service.
//!
//! Provides a unary endpoint that analyses an audio track and returns a
//! complete `AudioAnalysis` result.

use crate::proto::{AudioAnalysis, AudioRequest, Timestamp, TranscriptSegment, AudioEvent, CopyrightMatch};
use crate::server::SentinelServer;
use anyhow::Result;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, instrument, warn};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait AudioAnalysisServiceTrait: Send + Sync + 'static {
    async fn analyze_audio(
        &self,
        request: Request<AudioRequest>,
    ) -> Result<Response<AudioAnalysis>, Status>;
}

// ---------------------------------------------------------------------------
// AudioAnalysisService
// ---------------------------------------------------------------------------

/// Service that performs audio analysis on a given audio file path.
#[derive(Debug, Clone)]
pub struct AudioAnalysisService {
    server: SentinelServer,
}

impl AudioAnalysisService {
    pub fn new(server: SentinelServer) -> Self {
        Self { server }
    }
}

impl AudioAnalysisServiceTrait for AudioAnalysisService {
    #[instrument(skip(self), fields(job_id = %request.get_ref().job_id))]
    async fn analyze_audio(
        &self,
        request: Request<AudioRequest>,
    ) -> Result<Response<AudioAnalysis>, Status> {
        let req = request.into_inner();
        info!(
            job_id = %req.job_id,
            audio_path = %req.audio_path,
            "Received analyze_audio request"
        );

        // Validate request.
        if req.job_id.is_empty() {
            return Err(Status::invalid_argument("job_id is empty"));
        }
        if req.audio_path.is_empty() {
            return Err(Status::invalid_argument("audio_path is empty"));
        }

        let engine = Arc::clone(&self.server.analysis_engine);
        let audio_path = req.audio_path.clone();

        // Run audio analysis on a blocking thread.
        let result = tokio::task::spawn_blocking(move || {
            engine
                .analyze_audio(&audio_path)
                .map_err(|e| anyhow::anyhow!("Audio analysis failed: {}", e))
        })
        .await
        .map_err(|e| Status::internal(format!("Audio analysis task panicked: {}", e)))?
        .map_err(|e| Status::internal(format!("Audio analysis failed: {}", e)))?;

        // Convert internal result to proto-equivalent type.
        let mut analysis = AudioAnalysis {
            language: result.language,
            duration_seconds: result.duration_seconds,
            summary: result.summary,
            ..Default::default()
        };

        for ts in &result.transcript {
            analysis.transcript.push(TranscriptSegment {
                start: Timestamp::from_seconds(ts.start_seconds),
                end: Timestamp::from_seconds(ts.end_seconds),
                text: ts.text.clone(),
                confidence: ts.confidence,
                language: ts.language.clone(),
                speaker_id: ts.speaker_id.clone(),
            });
        }

        for ev in &result.audio_events {
            analysis.audio_events.push(AudioEvent {
                event_type: ev.event_type.clone(),
                start: Timestamp::from_seconds(ev.start_seconds),
                end: Timestamp::from_seconds(ev.end_seconds),
                confidence: ev.confidence,
                source_track: ev.source_track.clone(),
                related_transcript_idx: ev.related_transcript_idx,
            });
        }

        for cm in &result.copyright_matches {
            analysis.copyright_matches.push(CopyrightMatch {
                match_type: cm.match_type.clone(),
                start: Timestamp::from_seconds(cm.start_seconds),
                end: Timestamp::from_seconds(cm.end_seconds),
                confidence: cm.confidence,
                reference_id: cm.reference_id.clone(),
                reference_title: cm.reference_title.clone(),
                reference_owner: cm.reference_owner.clone(),
                claim_policy: cm.claim_policy.clone(),
            }));
        }

        info!(
            transcript_segments = analysis.transcript.len(),
            audio_events = analysis.audio_events.len(),
            copyright_matches = analysis.copyright_matches.len(),
            "Audio analysis complete"
        );

        Ok(Response::new(analysis))
    }
}

// ---------------------------------------------------------------------------
// Tower Service implementation
// ---------------------------------------------------------------------------

use std::future::Future;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use tower::Service;

impl Service<http::Request<tonic::body::BoxBody>> for AudioAnalysisService {
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
                "/sentinel.v1.AudioAnalysis/AnalyzeAudio" => {
                    match serde_json::from_slice::<AudioRequest>(&body_bytes) {
                        Ok(request) => {
                            match svc.analyze_audio(Request::new(request)).await {
                                Ok(resp) => {
                                    let json = serde_json::to_vec(&resp.into_inner()).unwrap_or_default();
                                    http::Response::builder()
                                        .status(200)
                                        .header("grpc-status", "0")
                                        .header("content-type", "application/grpc+json")
                                        .body(tonic::body::BoxBody::from(json))
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
    async fn test_analyze_audio_empty_job_id() {
        let service = AudioAnalysisService::new(dummy_server());

        let req = AudioRequest {
            job_id: "".into(),
            audio_path: "/tmp/audio.wav".into(),
        };

        let result = service.analyze_audio(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_analyze_audio_empty_path() {
        let service = AudioAnalysisService::new(dummy_server());

        let req = AudioRequest {
            job_id: "job-1".into(),
            audio_path: "".into(),
        };

        let result = service.analyze_audio(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_service_call_unimplemented_method() {
        let mut service = AudioAnalysisService::new(dummy_server());

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
