use yew::prelude::*;

use crate::models::ui::{SeverityLevel, ViolationType};

/// Violation card props
#[derive(Properties, PartialEq)]
pub struct ViolationCardProps {
    pub id: String,
    pub violation_type: ViolationType,
    pub severity: SeverityLevel,
    pub category: String,
    pub description: String,
    pub details: String,
    pub timestamp: Option<f64>,
    pub confidence: f32,
    pub show_fix_button: bool,
    #[prop_or_default]
    pub on_fix: Option<Callback<String>>,
}

/// Violation card with severity badge, timestamp, and description
#[function_component(ViolationCard)]
pub fn violation_card(props: &ViolationCardProps) -> Html {
    let violation_type = props.violation_type.clone();
    let severity = props.severity.clone();
    let is_expanded = use_state(|| false);
    let id = props.id.clone();

    let toggle_expand = {
        let is_expanded = is_expanded.clone();
        Callback::from(move |_| {
            is_expanded.set(!*is_expanded);
        })
    };

    let on_fix_click = {
        let id = id.clone();
        let on_fix = props.on_fix.clone();
        Callback::from(move |_| {
            if let Some(ref cb) = on_fix {
                cb.emit(id.clone());
            }
        })
    };

    let severity_color = match severity {
        SeverityLevel::Low => "#4ade80",
        SeverityLevel::Medium => "#facc15",
        SeverityLevel::High => "#fb923c",
        SeverityLevel::Critical => "#ef4444",
    };

    let type_icon = match violation_type {
        ViolationType::Copyright => "©",
        ViolationType::CommunityGuidelines => "🛡",
        ViolationType::Monetization => "$",
        ViolationType::ContentId => "🔍",
        ViolationType::AgeRestriction => "18+",
        ViolationType::Misinformation => "⚠",
        ViolationType::HateSpeech => "!",
        ViolationType::Harassment => "✕",
        ViolationType::Spam => "⚡",
        ViolationType::Other => "ℹ",
    };

    let timestamp_display = props.timestamp.map(|ts| {
        let mins = (ts / 60.0).floor() as u32;
        let secs = (ts % 60.0) as u32;
        format!("{:02}:{:02}", mins, secs)
    });

    html! {
        <div style={format!("
            background: var(--bg-card);
            border: 1px solid var(--border-subtle);
            border-left: 3px solid {};
            border-radius: var(--radius-md);
            padding: 16px;
            transition: all 0.2s ease;
            cursor: pointer;
        ", severity_color)} onclick={toggle_expand}>
            // Header row
            <div style="
                display: flex;
                align-items: center;
                gap: 12px;
            ">
                // Type icon
                <div style={format!("
                    width: 36px;
                    height: 36px;
                    border-radius: var(--radius-sm);
                    background: {}20;
                    color: {};
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    font-size: 0.875rem;
                    font-weight: 700;
                    flex-shrink: 0;
                ", severity_color, severity_color)}>
                    {type_icon}
                </div>

                // Content
                <div style="flex: 1; min-width: 0;">
                    <div style="
                        display: flex;
                        align-items: center;
                        gap: 8px;
                        margin-bottom: 2px;
                    ">
                        <span style="
                            font-size: 0.875rem;
                            font-weight: 600;
                            color: var(--text-primary);
                        ">
                            {&props.category}
                        </span>
                        <span class={format!("badge {}", severity.css_class())}>
                            {severity.as_str()}
                        </span>
                        <span class={format!("badge {}", match violation_type {
                            ViolationType::Copyright => "badge-accent",
                            ViolationType::CommunityGuidelines => "badge-medium",
                            ViolationType::Monetization => "badge-low",
                            _ => "badge-high",
                        })}>
                            {violation_type.as_str()}
                        </span>
                    </div>
                    <p style="
                        font-size: 0.8125rem;
                        color: var(--text-secondary);
                        margin: 0;
                        overflow: hidden;
                        text-overflow: ellipsis;
                        white-space: nowrap;
                    ">
                        {&props.description}
                    </p>
                </div>

                // Right side info
                <div style="
                    display: flex;
                    align-items: center;
                    gap: 12px;
                    flex-shrink: 0;
                ">
                    if let Some(ref ts) = timestamp_display {
                        <span style="
                            font-family: 'JetBrains Mono', monospace;
                            font-size: 0.75rem;
                            color: var(--text-muted);
                            background: var(--bg-tertiary);
                            padding: 2px 8px;
                            border-radius: 4px;
                        ">
                            {ts}
                        </span>
                    }
                    <span style={format!("
                        font-size: 0.75rem;
                        font-weight: 600;
                        color: {};
                    ", severity_color)}>
                        {format!("{:.0}%", props.confidence * 100.0)}
                    </span>
                </div>
            </div>

            // Expanded details
            if *is_expanded {
                <div style="
                    margin-top: 12px;
                    padding-top: 12px;
                    border-top: 1px solid var(--border-subtle);
                    animation: fadeIn 0.2s ease forwards;
                ">
                    <p style="
                        font-size: 0.8125rem;
                        color: var(--text-secondary);
                        line-height: 1.6;
                        margin-bottom: 12px;
                    ">
                        {&props.details}
                    </p>

                    <div style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                    ">
                        // Confidence bar
                        <div style="
                            display: flex;
                            align-items: center;
                            gap: 8px;
                            flex: 1;
                        ">
                            <span style="
                                font-size: 0.75rem;
                                color: var(--text-muted);
                            ">
                                {"Confidence"}
                            </span>
                            <div style="
                                flex: 1;
                                max-width: 200px;
                                height: 4px;
                                background: var(--bg-tertiary);
                                border-radius: 2px;
                                overflow: hidden;
                            ">
                                <div style={format!("
                                    height: 100%;
                                    width: {:.0}%;
                                    background: {};
                                    border-radius: 2px;
                                    transition: width 0.5s ease;
                                ", props.confidence * 100.0, severity_color)} />
                            </div>
                        </div>

                        // Fix button
                        if props.show_fix_button {
                            <button
                                class="btn btn-primary"
                                style="
                                    padding: 4px 12px;
                                    font-size: 0.75rem;
                                "
                                onclick={on_fix_click}
                            >
                                {"Fix Issue"}
                            </button>
                        }
                    </div>
                </div>
            }
        </div>
    }
}
