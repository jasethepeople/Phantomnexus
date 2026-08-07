use chrono::Utc;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::models::ui::{AnalysisStatus, JobListItem};
use crate::Route;

/// Filter state
#[derive(Clone, PartialEq)]
enum JobFilter {
    All,
    Active,
    Complete,
    Failed,
}

impl JobFilter {
    fn as_str(&self) -> &'static str {
        match self {
            JobFilter::All => "All",
            JobFilter::Active => "Active",
            JobFilter::Complete => "Complete",
            JobFilter::Failed => "Failed",
        }
    }
}

/// Job list page with filterable table
#[function_component(JobList)]
pub fn job_list() -> Html {
    let filter = use_state(|| JobFilter::All);
    let search = use_state(|| String::new());

    // Mock data - in real app, fetch from API
    let jobs = vec![
        JobListItem {
            id: "job_abc123".to_string(),
            video_title: "Gaming Tutorial - Level Up Fast".to_string(),
            status: AnalysisStatus::Complete,
            progress: 100.0,
            risk_score: 15.5,
            violation_count: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        JobListItem {
            id: "job_def456".to_string(),
            video_title: "Music Review - New Album 2024".to_string(),
            status: AnalysisStatus::Analyzing,
            progress: 68.0,
            risk_score: 45.0,
            violation_count: 3,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        JobListItem {
            id: "job_ghi789".to_string(),
            video_title: "Cooking Show - Pasta Recipe".to_string(),
            status: AnalysisStatus::Processing,
            progress: 35.0,
            risk_score: 8.0,
            violation_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        JobListItem {
            id: "job_jkl012".to_string(),
            video_title: "Tech News Weekly Episode 42".to_string(),
            status: AnalysisStatus::Complete,
            progress: 100.0,
            risk_score: 62.3,
            violation_count: 5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        JobListItem {
            id: "job_mno345".to_string(),
            video_title: "Vlog - Trip to Tokyo".to_string(),
            status: AnalysisStatus::Failed,
            progress: 45.0,
            risk_score: 0.0,
            violation_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        JobListItem {
            id: "job_pqr678".to_string(),
            video_title: "Fitness Workout - 30 Min HIIT".to_string(),
            status: AnalysisStatus::Queued,
            progress: 0.0,
            risk_score: 0.0,
            violation_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    ];

    // Filter jobs
    let filtered_jobs: Vec<&JobListItem> = jobs
        .iter()
        .filter(|job| {
            let matches_filter = match **filter {
                JobFilter::Active => matches!(job.status, AnalysisStatus::Queued | AnalysisStatus::Uploading | AnalysisStatus::Processing | AnalysisStatus::Analyzing),
                JobFilter::Complete => matches!(job.status, AnalysisStatus::Complete),
                JobFilter::Failed => matches!(job.status, AnalysisStatus::Failed | AnalysisStatus::Cancelled),
                JobFilter::All => true,
            };
            let matches_search = (*search).is_empty() || job.video_title.to_lowercase().contains(&(*search).to_lowercase());
            matches_filter && matches_search
        })
        .collect();

    let risk_badge_class = |score: f32| -> &'static str {
        match score as u32 {
            s if s < 25 => "badge-low",
            s if s < 50 => "badge-medium",
            s if s < 75 => "badge-high",
            _ => "badge-critical",
        }
    };

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
                        <h1 style="font-size: 1.75rem; margin-bottom: 4px;">
                            {"Analysis Jobs"}
                        </h1>
                        <p style="color: var(--text-muted); font-size: 0.875rem;">
                            {format!("{} jobs total", jobs.len())}
                        </p>
                    </div>
                    <Link<Route> to={Route::Upload} classes="btn btn-primary">
                        {"New Analysis"}
                    </Link<Route>>
                </div>

                // Filters
                <div style="
                    display: flex;
                    align-items: center;
                    gap: 16px;
                    margin-bottom: 24px;
                    flex-wrap: wrap;
                ">
                    // Filter tabs
                    <div style="
                        display: flex;
                        align-items: center;
                        gap: 4px;
                        background: var(--bg-card);
                        border: 1px solid var(--border-subtle);
                        border-radius: var(--radius-md);
                        padding: 4px;
                    ">
                        { vec![JobFilter::All, JobFilter::Active, JobFilter::Complete, JobFilter::Failed]
                            .into_iter()
                            .map(|f| {
                                let is_active = *filter == f;
                                let f_filter = filter.clone();
                                let label = f.as_str();
                                html! {
                                    <button
                                        key={label}
                                        onclick={Callback::from(move |_| f_filter.set(f.clone()))}
                                        style={format!("
                                            padding: 6px 14px;
                                            border-radius: 6px;
                                            font-size: 0.8125rem;
                                            font-weight: 500;
                                            border: none;
                                            cursor: pointer;
                                            transition: all 0.2s ease;
                                            background: {};
                                            color: {};
                                        ", if is_active { "var(--accent)" } else { "transparent" },
                                        if is_active { "#0f0f1a" } else { "var(--text-secondary)" })}
                                    >
                                        {label}
                                    </button>
                                }
                            }).collect::<Html>()
                        }
                    </div>

                    // Search
                    <div style="flex: 1; max-width: 300px;">
                        <input
                            type="text"
                            class="form-input"
                            placeholder="Search jobs..."
                            value={(*search).clone()}
                            oninput={Callback::from(move |e: InputEvent| {
                                let input = e.target_dyn_into::<web_sys::HtmlInputElement>();
                                if let Some(input) = input {
                                    search.set(input.value());
                                }
                            })}
                            style="margin-bottom: 0;"
                        />
                    </div>
                </div>

                // Job table
                <div class="table-container" style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    overflow: hidden;
                ">
                    <table class="table" style="margin: 0;">
                        <thead>
                            <tr>
                                <th>{"Title"}</th>
                                <th>{"Status"}</th>
                                <th>{"Progress"}</th>
                                <th>{"Risk"}</th>
                                <th>{"Violations"}</th>
                                <th>{"Actions"}</th>
                            </tr>
                        </thead>
                        <tbody>
                            if filtered_jobs.is_empty() {
                                <tr>
                                    <td colspan="6" style="text-align: center; padding: 48px;">
                                        <p style="color: var(--text-muted);">
                                            {"No jobs match your filters"}
                                        </p>
                                    </td>
                                </tr>
                            } else {
                                { filtered_jobs.iter().map(|job| {
                                    let job_id = job.id.clone();
                                    let detail_route = Route::JobDetail { id: job.id.clone() };
                                    let report_route = Route::Report { id: job.id.clone() };

                                    html! {
                                        <tr key={job.id.clone()}>
                                            <td>
                                                <div style="
                                                    display: flex;
                                                    align-items: center;
                                                    gap: 10px;
                                                ">
                                                    <div style={format!("
                                                        width: 8px;
                                                        height: 8px;
                                                        border-radius: 50%;
                                                        background: var(--accent);
                                                        opacity: {};
                                                    ", if matches!(job.status, AnalysisStatus::Complete) { "1" } else { "0.3" })} />
                                                    <span style="
                                                        font-weight: 500;
                                                        color: var(--text-primary);
                                                    ">
                                                        {&job.video_title}
                                                    </span>
                                                </div>
                                                <span style="
                                                    font-size: 0.6875rem;
                                                    color: var(--text-muted);
                                                    font-family: 'JetBrains Mono', monospace;
                                                ">
                                                    {&job.id}
                                                </span>
                                            </td>
                                            <td>
                                                <span class={format!("badge {}", job.status.css_class())}>
                                                    {job.status.as_str()}
                                                </span>
                                            </td>
                                            <td>
                                                <div style="width: 120px;">
                                                    <div style="
                                                        display: flex;
                                                        align-items: center;
                                                        gap: 8px;
                                                    ">
                                                        <div class="progress-track" style="flex: 1;">
                                                            <div class="progress-fill" style={format!("width: {}%;", job.progress)} />
                                                        </div>
                                                        <span style="
                                                            font-size: 0.75rem;
                                                            color: var(--text-muted);
                                                            font-family: 'JetBrains Mono', monospace;
                                                            min-width: 32px;
                                                            text-align: right;
                                                        ">
                                                            {format!("{:.0}%", job.progress)}
                                                        </span>
                                                    </div>
                                                </div>
                                            </td>
                                            <td>
                                                if job.risk_score > 0.0 {
                                                    <span class={format!("badge {}", risk_badge_class(job.risk_score))}>
                                                        {format!("{:.1}", job.risk_score)}
                                                    </span>
                                                } else {
                                                    <span style="color: var(--text-muted); font-size: 0.8125rem;">
                                                        {"--"}
                                                    </span>
                                                }
                                            </td>
                                            <td>
                                                if job.violation_count > 0 {
                                                    <span style={format!("
                                                        display: inline-flex;
                                                        align-items: center;
                                                        gap: 4px;
                                                        font-size: 0.8125rem;
                                                        font-weight: 600;
                                                        color: {};
                                                    ", match job.violation_count {
                                                        0 => "var(--text-muted)",
                                                        1..=2 => "var(--risk-medium)",
                                                        3..=5 => "var(--risk-high)",
                                                        _ => "var(--risk-critical)",
                                                    })}>
                                                        {job.violation_count.to_string()}
                                                        <span style="font-weight: 400; color: var(--text-muted);">
                                                            {"found"}
                                                        </span>
                                                    </span>
                                                } else {
                                                    <span style="color: var(--risk-low); font-size: 0.8125rem;">
                                                        {"Clear"}
                                                    </span>
                                                }
                                            </td>
                                            <td>
                                                <div style="
                                                    display: flex;
                                                    align-items: center;
                                                    gap: 8px;
                                                ">
                                                    <Link<Route> to={detail_route.clone()} classes="btn btn-ghost" style="padding: 4px 10px; font-size: 0.75rem;">
                                                        {"View"}
                                                    </Link<Route>>
                                                    if matches!(job.status, AnalysisStatus::Complete) {
                                                        <Link<Route> to={report_route} classes="btn btn-ghost" style="padding: 4px 10px; font-size: 0.75rem;">
                                                            {"Report"}
                                                        </Link<Route>>
                                                    }
                                                </div>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Html>() }
                            }
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}
