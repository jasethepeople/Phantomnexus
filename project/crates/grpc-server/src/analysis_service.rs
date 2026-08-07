//! gRPC analysis service implementation.
//!
//! Implements the main `SentinelAnalysis` tonic service with methods for
//! submitting videos, streaming progress, retrieving reports, and applying
//! auto-fixes.

use crate::pipeline::AnalysisPipeline;
use crate::proto::*;
use crate::server::{JobRecord, SentinelServer};
use anyhow::{Context, Result};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, instrument, warn};

// ---------------------------------------------------------------------------
// AnalysisService
// ---------------------------------------------------------------------------

/// Concrete tonic service backed by `SentinelServer` shared state.
#[derive(Debug, Clone)]
pub struct AnalysisService {
    server: SentinelServer,
}

impl AnalysisService {
    pub fn new(server: SentinelServer) -> Self {
        Self { server }
    }

    /// Convert a pipeline error into a tonic `Status`.
    fn to_status<E: std::fmt::Display>(err: E) -> Status {
        Status::internal(format!("Pipeline error: {}", err))
    }
}

// -- Trait defining the service interface (what tonic-build would generate) --

/// gRPC service trait – equivalent to what tonic-build would generate.
pub trait SentinelAnalysis: Send + Sync + 'static {
    type GetStatusStream: Stream<Item = Result<ProgressUpdate, Status>> + Send + 'static;

    async fn submit_video(
        &self,
        request: Request<SubmitRequest>,
    ) -> Result<Response<SubmitResponse>, Status>;

    async fn get_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<Pin<Box<Self::GetStatusStream>>>, Status>;

    async fn get_report(
        &self,
        request: Request<ReportRequest>,
    ) -> Result<Response<AnalysisReport>, Status>;

    async fn apply_fix(
        &self,
        request: Request<FixRequest>,
    ) -> Result<Response<FixResponse>, Status>;
}

impl SentinelAnalysis for AnalysisService {
    type GetStatusStream = Pin<Box<dyn Stream<Item = Result<ProgressUpdate, Status>> + Send>>;

    #[instrument(skip(self), fields(job_id))]
    async fn submit_video(
        &self,
        request: Request<SubmitRequest>,
    ) -> Result<Response<SubmitResponse>, Status> {
        let req = request.into_inner();
        info!(
            source_id = %req.video_source.source_id,
            source_type = %req.video_source.source_type,
            "Received submit_video request"
        );

        // Basic validation.
        if req.video_source.url.is_empty() {
            return Err(Status::invalid_argument("Video URL is empty"));
        }
        if req.video_source.source_id.is_empty() {
            return Err(Status::invalid_argument("Video source_id is empty"));
        }

        // Validate config JSON if provided.
        if !req.config_json.is_empty() {
            serde_json::from_str::<serde_json::Value>(&req.config_json)
                .map_err(|e| Status::invalid_argument(format!("Invalid config JSON: {}", e)))?;
        }

        // Create job.
        let job_id = self
            .server
            .job_store
            .create_job(req.clone())
            .map_err(|e| Status::resource_exhausted(format!("Job queue full: {}", e)))?;

        info!(%job_id, "Job created, spawning pipeline");

        // Spawn the 10-stage pipeline in the background.
        let server = self.server.clone();
        let (progress_tx, _progress_rx) = mpsc::channel::<ProgressUpdate>(256);
        let job = server
            .job_store
            .get_job(&job_id)
            .ok_or_else(|| Status::internal("Job disappeared after creation"))?;

        tokio::spawn(async move {
            let result = AnalysisPipeline::run(job.clone(), server.clone(), progress_tx.clone()).await;
            match result {
                Ok(analysis_result) => {
                    server.job_store.set_result(&job_id, analysis_result.report);
                    info!(%job_id, "Pipeline completed successfully");
                }
                Err(e) => {
                    error!(%job_id, error = %e, "Pipeline failed");
                    server.job_store.set_failed(&job_id, &e.to_string());
                }
            }
        });

        let response = SubmitResponse {
            job_id: job_id.clone(),
            status: AnalysisStatus::Pending,
            message: "Job accepted and queued".into(),
            accepted_at: chrono::Utc::now(),
        };

        Ok(Response::new(response))
    }

    #[instrument(skip(self), fields(job_id = %request.get_ref().job_id))]
    async fn get_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<Self::GetStatusStream>, Status> {
        let req = request.into_inner();
        info!(job_id = %req.job_id, "Received get_status request");

        let job = self
            .server
            .job_store
            .get_job(&req.job_id)
            .ok_or_else(|| Status::not_found(format!("Job '{}' not found", req.job_id)))?;

        // Try to get a broadcast subscription for live updates.
        let broadcast_rx = self.server.job_store.subscribe_progress(&req.job_id);

        if let Some(rx) = broadcast_rx {
            // Wrap broadcast receiver as a stream.
            let stream = BroadcastStream::new(rx);
            let mapped = stream.map(|result| {
                match result {
                    Ok(update) => Ok(update),
                    Err(e) => Err(Status::internal(format!("Broadcast lagged: {}", e))),
                }
            });
            Ok(Response::new(Box::pin(mapped) as Self::GetStatusStream))
        } else {
            // Job not found after all – return single-item stream with current status.
            let update = job.progress.clone();
            let stream = futures::stream::iter(vec![Ok(update)]);
            Ok(Response::new(Box::pin(stream) as Self::GetStatusStream))
        }
    }

    #[instrument(skip(self), fields(job_id = %request.get_ref().job_id))]
    async fn get_report(
        &self,
        request: Request<ReportRequest>,
    ) -> Result<Response<AnalysisReport>, Status> {
        let req = request.into_inner();
        info!(job_id = %req.job_id, "Received get_report request");

        let job = self
            .server
            .job_store
            .get_job(&req.job_id)
            .ok_or_else(|| Status::not_found(format!("Job '{}' not found", req.job_id)))?;

        let report = job
            .report
            .ok_or_else(|| Status::failed_precondition("Job has not completed yet"))?;

        Ok(Response::new(report))
    }

    #[instrument(skip(self), fields(job_id = %request.get_ref().job_id))]
    async fn apply_fix(
        &self,
        request: Request<FixRequest>,
    ) -> Result<Response<FixResponse>, Status> {
        let req = request.into_inner();
        info!(job_id = %req.job_id, fix_count = req.fixes.len(), "Received apply_fix request");

        if req.fixes.is_empty() {
            return Err(Status::invalid_argument("No fixes provided"));
        }

        let job = self
            .server
            .job_store
            .get_job(&req.job_id)
            .ok_or_else(|| Status::not_found(format!("Job '{}' not found", req.job_id)))?;

        let mut results = Vec::with_capacity(req.fixes.len());

        for fix in &req.fixes {
            debug!(fix_id = %fix.fix_id, fix_type = %fix.fix_type, "Applying fix");
            let fix_result = self
                .apply_single_fix(&job, fix)
                .await
                .map_err(|e| Status::internal(format!("Fix {} failed: {}", fix.fix_id, e)))?;
            results.push(fix_result);
        }

        let all_applied = results.iter().all(|r| r.applied);

        let response = FixResponse {
            job_id: req.job_id,
            results,
            all_applied,
        };

        Ok(Response::new(response))
    }
}

impl AnalysisService {
    /// Apply a single fix using the autofix engine.
    async fn apply_single_fix(
        &self,
        job: &JobRecord,
        fix: &AutoFix,
    ) -> Result<FixResult> {
        let engine = Arc::clone(&self.server.autofix_engine);
        let fix_json = serde_json::to_string(fix)?;
        let work_dir = self.server.config.frame_work_dir.clone();

        let result = tokio::task::spawn_blocking(move || {
            engine
                .apply_single(&fix_json, &work_dir)
                .map_err(|e| anyhow::anyhow!("Single fix failed: {}", e))
        })
        .await
        .context("fix task panicked")?;

        match result {
            Ok(r) => Ok(FixResult {
                fix_id: r.fix_id,
                applied: r.applied,
                output_path: r.output_path,
                processing_time_ms: r.processing_time_ms,
                error_message: r.error_message,
            }),
            Err(e) => Ok(FixResult {
                fix_id: fix.fix_id.clone(),
                applied: false,
                output_path: String::new(),
                processing_time_ms: 0,
                error_message: e.to_string(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tower Service implementation (manual – replaces tonic-build generated code)
// ---------------------------------------------------------------------------

use std::future::Future;
use std::task::{Context as TaskContext, Poll};
use tower::Service;

impl Service<http::Request<tonic::body::BoxBody>> for AnalysisService {
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
                    let response = http::Response::builder()
                        .status(200)
                        .header("grpc-status", "2") // UNKNOWN
                        .header("grpc-message", "failed to read body")
                        .body(tonic::body::BoxBody::default())
                        .unwrap();
                    return Ok(response);
                }
            };

            let response = match path {
                "/sentinel.v1.SentinelAnalysis/SubmitVideo" => {
                    match serde_json::from_slice::<SubmitRequest>(&body_bytes) {
                        Ok(request) => {
                            match svc.submit_video(Request::new(request)).await {
                                Ok(resp) => encode_response(resp.into_inner()),
                                Err(status) => status_to_response(status),
                            }
                        }
                        Err(e) => status_to_response(Status::invalid_argument(format!("JSON decode: {}", e))),
                    }
                }
                "/sentinel.v1.SentinelAnalysis/GetStatus" => {
                    match serde_json::from_slice::<StatusRequest>(&body_bytes) {
                        Ok(request) => {
                            match svc.get_status(Request::new(request)).await {
                                Ok(_resp) => {
                                    // Streaming responses handled separately.
                                    let response = http::Response::builder()
                                        .status(200)
                                        .header("grpc-status", "0")
                                        .header("content-type", "application/grpc")
                                        .body(tonic::body::BoxBody::default())
                                        .unwrap();
                                    response
                                }
                                Err(status) => status_to_response(status),
                            }
                        }
                        Err(e) => status_to_response(Status::invalid_argument(format!("JSON decode: {}", e))),
                    }
                }
                "/sentinel.v1.SentinelAnalysis/GetReport" => {
                    match serde_json::from_slice::<ReportRequest>(&body_bytes) {
                        Ok(request) => {
                            match svc.get_report(Request::new(request)).await {
                                Ok(resp) => encode_response(resp.into_inner()),
                                Err(status) => status_to_response(status),
                            }
                        }
                        Err(e) => status_to_response(Status::invalid_argument(format!("JSON decode: {}", e))),
                    }
                }
                "/sentinel.v1.SentinelAnalysis/ApplyFix" => {
                    match serde_json::from_slice::<FixRequest>(&body_bytes) {
                        Ok(request) => {
                            match svc.apply_fix(Request::new(request)).await {
                                Ok(resp) => encode_response(resp.into_inner()),
                                Err(status) => status_to_response(status),
                            }
                        }
                        Err(e) => status_to_response(Status::invalid_argument(format!("JSON decode: {}", e))),
                    }
                }
                _ => {
                    let response = http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12") // UNIMPLEMENTED
                        .header("grpc-message", format!("method {} not found", path))
                        .body(tonic::body::BoxBody::default())
                        .unwrap();
                    response
                }
            };

            Ok(response)
        })
    }
}

/// Encode a serializable response into a gRPC HTTP response.
fn encode_response<T: serde::Serialize>(value: T) -> http::Response<tonic::body::BoxBody> {
    let json = serde_json::to_vec(&value).unwrap_or_default();
    http::Response::builder()
        .status(200)
        .header("grpc-status", "0")
        .header("content-type", "application/grpc+json")
        .body(tonic::body::BoxBody::from(json))
        .unwrap()
}

/// Convert a tonic `Status` into an HTTP gRPC error response.
fn status_to_response(status: Status) -> http::Response<tonic::body::BoxBody> {
    http::Response::builder()
        .status(200)
        .header("grpc-status", format!("{}", status.code() as i32))
        .header("grpc-message", status.message())
        .body(tonic::body::BoxBody::default())
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build a minimal SentinelServer for tests.
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
    async fn test_submit_video_validation_empty_url() {
        let service = AnalysisService::new(dummy_server());

        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "".into(),
                metadata: Default::default(),
            },
            config_json: "{}".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };

        let result = service.submit_video(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_submit_video_validation_invalid_json() {
        let service = AnalysisService::new(dummy_server());

        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: Default::default(),
            },
            config_json: "not json {{{".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };

        let result = service.submit_video(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_get_report_job_not_found() {
        let service = AnalysisService::new(dummy_server());

        let req = ReportRequest {
            job_id: "nonexistent".into(),
        };

        let result = service.get_report(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_apply_fix_empty() {
        let service = AnalysisService::new(dummy_server());

        let req = FixRequest {
            job_id: "job-1".into(),
            fixes: vec![],
            apply_automatically: false,
        };

        let result = service.apply_fix(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_apply_fix_job_not_found() {
        let service = AnalysisService::new(dummy_server());

        let req = FixRequest {
            job_id: "nonexistent".into(),
            fixes: vec![AutoFix {
                fix_id: "f1".into(),
                fix_type: "blur".into(),
                target_start: Timestamp { seconds: 0.0 },
                target_end: Timestamp { seconds: 10.0 },
                description: "Blur sensitive region".into(),
                parameters_json: "{}".into(),
            }],
            apply_automatically: true,
        };

        let result = service.apply_fix(Request::new(req)).await;
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn test_service_call_unimplemented_method() {
        let mut service = AnalysisService::new(dummy_server());

        let req = http::Request::builder()
            .uri("/unknown.Service/UnknownMethod")
            .body(tonic::body::BoxBody::default())
            .unwrap();

        let resp = Service::call(&mut service, req).await.unwrap();
        let status_hdr = resp.headers().get("grpc-status").unwrap();
        assert_eq!(status_hdr, "12"); // UNIMPLEMENTED
    }
}
