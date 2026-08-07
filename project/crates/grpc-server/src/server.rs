//! Sentinel gRPC server – shared state, subsystem initialisation and job management.

use crate::proto::*;
use anyhow::{Context, Result};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Server configuration
// ---------------------------------------------------------------------------

/// Configuration for the gRPC server and all subsystems.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Bind address for the tonic server.
    pub bind_addr: String,
    /// Path to frame extraction working directory.
    pub frame_work_dir: String,
    /// Maximum concurrent analysis jobs.
    pub max_concurrent_jobs: usize,
    /// Enable the auto-fix subsystem.
    pub enable_autofix: bool,
    /// Path to policy rules file / directory.
    pub policy_rules_path: String,
    /// Default video FPS for frame extraction.
    pub default_fps: f32,
    /// Maximum frame resolution.
    pub max_resolution: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:50051".into(),
            frame_work_dir: "/tmp/sentinel_frames".into(),
            max_concurrent_jobs: 10,
            enable_autofix: true,
            policy_rules_path: "/etc/sentinel/policies".into(),
            default_fps: 1.0,
            max_resolution: 1920,
        }
    }
}

impl ServerConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Result<Self> {
        use std::env;
        let mut cfg = ServerConfig::default();

        if let Ok(v) = env::var("SENTINEL_BIND_ADDR") {
            cfg.bind_addr = v;
        }
        if let Ok(v) = env::var("SENTINEL_FRAME_WORK_DIR") {
            cfg.frame_work_dir = v;
        }
        if let Ok(v) = env::var("SENTINEL_MAX_CONCURRENT_JOBS") {
            cfg.max_concurrent_jobs = v.parse().context("MAX_CONCURRENT_JOBS")?;
        }
        if let Ok(v) = env::var("SENTINEL_ENABLE_AUTOFIX") {
            cfg.enable_autofix = v.parse().unwrap_or(true);
        }
        if let Ok(v) = env::var("SENTINEL_POLICY_RULES_PATH") {
            cfg.policy_rules_path = v;
        }
        if let Ok(v) = env::var("SENTINEL_DEFAULT_FPS") {
            cfg.default_fps = v.parse().context("DEFAULT_FPS")?;
        }
        if let Ok(v) = env::var("SENTINEL_MAX_RESOLUTION") {
            cfg.max_resolution = v.parse().context("MAX_RESOLUTION")?;
        }

        Ok(cfg)
    }
}

// ---------------------------------------------------------------------------
// Stored job record (shared between pipeline and service layers)
// ---------------------------------------------------------------------------

/// Internal representation of an analysis job kept in the in-memory store.
#[derive(Debug, Clone)]
pub struct JobRecord {
    pub job_id: String,
    pub request: SubmitRequest,
    pub status: AnalysisStatus,
    pub progress: ProgressUpdate,
    pub report: Option<AnalysisReport>,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Broadcast channel for progress updates (re-subscribable by clients).
    pub progress_tx: broadcast::Sender<ProgressUpdate>,
}

impl JobRecord {
    fn new(request: SubmitRequest) -> Self {
        let job_id = Uuid::new_v4().to_string();
        let (progress_tx, _) = broadcast::channel::<ProgressUpdate>(256);
        let now = chrono::Utc::now();
        let progress = ProgressUpdate {
            job_id: job_id.clone(),
            status: AnalysisStatus::Pending,
            current_stage: 0,
            total_stages: 9,
            stage_name: "Pending".into(),
            progress_pct: 0.0,
            message: "Job accepted, waiting for resources".into(),
            timestamp: now,
            detail_json: "{}".into(),
        };
        Self {
            job_id,
            request,
            status: AnalysisStatus::Pending,
            progress: progress.clone(),
            report: None,
            error_message: None,
            created_at: now,
            completed_at: None,
            progress_tx,
        }
    }
}

// ---------------------------------------------------------------------------
// JobStore
// ---------------------------------------------------------------------------

/// Thread-safe in-memory storage for active and recently-completed jobs.
#[derive(Debug, Clone)]
pub struct JobStore {
    jobs: Arc<DashMap<String, JobRecord>>,
    max_jobs: usize,
}

impl JobStore {
    pub fn new(max_jobs: usize) -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
            max_jobs,
        }
    }

    /// Create a new job entry and return its ID.
    pub fn create_job(&self, request: SubmitRequest) -> Result<String> {
        if self.jobs.len() >= self.max_jobs {
            anyhow::bail!("Maximum concurrent jobs ({}) reached", self.max_jobs);
        }
        let record = JobRecord::new(request);
        let id = record.job_id.clone();
        self.jobs.insert(id.clone(), record);
        info!(job_id = %id, "Created new analysis job");
        Ok(id)
    }

    /// Retrieve a clone of a job record by ID.
    pub fn get_job(&self, job_id: &str) -> Option<JobRecord> {
        self.jobs.get(job_id).map(|e| e.clone())
    }

    /// Update the progress of a running job.
    pub fn update_progress(&self, update: &ProgressUpdate) {
        if let Some(mut entry) = self.jobs.get_mut(&update.job_id) {
            entry.status = update.status;
            entry.progress = update.clone();
            // Broadcast to any streaming listeners.
            let _ = entry.progress_tx.send(update.clone());
            debug!(
                job_id = %update.job_id,
                stage = %update.stage_name,
                pct = update.progress_pct,
                "Progress update"
            );
        } else {
            warn!(job_id = %update.job_id, "Progress update for unknown job");
        }
    }

    /// Store the final analysis report and mark the job completed.
    pub fn set_result(&self, job_id: &str, report: AnalysisReport) {
        if let Some(mut entry) = self.jobs.get_mut(job_id) {
            entry.status = report.status;
            entry.report = Some(report.clone());
            entry.completed_at = Some(chrono::Utc::now());

            let update = ProgressUpdate {
                job_id: job_id.into(),
                status: report.status,
                current_stage: 9,
                total_stages: 9,
                stage_name: "Completed".into(),
                progress_pct: 100.0,
                message: "Analysis complete".into(),
                timestamp: chrono::Utc::now(),
                detail_json: serde_json::to_string(&report).unwrap_or_default(),
            };
            entry.progress = update.clone();
            let _ = entry.progress_tx.send(update);
            info!(job_id = %job_id, "Job completed and report stored");
        }
    }

    /// Mark a job as failed.
    pub fn set_failed(&self, job_id: &str, error: &str) {
        if let Some(mut entry) = self.jobs.get_mut(job_id) {
            entry.status = AnalysisStatus::Failed;
            entry.error_message = Some(error.into());
            entry.completed_at = Some(chrono::Utc::now());

            let update = ProgressUpdate {
                job_id: job_id.into(),
                status: AnalysisStatus::Failed,
                current_stage: 9,
                total_stages: 9,
                stage_name: "Failed".into(),
                progress_pct: 0.0,
                message: error.into(),
                timestamp: chrono::Utc::now(),
                detail_json: serde_json::json!({"error": error}).to_string(),
            };
            entry.progress = update.clone();
            let _ = entry.progress_tx.send(update);
            error!(job_id = %job_id, error = %error, "Job failed");
        }
    }

    /// Get a broadcast receiver for progress updates of a specific job.
    pub fn subscribe_progress(&self, job_id: &str) -> Option<broadcast::Receiver<ProgressUpdate>> {
        self.jobs
            .get(job_id)
            .map(|entry| entry.progress_tx.subscribe())
    }

    /// List all active (non-completed, non-failed) job IDs.
    pub fn active_jobs(&self) -> Vec<String> {
        self.jobs
            .iter()
            .filter(|e| {
                !matches!(e.status, AnalysisStatus::Completed)
                    && !matches!(e.status, AnalysisStatus::Failed)
                    && !matches!(e.status, AnalysisStatus::Cancelled)
            })
            .map(|e| e.job_id.clone())
            .collect()
    }

    /// Current number of stored jobs.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

// ---------------------------------------------------------------------------
// SentinelServer (root state object)
// ---------------------------------------------------------------------------

/// Root server state shared across all tonic service handlers.
#[derive(Debug, Clone)]
pub struct SentinelServer {
    pub config: ServerConfig,
    pub job_store: JobStore,
    pub analysis_engine: Arc<analysis_engine::AnalysisEngine>,
    pub policy_engine: Arc<policy_engine::PolicyEngine>,
    pub synthesis_engine: Arc<synthesis::SynthesisEngine>,
    pub autofix_engine: Arc<autofix::AutoFixEngine>,
    pub frame_extractor: Arc<frame_extractor::FrameExtractor>,
}

impl SentinelServer {
    /// Initialise all subsystems.
    pub fn new(config: ServerConfig) -> Result<Self> {
        info!("Initialising SentinelServer subsystems...");

        let frame_extractor = Arc::new(
            frame_extractor::FrameExtractor::new(&config.frame_work_dir)
                .context("frame extractor init")?,
        );
        info!("Frame extractor initialised");

        let analysis_engine = Arc::new(
            analysis_engine::AnalysisEngine::new()
                .context("analysis engine init")?,
        );
        info!("Analysis engine initialised");

        let policy_engine = Arc::new(
            policy_engine::PolicyEngine::load(&config.policy_rules_path)
                .context("policy engine init")?,
        );
        info!("Policy engine initialised");

        let synthesis_engine = Arc::new(
            synthesis::SynthesisEngine::new()
                .context("synthesis engine init")?,
        );
        info!("Synthesis engine initialised");

        let autofix_engine = Arc::new(
            autofix::AutoFixEngine::new(&config.frame_work_dir)
                .context("autofix engine init")?,
        );
        info!("Auto-fix engine initialised");

        let job_store = JobStore::new(config.max_concurrent_jobs);

        Ok(Self {
            config,
            job_store,
            analysis_engine,
            policy_engine,
            synthesis_engine,
            autofix_engine,
            frame_extractor,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_defaults() {
        // Ensure defaults are sensible when env vars are missing.
        let cfg = ServerConfig::default();
        assert_eq!(cfg.bind_addr, "0.0.0.0:50051");
        assert_eq!(cfg.max_concurrent_jobs, 10);
        assert!(cfg.enable_autofix);
    }

    #[test]
    fn test_job_store_create_and_get() {
        let store = JobStore::new(5);
        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: Default::default(),
            },
            config_json: "{}".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };
        let id = store.create_job(req.clone()).unwrap();
        assert!(!id.is_empty());

        let job = store.get_job(&id).unwrap();
        assert_eq!(job.job_id, id);
        assert!(matches!(job.status, AnalysisStatus::Pending));
    }

    #[test]
    fn test_job_store_max_jobs() {
        let store = JobStore::new(2);
        for i in 0..3 {
            let req = SubmitRequest {
                video_source: VideoSource {
                    source_id: format!("s{}", i),
                    source_type: "youtube".into(),
                    url: format!("https://youtu.be/{}", i),
                    metadata: Default::default(),
                },
                config_json: "{}".into(),
                enable_autofix: false,
                callback_url: "".into(),
            };
            if i < 2 {
                assert!(store.create_job(req).is_ok());
            } else {
                assert!(store.create_job(req).is_err());
            }
        }
    }

    #[test]
    fn test_job_store_update_progress() {
        let store = JobStore::new(5);
        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: Default::default(),
            },
            config_json: "{}".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };
        let id = store.create_job(req).unwrap();

        let update = ProgressUpdate {
            job_id: id.clone(),
            status: AnalysisStatus::ExtractingFrames,
            current_stage: 1,
            total_stages: 9,
            stage_name: "Extracting Frames".into(),
            progress_pct: 15.0,
            message: "Extracting frame batch 3/20".into(),
            timestamp: chrono::Utc::now(),
            detail_json: "{}".into(),
        };
        store.update_progress(&update);

        let job = store.get_job(&id).unwrap();
        assert!(matches!(job.status, AnalysisStatus::ExtractingFrames));
        assert!((job.progress.progress_pct - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_job_store_set_result() {
        let store = JobStore::new(5);
        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: Default::default(),
            },
            config_json: "{}".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };
        let id = store.create_job(req.clone()).unwrap();

        let report = AnalysisReport {
            job_id: id.clone(),
            status: AnalysisStatus::Completed,
            video_source: req.video_source,
            visual_analysis: None,
            audio_analysis: None,
            metadata_analysis: None,
            synthesis: None,
            fixes_applied: vec![],
            created_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            processing_time_ms: 1234,
        };
        store.set_result(&id, report);

        let job = store.get_job(&id).unwrap();
        assert!(matches!(job.status, AnalysisStatus::Completed));
        assert!(job.report.is_some());
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_job_store_set_failed() {
        let store = JobStore::new(5);
        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: Default::default(),
            },
            config_json: "{}".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };
        let id = store.create_job(req).unwrap();

        store.set_failed(&id, "FFmpeg not found");

        let job = store.get_job(&id).unwrap();
        assert!(matches!(job.status, AnalysisStatus::Failed));
        assert_eq!(job.error_message, Some("FFmpeg not found".into()));
    }

    #[test]
    fn test_job_store_subscribe_progress() {
        let store = JobStore::new(5);
        let req = SubmitRequest {
            video_source: VideoSource {
                source_id: "s1".into(),
                source_type: "youtube".into(),
                url: "https://youtu.be/test".into(),
                metadata: Default::default(),
            },
            config_json: "{}".into(),
            enable_autofix: false,
            callback_url: "".into(),
        };
        let id = store.create_job(req).unwrap();

        let mut rx = store.subscribe_progress(&id).unwrap();
        // Send an update through the store so the broadcast fires.
        let update = ProgressUpdate {
            job_id: id.clone(),
            status: AnalysisStatus::AnalyzingVisual,
            current_stage: 2,
            total_stages: 9,
            stage_name: "Visual Analysis".into(),
            progress_pct: 50.0,
            message: "Running detection".into(),
            timestamp: chrono::Utc::now(),
            detail_json: "{}".into(),
        };
        store.update_progress(&update);

        let received = rx.try_recv().expect("should receive progress");
        assert!(matches!(received.status, AnalysisStatus::AnalyzingVisual));
    }
}
