use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::models::ui::{
    AnalysisReportView, FixResponse, JobListItem, JobStatus,
    SubmitAnalysisRequest, SubmitAnalysisResponse,
};

/// Custom API error type
#[derive(Debug, Clone)]
pub enum ApiError {
    Network(String),
    Serialization(String),
    Http(u16, String),
    NotFound,
    Unauthorized,
    ServerError(String),
    Unknown(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Network(msg) => write!(f, "Network error: {}", msg),
            ApiError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            ApiError::Http(code, msg) => write!(f, "HTTP {}: {}", code, msg),
            ApiError::NotFound => write!(f, "Resource not found"),
            ApiError::Unauthorized => write!(f, "Unauthorized"),
            ApiError::ServerError(msg) => write!(f, "Server error: {}", msg),
            ApiError::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        ApiError::Network(err.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// API client for communicating with the Sentinel backend
#[derive(Debug, Clone)]
pub struct ApiClient {
    base_url: String,
}

impl ApiClient {
    /// Create a new API client with the given base URL
    pub fn new(base_url: String) -> Self {
        let base = base_url.trim_end_matches('/').to_string();
        Self { base_url: base }
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build a full URL from a path
    fn url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("{}/{}", self.base_url, path)
    }

    /// Helper: send GET request and deserialize JSON response
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> ApiResult<T> {
        let url = self.url(path);
        log::debug!("GET {}", url);

        let response = Request::get(&url)
            .header("Accept", "application/json")
            .send()
            .await?;

        let status = response.status();
        if status == 404 {
            return Err(ApiError::NotFound);
        }
        if status == 401 {
            return Err(ApiError::Unauthorized);
        }
        if status >= 500 {
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::ServerError(text));
        }
        if status >= 400 {
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::Http(status, text));
        }

        response
            .json::<T>()
            .await
            .map_err(|e| ApiError::Serialization(e.to_string()))
    }

    /// Helper: send POST request with JSON body and deserialize response
    async fn post<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let url = self.url(path);
        let body_json = serde_json::to_string(body)
            .map_err(|e| ApiError::Serialization(e.to_string()))?;
        log::debug!("POST {} - body: {}", url, body_json);

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body_json)
            .map_err(|e| ApiError::Serialization(e.to_string()))?
            .send()
            .await?;

        let status = response.status();
        if status == 404 {
            return Err(ApiError::NotFound);
        }
        if status == 401 {
            return Err(ApiError::Unauthorized);
        }
        if status >= 500 {
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::ServerError(text));
        }
        if status >= 400 {
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::Http(status, text));
        }

        response
            .json::<T>()
            .await
            .map_err(|e| ApiError::Serialization(e.to_string()))
    }

    /// Helper: send PUT request
    async fn put<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> ApiResult<T> {
        let url = self.url(path);
        let body_json = serde_json::to_string(body)
            .map_err(|e| ApiError::Serialization(e.to_string()))?;

        let response = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body_json)
            .map_err(|e| ApiError::Serialization(e.to_string()))?
            .send()
            .await?;

        let status = response.status();
        if status == 404 {
            return Err(ApiError::NotFound);
        }
        if status >= 500 {
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::ServerError(text));
        }
        if status >= 400 {
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::Http(status, text));
        }

        response
            .json::<T>()
            .await
            .map_err(|e| ApiError::Serialization(e.to_string()))
    }

    // ========================================================================
    // Public API Methods
    // ========================================================================

    /// Submit a new video analysis job
    pub async fn submit_analysis(&self, request: SubmitAnalysisRequest) -> ApiResult<SubmitAnalysisResponse> {
        self.post("/analyze/submit", &request).await
    }

    /// Get the status of a job
    pub async fn get_job_status(&self, job_id: &str) -> ApiResult<JobStatus> {
        self.get(&format!("/jobs/{}/status", job_id)).await
    }

    /// Get the full analysis report for a completed job
    pub async fn get_report(&self, job_id: &str) -> ApiResult<AnalysisReportView> {
        self.get(&format!("/jobs/{}/report", job_id)).await
    }

    /// Get a list of all jobs
    pub async fn list_jobs(&self) -> ApiResult<Vec<JobListItem>> {
        self.get("/jobs").await
    }

    /// Apply fixes for a job
    pub async fn apply_fix(&self, job_id: &str, fix_ids: Vec<String>) -> ApiResult<FixResponse> {
        let body = ApplyFixBody { fix_ids };
        self.post(&format!("/jobs/{}/fix", job_id), &body).await
    }

    /// Cancel a running job
    pub async fn cancel_job(&self, job_id: &str) -> ApiResult<JobStatus> {
        self.post(&format!("/jobs/{}/cancel", job_id), &serde_json::json!({})).await
    }

    /// Health check
    pub async fn health_check(&self) -> ApiResult<HealthResponse> {
        self.get("/health").await
    }
}

/// Request body for applying fixes
#[derive(Debug, Clone, Serialize)]
struct ApplyFixBody {
    fix_ids: Vec<String>,
}

/// Health check response
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}
