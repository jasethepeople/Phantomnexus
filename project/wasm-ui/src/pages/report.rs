use chrono::Utc;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::risk_meter::RiskMeter;
use crate::components::timeline::{Timeline, TimelineProps};
use crate::components::violation_card::ViolationCard;
use crate::models::ui::{
    AnalysisReportView, AnalysisStatus, RiskScoreView, SeverityLevel, TimelineEventType,
    TimelineEventView, ViolationType,
};
use crate::Route;

/// Report page props
#[derive(Properties, PartialEq)]
pub struct ReportProps {
    pub id: String,
}

/// Full report page: risk summary cards, violations list, analysis sections, timeline
#[function_component(Report)]
pub fn report(props: &ReportProps) -> Html {
    // Mock report data - in real app, fetch from API
    let report = AnalysisReportView {
        job_id: props.id.clone(),
        video_title: "Gaming Tutorial - Level Up Fast".to_string(),
        video_id: "vid_abc123".to_string(),
        status: AnalysisStatus::Complete,
        risk_score: RiskScoreView::new(45.2, 62.0, 30.0, 15.0),
        violations: vec![
            create_violation(
                "v1",
                ViolationType::Copyright,
                SeverityLevel::High,
                "Audio Copyright",
                "Background music detected that matches copyrighted content in Content ID database.",
                "The audio track at 02:15-03:42 contains a segment that matches 'Song Name' by Artist (Content ID: XYZ123). Claimant: MusicLabel Inc.",
                Some(135.0),
                Some(87.0),
                0.94,
            ),
            create_violation(
                "v2",
                ViolationType::CommunityGuidelines,
                SeverityLevel::Medium,
                "Strong Language",
                "Video contains strong language that may be flagged by automated moderation.",
                "Detected 3 instances of strong language. Most platforms require age-gating or may limit monetization for content with frequent profanity.",
                Some(420.0),
                None,
                0.78,
            ),
            create_violation(
                "v3",
                ViolationType::Monetization,
                SeverityLevel::Low,
                "Thumbnail Text",
                "Thumbnail text may not be clearly readable on smaller screens.",
                "The thumbnail contains text smaller than 12pt which may not be readable on mobile devices. Consider increasing font size for better engagement.",
                None,
                None,
                0.65,
            ),
            create_violation(
                "v4",
                ViolationType::Copyright,
                SeverityLevel::Medium,
                "Gameplay Footage",
                "Game publisher may claim revenue from gameplay footage.",
                "The game publisher 'StudioX' has a history of claiming revenue from Let's Play videos. This is typically allowed but may result in revenue sharing.",
                Some(0.0),
                Some(1800.0),
                0.82,
            ),
            create_violation(
                "v5",
                ViolationType::CommunityGuidelines,
                SeverityLevel::Low,
                "External Links",
                "Description contains external links that may violate platform policies.",
                "Multiple external links detected in description. Ensure all links comply with platform's external links policy. Some links redirect to potentially unsafe domains.",
                None,
                None,
                0.55,
            ),
        ],
        timeline: vec![
            TimelineEventView {
                id: "t1".to_string(),
                event_type: TimelineEventType::Milestone,
                label: "Upload".to_string(),
                description: "Video uploaded successfully".to_string(),
                timestamp: 0.0,
                severity: None,
                metadata: serde_json::json!({}),
            },
            TimelineEventView {
                id: "t2".to_string(),
                event_type: TimelineEventType::Info,
                label: "Transcription".to_string(),
                description: "Audio transcription completed".to_string(),
                timestamp: 45.0,
                severity: None,
                metadata: serde_json::json!({}),
            },
            TimelineEventView {
                id: "t3".to_string(),
                event_type: TimelineEventType::Warning,
                label: "Language Detected".to_string(),
                description: "Strong language found in transcript".to_string(),
                timestamp: 120.0,
                severity: Some(SeverityLevel::Medium),
                metadata: serde_json::json!({}),
            },
            TimelineEventView {
                id: "t4".to_string(),
                event_type: TimelineEventType::Violation,
                label: "Copyright Match".to_string(),
                description: "Audio matches Content ID database".to_string(),
                timestamp: 135.0,
                severity: Some(SeverityLevel::High),
                metadata: serde_json::json!({}),
            },
            TimelineEventView {
                id: "t5".to_string(),
                event_type: TimelineEventType::Violation,
                label: "Gameplay Claim".to_string(),
                description: "Publisher may claim gameplay footage".to_string(),
                timestamp: 600.0,
                severity: Some(SeverityLevel::Medium),
                metadata: serde_json::json!({}),
            },
            TimelineEventView {
                id: "t6".to_string(),
                event_type: TimelineEventType::Warning,
                label: "Thumbnail Check".to_string(),
                description: "Thumbnail text size may be too small".to_string(),
                timestamp: 1200.0,
                severity: Some(SeverityLevel::Low),
                metadata: serde_json::json!({}),
            },
            TimelineEventView {
                id: "t7".to_string(),
                event_type: TimelineEventType::Milestone,
                label: "Complete".to_string(),
                description: "Analysis completed".to_string(),
                timestamp: 1800.0,
                severity: None,
                metadata: serde_json::json!({}),
            },
        ],
        created_at: Utc::now(),
        completed_at: Some(Utc::now()),
        metadata: Default::default(),
    };

    let risk = &report.risk_score;

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
                            <h1 style="font-size: 1.75rem;">{"Analysis Report"}</h1>
                            <span class={format!("badge {}", report.status.css_class())}>
                                {report.status.as_str()}
                            </span>
                        </div>
                        <p style="color: var(--text-secondary); font-size: 0.875rem;">
                            {&report.video_title}
                        </p>
                        <p style="
                            font-family: 'JetBrains Mono', monospace;
                            font-size: 0.75rem;
                            color: var(--text-muted);
                        ">
                            {format!("Job: {}", report.job_id)}
                        </p>
                    </div>
                    <Link<Route> to={Route::AutoFix { id: props.id.clone() }} classes="btn btn-primary">
                        {"Fix Issues"}
                    </Link<Route>>
                </div>

                // Risk Summary Cards (3 columns)
                <div style="
                    display: grid;
                    grid-template-columns: repeat(3, 1fr);
                    gap: 16px;
                    margin-bottom: 24px;
                ">
                    // Overall Risk Card
                    <div class="card" style="
                        text-align: center;
                        display: flex;
                        flex-direction: column;
                        align-items: center;
                    ">
                        <h3 style="
                            font-size: 0.6875rem;
                            font-weight: 600;
                            text-transform: uppercase;
                            letter-spacing: 0.05em;
                            color: var(--text-muted);
                            margin-bottom: 16px;
                        ">
                            {"Overall Risk"}
                        </h3>
                        <RiskMeter score={risk.overall} size={140} animated=true show_label=true />
                        <span style={format!("
                            margin-top: 8px;
                            padding: 4px 12px;
                            border-radius: 4px;
                            font-size: 0.75rem;
                            font-weight: 600;
                            background: {};
                            color: {};
                        ", risk.css_bg(), risk.css_color())}>
                            {&risk.overall_label}
                        </span>
                    </div>

                    // Category Breakdown Card
                    <div class="card">
                        <h3 style="
                            font-size: 0.6875rem;
                            font-weight: 600;
                            text-transform: uppercase;
                            letter-spacing: 0.05em;
                            color: var(--text-muted);
                            margin-bottom: 20px;
                        ">
                            {"Risk Breakdown"}
                        </h3>
                        <div style="display: flex; flex-direction: column; gap: 16px;">
                            { vec![
                                ("Copyright", risk.copyright, RiskScoreView::risk_level(risk.copyright).1),
                                ("Community Guidelines", risk.community_guidelines, RiskScoreView::risk_level(risk.community_guidelines).1),
                                ("Monetization", risk.monetization, RiskScoreView::risk_level(risk.monetization).1),
                            ].into_iter().map(|(label, score, color)| {
                                html! {
                                    <div key={label}>
                                        <div style="
                                            display: flex;
                                            justify-content: space-between;
                                            align-items: center;
                                            margin-bottom: 6px;
                                        ">
                                            <span style="font-size: 0.8125rem; color: var(--text-secondary);">
                                                {label}
                                            </span>
                                            <span style={format!("
                                                font-size: 0.75rem;
                                                font-weight: 600;
                                                font-family: 'JetBrains Mono', monospace;
                                                color: {};
                                            ", color)}>
                                                {format!("{:.1}", score)}
                                            </span>
                                        </div>
                                        <div style="
                                            width: 100%;
                                            height: 6px;
                                            background: var(--bg-secondary);
                                            border-radius: 3px;
                                            overflow: hidden;
                                        ">
                                            <div style={format!("
                                                height: 100%;
                                                width: {}%;
                                                background: {};
                                                border-radius: 3px;
                                                transition: width 0.8s ease;
                                            ", score, color)} />
                                        </div>
                                    </div>
                                }
                            }).collect::<Html>() }
                        </div>
                    </div>

                    // Summary Stats Card
                    <div class="card">
                        <h3 style="
                            font-size: 0.6875rem;
                            font-weight: 600;
                            text-transform: uppercase;
                            letter-spacing: 0.05em;
                            color: var(--text-muted);
                            margin-bottom: 20px;
                        ">
                            {"Summary"}
                        </h3>
                        <div style="display: flex; flex-direction: column; gap: 16px;">
                            { vec![
                                ("Violations Found", report.violations.len().to_string(), "var(--risk-critical)", report.violations.len() > 0),
                                ("Auto-Fixable", report.violations.iter().filter(|v| v.suggested_fix.is_some()).count().to_string(), "var(--accent)", true),
                                ("Analysis Time", "2m 34s", "var(--text-secondary)", true),
                                ("Confidence", "94%", "var(--risk-low)", true),
                            ].into_iter().map(|(label, value, color, _show)| {
                                html! {
                                    <div key={label} style="
                                        display: flex;
                                        align-items: center;
                                        justify-content: space-between;
                                        padding: 12px;
                                        background: var(--bg-secondary);
                                        border-radius: var(--radius-sm);
                                    ">
                                        <span style="font-size: 0.8125rem; color: var(--text-secondary);">
                                            {label}
                                        </span>
                                        <span style={format!("
                                            font-size: 0.875rem;
                                            font-weight: 700;
                                            font-family: 'JetBrains Mono', monospace;
                                            color: {};
                                        ", color)}>
                                            {value}
                                        </span>
                                    </div>
                                }
                            }).collect::<Html>() }
                        </div>
                    </div>
                </div>

                // Violations Section
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                    margin-bottom: 24px;
                ">
                    <div style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        margin-bottom: 20px;
                    ">
                        <h2 style="font-size: 1.125rem; font-weight: 600; color: var(--text-primary);">
                            {format!("Violations ({})", report.violations.len())}
                        </h2>
                        <div style="display: flex; gap: 8px;">
                            { vec![
                                ("All", report.violations.len()),
                                ("Critical", report.violations.iter().filter(|v| matches!(v.severity, SeverityLevel::Critical)).count()),
                                ("High", report.violations.iter().filter(|v| matches!(v.severity, SeverityLevel::High)).count()),
                                ("Medium", report.violations.iter().filter(|v| matches!(v.severity, SeverityLevel::Medium)).count()),
                                ("Low", report.violations.iter().filter(|v| matches!(v.severity, SeverityLevel::Low)).count()),
                            ].into_iter().map(|(label, count)| {
                                html! {
                                    <span key={label} style="
                                        font-size: 0.6875rem;
                                        padding: 2px 8px;
                                        border-radius: 4px;
                                        background: var(--bg-tertiary);
                                        color: var(--text-muted);
                                    ">
                                        {format!("{} {}", label, count)}
                                    </span>
                                }
                            }).collect::<Html>() }
                        </div>
                    </div>

                    <div style="display: flex; flex-direction: column; gap: 12px;">
                        { report.violations.iter().map(|v| {
                            html! {
                                <ViolationCard
                                    key={v.id.clone()}
                                    id={v.id.clone()}
                                    violation_type={v.violation_type.clone()}
                                    severity={v.severity.clone()}
                                    category={v.category.clone()}
                                    description={v.description.clone()}
                                    details={v.details.clone()}
                                    timestamp={v.timestamp}
                                    confidence={v.confidence}
                                    show_fix_button=true
                                />
                            }
                        }).collect::<Html>() }
                    </div>
                </div>

                // Timeline Section
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                    margin-bottom: 24px;
                ">
                    <h2 style="font-size: 1.125rem; font-weight: 600; color: var(--text-primary); margin-bottom: 20px;">
                        {"Analysis Timeline"}
                    </h2>
                    <Timeline events={report.timeline.clone()} height={300} interactive=true />
                </div>

                // Analysis Details
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                ">
                    <h2 style="font-size: 1.125rem; font-weight: 600; color: var(--text-primary); margin-bottom: 20px;">
                        {"Analysis Details"}
                    </h2>
                    <div style="
                        display: grid;
                        grid-template-columns: repeat(2, 1fr);
                        gap: 16px;
                    ">
                        { vec![
                            ("Analysis Engine", "Sentinel v2.1.0", "AI model version used for analysis"),
                            ("Content ID Database", "2024-01-15", "Last updated"),
                            ("Audio Analysis", "Whisper v3 + Custom", "Transcription and audio fingerprinting"),
                            ("Video Analysis", "Frame-by-frame + OCR", "Visual content scanning"),
                            ("Metadata Check", "Platform Policies v4.2", "Title, description, tags validation"),
                            ("Processing Time", "2m 34s", "Total wall-clock time"),
                        ].into_iter().map(|(label, value, desc)| {
                            html! {
                                <div key={label} style="
                                    padding: 16px;
                                    background: var(--bg-secondary);
                                    border-radius: var(--radius-sm);
                                ">
                                    <div style="
                                        font-size: 0.6875rem;
                                        font-weight: 600;
                                        text-transform: uppercase;
                                        letter-spacing: 0.05em;
                                        color: var(--text-muted);
                                        margin-bottom: 8px;
                                    ">
                                        {label}
                                    </div>
                                    <div style="
                                        font-size: 0.875rem;
                                        font-weight: 600;
                                        color: var(--text-primary);
                                        margin-bottom: 4px;
                                    ">
                                        {value}
                                    </div>
                                    <div style="
                                        font-size: 0.75rem;
                                        color: var(--text-muted);
                                    ">
                                        {desc}
                                    </div>
                                </div>
                            }
                        }).collect::<Html>() }
                    </div>
                </div>
            </div>
        </div>
    }
}

fn create_violation(
    id: &str,
    violation_type: ViolationType,
    severity: SeverityLevel,
    category: &str,
    description: &str,
    details: &str,
    timestamp: Option<f64>,
    duration: Option<f64>,
    confidence: f32,
) -> crate::models::ui::ViolationView {
    crate::models::ui::ViolationView {
        id: id.to_string(),
        violation_type,
        severity,
        category: category.to_string(),
        description: description.to_string(),
        details: details.to_string(),
        timestamp,
        duration,
        confidence,
        suggested_fix: None,
        created_at: Utc::now(),
    }
}
