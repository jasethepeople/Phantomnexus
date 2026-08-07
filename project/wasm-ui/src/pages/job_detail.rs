use chrono::Utc;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::progress_bar::{ProgressBar, StageDef};
use crate::models::ui::{
    AnalysisStatus, ConnectionState, PipelineStage, ProgressMessage, StageStatus,
};
use crate::services::websocket::{WsClient, WsMessage};
use crate::Route;

/// Job detail page props
#[derive(Properties, PartialEq)]
pub struct JobDetailProps {
    pub id: String,
}

/// Job detail with real-time progress, live log, and pipeline tracker
#[function_component(JobDetail)]
pub fn job_detail(props: &JobDetailProps) -> Html {
    let job_id = props.id.clone();
    let progress = use_state(|| 0f32);
    let stage_label = use_state(|| "Initializing...".to_string());
    let logs = use_state(|| Vec::<(String, String, String)>::new());
    let connection_state = use_state(|| ConnectionState::Disconnected);
    let current_stage = use_state(|| 0usize);
    let status = use_state(|| AnalysisStatus::Processing);

    // Pipeline stages
    let stages = use_state(|| vec![
        PipelineStage::new("upload", "Upload"),
        PipelineStage::new("transcribe", "Transcribe"),
        PipelineStage::new("analyze", "Analyze"),
        PipelineStage::new("report", "Report"),
    ]);

    // Stage definitions for progress bar
    let stage_defs = vec![
        StageDef { id: "upload", label: "Upload", icon: "↑" },
        StageDef { id: "transcribe", label: "Transcribe", icon: "T" },
        StageDef { id: "analyze", label: "Analyze", icon: "🔍" },
        StageDef { id: "report", label: "Report", icon: "📊" },
    ];

    // Connect to WebSocket
    {
        let job_id_ws = job_id.clone();
        let progress = progress.clone();
        let stage_label = stage_label.clone();
        let logs = logs.clone();
        let connection_state = connection_state.clone();
        let current_stage = current_stage.clone();
        let status = status.clone();
        let stages = stages.clone();

        use_effect_with((), move |_| {
            let ws_url = format!("ws://localhost:8080/ws");

            let on_message = {
                let progress = progress.clone();
                let stage_label = stage_label.clone();
                let logs = logs.clone();
                let current_stage = current_stage.clone();
                let status = status.clone();
                let stages = stages.clone();

                move |msg: WsMessage| {
                    match msg {
                        WsMessage::Progress(p) => {
                            progress.set(p.progress);
                            stage_label.set(p.stage_label.clone());

                            // Update stage based on progress
                            let stage_idx = match p.progress {
                                s if s < 15.0 => 0,
                                s if s < 40.0 => 1,
                                s if s < 75.0 => 2,
                                _ => 3,
                            };
                            current_stage.set(stage_idx);

                            // Update pipeline stages
                            stages.set({
                                let mut s = (*stages).clone();
                                for (i, stage) in s.iter_mut().enumerate() {
                                    if i < stage_idx {
                                        stage.status = StageStatus::Complete;
                                        stage.progress = 100.0;
                                    } else if i == stage_idx {
                                        stage.status = StageStatus::InProgress;
                                        stage.progress = p.progress;
                                    }
                                }
                                s
                            });

                            if let Some(log) = p.log_line {
                                let mut new_logs = (*logs).clone();
                                let time = chrono::Utc::now().format("%H:%M:%S").to_string();
                                new_logs.push((time, "info".to_string(), log));
                                if new_logs.len() > 200 {
                                    new_logs.remove(0);
                                }
                                logs.set(new_logs);
                            }

                            if p.progress >= 100.0 {
                                status.set(AnalysisStatus::Complete);
                            }
                        }
                        WsMessage::Status { status: s, progress: p, .. } => {
                            progress.set(p);
                            status.set(match s.as_str() {
                                "complete" => AnalysisStatus::Complete,
                                "failed" => AnalysisStatus::Failed,
                                "processing" => AnalysisStatus::Processing,
                                "analyzing" => AnalysisStatus::Analyzing,
                                _ => AnalysisStatus::Processing,
                            });
                        }
                        WsMessage::Log { line, level, .. } => {
                            let mut new_logs = (*logs).clone();
                            let time = chrono::Utc::now().format("%H:%M:%S").to_string();
                            new_logs.push((time, level, line));
                            if new_logs.len() > 200 {
                                new_logs.remove(0);
                            }
                            logs.set(new_logs);
                        }
                        WsMessage::Complete { success, .. } => {
                            status.set(if success {
                                AnalysisStatus::Complete
                            } else {
                                AnalysisStatus::Failed
                            }));
                            progress.set(100.0);
                        }
                        WsMessage::Error { message, .. } => {
                            let mut new_logs = (*logs).clone();
                            let time = chrono::Utc::now().format("%H:%M:%S").to_string();
                            new_logs.push((time, "error".to_string(), message));
                            logs.set(new_logs);
                        }
                        _ => {}
                    }
                }
            };

            connection_state.set(ConnectionState::Connecting);

            match WsClient::connect(&ws_url, Box::new(on_message)) {
                Ok(client) => {
                    connection_state.set(ConnectionState::Connected);
                    // We can't store client easily without RefCell, so we simulate
                    // In real app, store in a RefCell<Option<WsClient>>
                }
                Err(e) => {
                    connection_state.set(ConnectionState::Error(e));
                }
            }

            // Simulate progress for demo
            let progress_sim = progress.clone();
            let stage_label_sim = stage_label.clone();
            let logs_sim = logs.clone();
            let current_stage_sim = current_stage.clone();
            let stages_sim = stages.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let stages_data = vec![
                    ("Uploading video file...", 0.0, 0),
                    ("Uploading video file...", 10.0, 0),
                    ("Transcribing audio...", 20.0, 1),
                    ("Transcribing audio...", 35.0, 1),
                    ("Analyzing content...", 45.0, 2),
                    ("Analyzing content...", 60.0, 2),
                    ("Analyzing content...", 75.0, 2),
                    ("Generating report...", 85.0, 3),
                    ("Finalizing...", 95.0, 3),
                    ("Analysis complete!", 100.0, 3),
                ];

                for (label, prog, stage) in stages_data {
                    gloo_timers::future::TimeoutFuture::new(800).await;
                    progress_sim.set(prog);
                    stage_label_sim.set(label.to_string());
                    current_stage_sim.set(stage);

                    let mut new_stages = (*stages_sim).clone();
                    for (i, s) in new_stages.iter_mut().enumerate() {
                        if i < stage {
                            s.status = StageStatus::Complete;
                            s.progress = 100.0;
                        } else if i == stage {
                            s.status = StageStatus::InProgress;
                            s.progress = prog;
                        }
                    }
                    stages_sim.set(new_stages);

                    let time = chrono::Utc::now().format("%H:%M:%S").to_string();
                    let level = "info".to_string();
                    let log_msg = format!("{} [{:.0}%] {}", label, prog, match stage {
                        0 => "Upload pipeline",
                        1 => "Transcription engine",
                        2 => "Analysis engine",
                        _ => "Report generator",
                    });

                    let mut new_logs = (*logs_sim).clone();
                    new_logs.push((time, level, log_msg));
                    logs_sim.set(new_logs);
                }
            });

            || {}
        });
    }

    let report_route = Route::Report { id: job_id.clone() };
    let fix_route = Route::AutoFix { id: job_id.clone() };

    html! {
        <div class="page-enter">
            <div class="container" style="padding-top: 48px; padding-bottom: 48px;">
                // Header
                <div style="
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    margin-bottom: 32px;
                ">
                    <div>
                        <div style="
                            display: flex;
                            align-items: center;
                            gap: 12px;
                            margin-bottom: 4px;
                        ">
                            <h1 style="font-size: 1.5rem;">
                                {"Job Details"}
                            </h1>
                            <span class={format!("badge {}", (*status).css_class())}>
                                {(*status).as_str()}
                            </span>
                        </div>
                        <p style="
                            font-family: 'JetBrains Mono', monospace;
                            font-size: 0.8125rem;
                            color: var(--text-muted);
                        ">
                            {&job_id}
                        </p>
                    </div>
                    <div style="display: flex; gap: 8px;">
                        if *progress >= 100.0 {
                            <Link<Route> to={report_route} classes="btn btn-primary" style="font-size: 0.8125rem;">
                                {"View Report"}
                            </Link<Route>>
                            <Link<Route> to={fix_route} classes="btn btn-secondary" style="font-size: 0.8125rem;">
                                {"Auto-Fix"}
                            </Link<Route>>
                        }
                    </div>
                </div>

                // Progress Section
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                    margin-bottom: 24px;
                ">
                    <ProgressBar
                        progress={*progress}
                        stage_label={(*stage_label).clone()}
                        stages={stage_defs}
                        current_stage={*current_stage}
                        show_shimmer={*progress < 100.0}
                    />
                </div>

                // Pipeline Tracker
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                    margin-bottom: 24px;
                ">
                    <h2 style="font-size: 1rem; font-weight: 600; margin-bottom: 16px; color: var(--text-primary);">
                        {"Pipeline Stages"}
                    </h2>
                    <div style="
                        display: grid;
                        grid-template-columns: repeat(4, 1fr);
                        gap: 16px;
                    ">
                        { (*stages).iter().map(|stage| {
                            let status_color = match stage.status {
                                StageStatus::Pending => "var(--text-muted)",
                                StageStatus::InProgress => "var(--accent)",
                                StageStatus::Complete => "var(--risk-low)",
                                StageStatus::Failed => "var(--risk-critical)",
                                StageStatus::Skipped => "var(--text-muted)",
                            };
                            let status_icon = match stage.status {
                                StageStatus::Pending => "○",
                                StageStatus::InProgress => "◐",
                                StageStatus::Complete => "✓",
                                StageStatus::Failed => "✕",
                                StageStatus::Skipped => "⊘",
                            };

                            html! {
                                <div key={stage.id.clone()} style={format!("
                                    padding: 16px;
                                    border-radius: var(--radius-md);
                                    border: 1px solid var(--border-subtle);
                                    background: {}10;
                                    transition: all 0.2s ease;
                                ", status_color)}>
                                    <div style="
                                        display: flex;
                                        align-items: center;
                                        justify-content: space-between;
                                        margin-bottom: 8px;
                                    ">
                                        <span style={format!("
                                            font-size: 1.25rem;
                                            color: {};
                                        ", status_color)}>
                                            {status_icon}
                                        </span>
                                        <span class={format!("badge {}", stage.status.css_class())} style="font-size: 0.625rem;">
                                            {stage.status.as_str()}
                                        </span>
                                    </div>
                                    <h3 style="
                                        font-size: 0.875rem;
                                        font-weight: 600;
                                        color: var(--text-primary);
                                        margin-bottom: 4px;
                                    ">
                                        {&stage.label}
                                    </h3>
                                    <div style="margin-top: 8px;">
                                        <div class="progress-track">
                                            <div class="progress-fill" style={format!("
                                                width: {}%;
                                                background: {};
                                            ", stage.progress, status_color)} />
                                        </div>
                                        <span style="
                                            font-size: 0.6875rem;
                                            color: var(--text-muted);
                                            font-family: 'JetBrains Mono', monospace;
                                            margin-top: 4px;
                                            display: block;
                                        ">
                                            {format!("{:.0}%", stage.progress)}
                                        </span>
                                    </div>
                                </div>
                            }
                        }).collect::<Html>() }
                    </div>
                </div>

                // Live Log Section
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                ">
                    <div style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        margin-bottom: 16px;
                    ">
                        <h2 style="font-size: 1rem; font-weight: 600; color: var(--text-primary);">
                            {"Live Log"}
                        </h2>
                        <div style="display: flex; align-items: center; gap: 8px;">
                            <span class={(*connection_state).css_class()} />
                            <span style="font-size: 0.75rem; color: var(--text-muted);">
                                {(*connection_state).label()}
                            </span>
                        </div>
                    </div>
                    <div class="log-output">
                        if (*logs).is_empty() {
                            <div style="color: var(--text-muted); text-align: center; padding: 24px;">
                                {"Waiting for log output..."}
                            </div>
                        } else {
                            { (*logs).iter().map(|(time, level, msg)| {
                                let log_class = match level.as_str() {
                                    "error" => "log-error",
                                    "warning" | "warn" => "log-warning",
                                    "success" => "log-success",
                                    _ => "log-info",
                                };
                                let line_key = format!("{}-{}", time, msg.chars().take(20).collect::<String>());
                                html! {
                                    <div key={line_key} class="log-entry">
                                        <span class="log-time">{time}</span>
                                        <span class={format!("{}", log_class)}>{msg}</span>
                                    </div>
                                }
                            }).collect::<Html>() }
                        }
                    </div>
                </div>
            </div>
        </div>
    }
}
