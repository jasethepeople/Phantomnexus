//! Analysis pipeline – 10-stage async pipeline that orchestrates all
//! subsystems for a single video analysis job.

use crate::proto::*;
use crate::server::{JobRecord, SentinelServer};
use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};

/// Final result produced by the pipeline.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub report: AnalysisReport,
    pub violations: Vec<Violation>,
    pub fixes_applied: Vec<FixResult>,
}

/// 10-stage analysis pipeline.
pub struct AnalysisPipeline;

impl AnalysisPipeline {
    /// Run the complete pipeline for a single job.
    #[instrument(skip(server, progress_tx), fields(job_id = %job.job_id))]
    pub async fn run(
        job: JobRecord,
        server: SentinelServer,
        progress_tx: mpsc::Sender<ProgressUpdate>,
    ) -> Result<AnalysisResult> {
        let job_id = job.job_id.clone();
        let start_time = std::time::Instant::now();
        info!("Starting analysis pipeline");

        // -- Stage 1: Frame extraction ---------------------------------------
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::ExtractingFrames,
            1,
            "Extracting frames from video",
            5.0,
            "Starting frame extraction",
        )
        .await;

        let frame_result = Self::stage_extract_frames(&job, &server, &progress_tx).await;
        let (frames, audio_path) = match frame_result {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, "Frame extraction failed");
                Self::send_progress(
                    &progress_tx,
                    &job_id,
                    AnalysisStatus::Failed,
                    9,
                    &format!("Frame extraction failed: {}", e),
                    0.0,
                    &e.to_string(),
                )
                .await;
                return Err(e);
            }
        };
        info!(frame_count = frames.len(), "Frame extraction complete");

        // -- Stage 2: Visual analysis (parallel with stage 3 prep) -----------
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::AnalyzingVisual,
            2,
            "Visual Analysis",
            20.0,
            "Running object detection, OCR, scene analysis",
        )
        .await;

        let visual = Self::stage_visual_analysis(&job, &server, &frames, &progress_tx).await
            .context("visual analysis")?;
        info!(
            objects = visual.objects.len(),
            text_regions = visual.detected_text.len(),
            scene_transitions = visual.scene_transitions.len(),
            flashes = visual.flashing_segments.len(),
            "Visual analysis complete"
        );

        // -- Stage 3: Audio analysis -----------------------------------------
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::AnalyzingAudio,
            3,
            "Audio Analysis",
            40.0,
            "Transcribing speech and detecting audio events",
        )
        .await;

        let audio = if let Some(audio_path) = audio_path {
            Self::stage_audio_analysis(&job, &server, &audio_path, &progress_tx).await
                .context("audio analysis")?
        } else {
            warn!("No audio track available; returning empty audio analysis");
            AudioAnalysis {
                transcript: vec![],
                audio_events: vec![],
                copyright_matches: vec![],
                language: String::new(),
                duration_seconds: 0.0,
                summary: "No audio track".into(),
            }
        };
        info!(
            transcript_segments = audio.transcript.len(),
            audio_events = audio.audio_events.len(),
            copyright_matches = audio.copyright_matches.len(),
            "Audio analysis complete"
        );

        // -- Stage 4: Metadata analysis --------------------------------------
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::AnalyzingMetadata,
            4,
            "Metadata Analysis",
            50.0,
            "Checking title, description, tags, thumbnails",
        )
        .await;

        let metadata = Self::stage_metadata_analysis(&job, &server, &progress_tx).await
            .context("metadata analysis")?;
        info!("Metadata analysis complete");

        // -- Stage 5: Synthesis ----------------------------------------------
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::Synthesizing,
            5,
            "Synthesizing Results",
            60.0,
            "Merging visual, audio and metadata findings",
        )
        .await;

        let synthesis = Self::stage_synthesis(
            &job, &server, &visual, &audio, &metadata, &progress_tx,
        )
        .await
        .context("synthesis")?;
        info!(
            violations = synthesis.violations.len(),
            risk_level = %synthesis.risk_level,
            "Synthesis complete"
        );

        // -- Stage 6: Apply policies -----------------------------------------
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::ApplyingPolicy,
            6,
            "Applying Policies",
            70.0,
            "Enforcing policy rules on detected violations",
        )
        .await;

        let policy_violations = Self::stage_apply_policies(
            &job, &server, &synthesis, &visual, &audio, &metadata, &progress_tx,
        )
        .await
        .context("policy application")?;
        info!(policy_violations = policy_violations.len(), "Policy check complete");

        // -- Stage 7: Auto-fix (optional) ------------------------------------
        let mut fixes_applied = vec![];
        if job.request.enable_autofix && server.config.enable_autofix {
            Self::send_progress(
                &progress_tx,
                &job_id,
                AnalysisStatus::RunningAutoFix,
                7,
                "Running Auto-Fix",
                80.0,
                "Attempting automatic remediation",
            )
            .await;

            fixes_applied = Self::stage_autofix(
                &job, &server, &policy_violations, &progress_tx,
            )
            .await
            .context("auto-fix")?;
            info!(fix_count = fixes_applied.len(), "Auto-fix stage complete");
        } else {
            debug!("Auto-fix disabled for this job");
        }

        // -- Stage 8: Validate fixes -----------------------------------------
        if job.request.enable_autofix && server.config.enable_autofix {
            Self::send_progress(
                &progress_tx,
                &job_id,
                AnalysisStatus::ValidatingFixes,
                8,
                "Validating Fixes",
                90.0,
                "Verifying remediation did not break content",
            )
            .await;

            let _ = Self::stage_validate_fixes(&job, &server, &fixes_applied, &progress_tx).await;
            info!("Fix validation complete");
        }

        // -- Stage 9: Finalize -----------------------------------------------
        let elapsed = start_time.elapsed().as_millis() as u64;
        Self::send_progress(
            &progress_tx,
            &job_id,
            AnalysisStatus::Completed,
            9,
            "Completed",
            100.0,
            "Analysis complete",
        )
        .await;

        let report = AnalysisReport {
            job_id: job_id.clone(),
            status: AnalysisStatus::Completed,
            video_source: job.request.video_source.clone(),
            visual_analysis: Some(visual.clone()),
            audio_analysis: Some(audio.clone()),
            metadata_analysis: Some(metadata.clone()),
            synthesis: Some(synthesis.clone()),
            fixes_applied: fixes_applied.clone(),
            created_at: job.created_at,
            completed_at: Some(Utc::now()),
            processing_time_ms: elapsed,
        };

        info!(
            total_violations = policy_violations.len(),
            fixes_applied = fixes_applied.len(),
            elapsed_ms = elapsed,
            "Pipeline finished successfully"
        );

        Ok(AnalysisResult {
            report,
            violations: policy_violations,
            fixes_applied,
        })
    }

    // -----------------------------------------------------------------------
    // Individual stages
    // -----------------------------------------------------------------------

    /// Stage 1 – Extract frames (and audio track if requested).
    async fn stage_extract_frames(
        job: &JobRecord,
        server: &SentinelServer,
        progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<(Vec<frame_extractor::Frame>, Option<String>)> {
        debug!("Stage 1: Extracting frames");

        let meta = job.request.video_source.to_metadata();
        let cfg = frame_extractor::ExtractConfig {
            fps: server.config.default_fps,
            max_resolution: server.config.max_resolution,
            extract_audio: true,
            work_dir: server.config.frame_work_dir.clone(),
        };

        // Spawn blocking frame extraction on a dedicated thread.
        let extractor = Arc::clone(&server.frame_extractor);
        let job_id = job.job_id.clone();
        let ptx = progress_tx.clone();

        let result = tokio::task::spawn_blocking(move || {
            let frames = extractor.extract(&meta, &cfg).map_err(|e| {
                anyhow::anyhow!("Frame extraction failed: {}", e)
            })?;

            // Send intermediate progress.
            let _ = ptx.try_send(ProgressUpdate {
                job_id: job_id.clone(),
                status: AnalysisStatus::ExtractingFrames,
                current_stage: 1,
                total_stages: 9,
                stage_name: "Extracting Frames".into(),
                progress_pct: 10.0,
                message: format!("Extracted {} frames", frames.len()),
                timestamp: Utc::now(),
                detail_json: serde_json::json!({"frame_count": frames.len()}).to_string(),
            });

            let audio_path = cfg.work_dir.clone()
                + "/"
                + &meta.source_id
                + "_audio.wav";

            Ok::<_, anyhow::Error>((frames, Some(audio_path)))
        })
        .await
        .context("frame extraction task panicked")??;

        Ok(result)
    }

    /// Stage 2 – Visual analysis on extracted frames.
    async fn stage_visual_analysis(
        job: &JobRecord,
        server: &SentinelServer,
        frames: &[frame_extractor::Frame],
        _progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<VisualAnalysis> {
        debug!("Stage 2: Visual analysis");

        let engine = Arc::clone(&server.analysis_engine);
        let frames_vec = frames.to_vec();
        let job_id = job.job_id.clone();

        let visual_proto = tokio::task::spawn_blocking(move || {
            let result = engine.analyze_visual(&frames_vec).map_err(|e| {
                anyhow::anyhow!("Visual analysis failed: {}", e)
            })?;

            // Convert internal result to proto type.
            let mut visual = VisualAnalysis {
                frame_count: frames_vec.len() as u64,
                duration_seconds: result.duration_seconds,
                summary: result.summary.clone(),
                ..Default::default()
            };

            for obj in &result.objects {
                visual.objects.push(DetectedObject {
                    label: obj.label.clone(),
                    confidence: obj.confidence,
                    bbox: obj.bbox.as_ref().map(|b| BoundingBox {
                        x: b.x,
                        y: b.y,
                        width: b.width,
                        height: b.height,
                    }),
                    frame_index: obj.frame_index,
                    timestamp: Timestamp::from_seconds(obj.timestamp_seconds),
                });
            }

            for txt in &result.detected_text {
                visual.detected_text.push(DetectedText {
                    text: txt.text.clone(),
                    confidence: txt.confidence,
                    bbox: txt.bbox.as_ref().map(|b| BoundingBox {
                        x: b.x,
                        y: b.y,
                        width: b.width,
                        height: b.height,
                    }),
                    frame_index: txt.frame_index,
                    timestamp: Timestamp::from_seconds(txt.timestamp_seconds),
                });
            }

            for st in &result.scene_transitions {
                visual.scene_transitions.push(SceneTransition {
                    frame_index: st.frame_index,
                    timestamp: Timestamp::from_seconds(st.timestamp_seconds),
                    transition_type: st.transition_type.clone(),
                    confidence: st.confidence,
                });
            }

            for fs in &result.flashing_segments {
                visual.flashing_segments.push(FlashingSegment {
                    start: Timestamp::from_seconds(fs.start_seconds),
                    end: Timestamp::from_seconds(fs.end_seconds),
                    frequency_hz: fs.frequency_hz,
                    max_luminance_delta: fs.max_luminance_delta,
                    risk_score: fs.risk_score,
                });
            }

            Ok::<_, anyhow::Error>(visual)
        })
        .await
        .context("visual analysis task panicked")??;

        Ok(visual_proto)
    }

    /// Stage 3 – Audio analysis on extracted audio track.
    async fn stage_audio_analysis(
        job: &JobRecord,
        server: &SentinelServer,
        audio_path: &str,
        _progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<AudioAnalysis> {
        debug!("Stage 3: Audio analysis");

        let engine = Arc::clone(&server.analysis_engine);
        let path = audio_path.to_string();
        let job_id = job.job_id.clone();

        let audio_proto = tokio::task::spawn_blocking(move || {
            let result = engine.analyze_audio(&path).map_err(|e| {
                anyhow::anyhow!("Audio analysis failed: {}", e)
            })?;

            let mut audio = AudioAnalysis {
                language: result.language.clone(),
                duration_seconds: result.duration_seconds,
                summary: result.summary.clone(),
                ..Default::default()
            };

            for ts in &result.transcript {
                audio.transcript.push(TranscriptSegment {
                    start: Timestamp::from_seconds(ts.start_seconds),
                    end: Timestamp::from_seconds(ts.end_seconds),
                    text: ts.text.clone(),
                    confidence: ts.confidence,
                    language: ts.language.clone(),
                    speaker_id: ts.speaker_id.clone(),
                });
            }

            for ev in &result.audio_events {
                audio.audio_events.push(AudioEvent {
                    event_type: ev.event_type.clone(),
                    start: Timestamp::from_seconds(ev.start_seconds),
                    end: Timestamp::from_seconds(ev.end_seconds),
                    confidence: ev.confidence,
                    source_track: ev.source_track.clone(),
                    related_transcript_idx: ev.related_transcript_idx,
                });
            }

            for cm in &result.copyright_matches {
                audio.copyright_matches.push(CopyrightMatch {
                    match_type: cm.match_type.clone(),
                    start: Timestamp::from_seconds(cm.start_seconds),
                    end: Timestamp::from_seconds(cm.end_seconds),
                    confidence: cm.confidence,
                    reference_id: cm.reference_id.clone(),
                    reference_title: cm.reference_title.clone(),
                    reference_owner: cm.reference_owner.clone(),
                    claim_policy: cm.claim_policy.clone(),
                });
            }

            Ok::<_, anyhow::Error>(audio)
        })
        .await
        .context("audio analysis task panicked")??;

        Ok(audio_proto)
    }

    /// Stage 4 – Metadata analysis.
    async fn stage_metadata_analysis(
        job: &JobRecord,
        server: &SentinelServer,
        _progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<MetadataAnalysis> {
        debug!("Stage 4: Metadata analysis");

        let engine = Arc::clone(&server.analysis_engine);
        let meta = job.request.video_source.to_metadata();

        let md_proto = tokio::task::spawn_blocking(move || {
            let result = engine.analyze_metadata(&meta).map_err(|e| {
                anyhow::anyhow!("Metadata analysis failed: {}", e)
            })?;

            Ok::<_, anyhow::Error>(MetadataAnalysis {
                title_issues: result.title_issues,
                description_issues: result.description_issues,
                tag_issues: result.tag_issues,
                category_mismatch: result.category_mismatch,
                age_gating_issues: result.age_gating_issues,
                thumbnails_issues: result.thumbnails_issues,
                summary: result.summary,
            })
        })
        .await
        .context("metadata analysis task panicked")??;

        Ok(md_proto)
    }

    /// Stage 5 – Synthesis of all analysis streams.
    async fn stage_synthesis(
        _job: &JobRecord,
        server: &SentinelServer,
        visual: &VisualAnalysis,
        audio: &AudioAnalysis,
        metadata: &MetadataAnalysis,
        _progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<SynthesisResult> {
        debug!("Stage 5: Synthesis");

        let engine = Arc::clone(&server.synthesis_engine);
        let visual_json = serde_json::to_string(visual)?;
        let audio_json = serde_json::to_string(audio)?;
        let metadata_json = serde_json::to_string(metadata)?;

        let synth = tokio::task::spawn_blocking(move || {
            let result = engine
                .synthesize(&visual_json, &audio_json, &metadata_json)
                .map_err(|e| anyhow::anyhow!("Synthesis failed: {}", e))?;

            let mut violations = Vec::new();
            for v in &result.violations {
                violations.push(Violation {
                    violation_id: v.id.clone(),
                    category: match v.category.as_str() {
                        "inappropriate_visual" => ViolationCategory::InappropriateVisual,
                        "hate_speech" => ViolationCategory::HateSpeech,
                        "harassment" => ViolationCategory::Harassment,
                        "copyright" => ViolationCategory::Copyright,
                        "spam" => ViolationCategory::Spam,
                        "misinformation" => ViolationCategory::Misinformation,
                        "graphic_content" => ViolationCategory::GraphicContent,
                        "dangerous_content" => ViolationCategory::DangerousContent,
                        "child_safety" => ViolationCategory::ChildSafety,
                        "privacy" => ViolationCategory::Privacy,
                        "flashing_content" => ViolationCategory::FlashingContent,
                        "audio_mismatch" => ViolationCategory::AudioMismatch,
                        "metadata_issue" => ViolationCategory::MetadataIssue,
                        _ => ViolationCategory::Other,
                    },
                    severity: match v.severity.as_str() {
                        "info" => Severity::Info,
                        "low" => Severity::Low,
                        "medium" => Severity::Medium,
                        "high" => Severity::High,
                        "critical" => Severity::Critical,
                        _ => Severity::Info,
                    },
                    start: Timestamp::from_seconds(v.start_seconds),
                    end: Timestamp::from_seconds(v.end_seconds),
                    description: v.description.clone(),
                    recommendation: v.recommendation.clone(),
                    source_stage: v.source_stage.clone(),
                    evidence_json: serde_json::to_string(&v.evidence).unwrap_or_default(),
                });
            }

            Ok::<_, anyhow::Error>(SynthesisResult {
                violations,
                overall_risk_score: result.overall_risk_score,
                risk_level: result.risk_level.clone(),
                summary: result.summary.clone(),
                suggested_actions: result.suggested_actions.clone(),
            })
        })
        .await
        .context("synthesis task panicked")??;

        Ok(synth)
    }

    /// Stage 6 – Apply policy rules.
    async fn stage_apply_policies(
        _job: &JobRecord,
        server: &SentinelServer,
        synthesis: &SynthesisResult,
        _visual: &VisualAnalysis,
        _audio: &AudioAnalysis,
        _metadata: &MetadataAnalysis,
        _progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<Vec<Violation>> {
        debug!("Stage 6: Applying policies");

        let engine = Arc::clone(&server.policy_engine);
        let violations_json = serde_json::to_string(&synthesis.violations)?;

        let filtered = tokio::task::spawn_blocking(move || {
            let result = engine
                .apply(&violations_json)
                .map_err(|e| anyhow::anyhow!("Policy application failed: {}", e))?;

            let mut out = Vec::new();
            for v in &result.violations {
                out.push(Violation {
                    violation_id: v.id.clone(),
                    category: match v.category.as_str() {
                        "inappropriate_visual" => ViolationCategory::InappropriateVisual,
                        "hate_speech" => ViolationCategory::HateSpeech,
                        "harassment" => ViolationCategory::Harassment,
                        "copyright" => ViolationCategory::Copyright,
                        "spam" => ViolationCategory::Spam,
                        "misinformation" => ViolationCategory::Misinformation,
                        "graphic_content" => ViolationCategory::GraphicContent,
                        "dangerous_content" => ViolationCategory::DangerousContent,
                        "child_safety" => ViolationCategory::ChildSafety,
                        "privacy" => ViolationCategory::Privacy,
                        "flashing_content" => ViolationCategory::FlashingContent,
                        "audio_mismatch" => ViolationCategory::AudioMismatch,
                        "metadata_issue" => ViolationCategory::MetadataIssue,
                        _ => ViolationCategory::Other,
                    },
                    severity: match v.severity.as_str() {
                        "info" => Severity::Info,
                        "low" => Severity::Low,
                        "medium" => Severity::Medium,
                        "high" => Severity::High,
                        "critical" => Severity::Critical,
                        _ => Severity::Info,
                    },
                    start: Timestamp::from_seconds(v.start_seconds),
                    end: Timestamp::from_seconds(v.end_seconds),
                    description: v.description.clone(),
                    recommendation: v.recommendation.clone(),
                    source_stage: v.source_stage.clone(),
                    evidence_json: serde_json::to_string(&v.evidence).unwrap_or_default(),
                });
            }
            Ok::<_, anyhow::Error>(out)
        })
        .await
        .context("policy task panicked")??;

        Ok(filtered)
    }

    /// Stage 7 – Run auto-fix on violations.
    async fn stage_autofix(
        job: &JobRecord,
        server: &SentinelServer,
        violations: &[Violation],
        progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<Vec<FixResult>> {
        debug!("Stage 7: Auto-fix");

        let engine = Arc::clone(&server.autofix_engine);
        let violations_json = serde_json::to_string(violations)?;
        let work_dir = server.config.frame_work_dir.clone();
        let job_id = job.job_id.clone();
        let ptx = progress_tx.clone();

        let fixes = tokio::task::spawn_blocking(move || {
            let result = engine
                .apply_fixes(&violations_json, &work_dir)
                .map_err(|e| anyhow::anyhow!("Auto-fix failed: {}", e))?;

            let mut fix_results = Vec::new();
            for (i, fix) in result.fixes.iter().enumerate() {
                let _ = ptx.try_send(ProgressUpdate {
                    job_id: job_id.clone(),
                    status: AnalysisStatus::RunningAutoFix,
                    current_stage: 7,
                    total_stages: 9,
                    stage_name: "Running Auto-Fix".into(),
                    progress_pct: 80.0 + (10.0 * i as f32 / result.fixes.len().max(1) as f32),
                    message: format!("Applying fix {}/{}", i + 1, result.fixes.len()),
                    timestamp: Utc::now(),
                    detail_json: serde_json::to_string(fix).unwrap_or_default(),
                });

                fix_results.push(FixResult {
                    fix_id: fix.fix_id.clone(),
                    applied: fix.applied,
                    output_path: fix.output_path.clone(),
                    processing_time_ms: fix.processing_time_ms,
                    error_message: fix.error_message.clone(),
                });
            }

            Ok::<_, anyhow::Error>(fix_results)
        })
        .await
        .context("autofix task panicked")??;

        Ok(fixes)
    }

    /// Stage 8 – Validate applied fixes.
    async fn stage_validate_fixes(
        _job: &JobRecord,
        server: &SentinelServer,
        fixes: &[FixResult],
        _progress_tx: &mpsc::Sender<ProgressUpdate>,
    ) -> Result<()> {
        debug!("Stage 8: Validating fixes");

        let engine = Arc::clone(&server.autofix_engine);
        let fixes_json = serde_json::to_string(fixes)?;

        tokio::task::spawn_blocking(move || {
            engine
                .validate_fixes(&fixes_json)
                .map_err(|e| anyhow::anyhow!("Fix validation failed: {}", e))?;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("validate task panicked")??;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    async fn send_progress(
        tx: &mpsc::Sender<ProgressUpdate>,
        job_id: &str,
        status: AnalysisStatus,
        stage: u32,
        stage_name: &str,
        pct: f32,
        message: &str,
    ) {
        let update = ProgressUpdate {
            job_id: job_id.into(),
            status,
            current_stage: stage,
            total_stages: 9,
            stage_name: stage_name.into(),
            progress_pct: pct,
            message: message.into(),
            timestamp: Utc::now(),
            detail_json: "{}".into(),
        };
        let _ = tx.send(update.clone()).await;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::*;

    #[tokio::test]
    async fn test_send_progress_does_not_panic() {
        let (tx, mut rx) = mpsc::channel(4);
        AnalysisPipeline::send_progress(
            &tx, "job-1", AnalysisStatus::AnalyzingVisual, 2, "Visual Analysis", 50.0,
            "half way",
        )
        .await;

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.job_id, "job-1");
        assert!(matches!(msg.status, AnalysisStatus::AnalyzingVisual));
        assert!((msg.progress_pct - 50.0).abs() < f32::EPSILON);
    }
}
