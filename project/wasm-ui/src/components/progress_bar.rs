use yew::prelude::*;

/// Pipeline stage definition
#[derive(Properties, PartialEq, Clone)]
pub struct StageDef {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

/// Progress bar props
#[derive(Properties, PartialEq)]
pub struct ProgressBarProps {
    pub progress: f32,
    #[prop_or("Processing...".to_string())]
    pub stage_label: String,
    #[prop_or_default]
    pub stages: Vec<StageDef>,
    #[prop_or(0usize)]
    pub current_stage: usize,
    #[prop_or(false)]
    pub show_shimmer: bool,
    #[prop_or(false)]
    pub compact: bool,
}

/// Animated progress bar with stage labels and shimmer effect
#[function_component(ProgressBar)]
pub fn progress_bar(props: &ProgressBarProps) -> Html {
    let progress = props.progress.clamp(0.0, 100.0);
    let progress_pct = format!("{:.1}%", progress);

    html! {
        <div class={if props.compact { "progress-bar-compact" } else { "progress-bar" }}>
            // Header with label and percentage
            <div style="
                display: flex;
                align-items: center;
                justify-content: space-between;
                margin-bottom: 8px;
            ">
                <span style="
                    font-size: 0.8125rem;
                    font-weight: 500;
                    color: var(--text-secondary);
                ">
                    {&props.stage_label}
                </span>
                <span style="
                    font-size: 0.8125rem;
                    font-weight: 600;
                    color: var(--accent);
                    font-family: 'JetBrains Mono', monospace;
                ">
                    {progress_pct}
                </span>
            </div>

            // Progress track and fill
            <div style="
                width: 100%;
                height: 8px;
                background: var(--bg-secondary);
                border-radius: 4px;
                overflow: hidden;
                position: relative;
            ">
                <div style={format!("
                    height: 100%;
                    width: {}%;
                    background: linear-gradient(90deg, var(--accent), #00a8d4);
                    border-radius: 4px;
                    transition: width 0.6s cubic-bezier(0.4, 0, 0.2, 1);
                    position: relative;
                ", progress)}>
                    // Shimmer overlay
                    if props.show_shimmer {
                        <div style="
                            position: absolute;
                            top: 0;
                            left: 0;
                            right: 0;
                            bottom: 0;
                            background: linear-gradient(90deg, transparent 0%, rgba(255,255,255,0.15) 50%, transparent 100%);
                            background-size: 200% 100%;
                            animation: shimmer 2s infinite;
                            border-radius: 4px;
                        "/>
                    }
                    // Glow at the leading edge
                    <div style={format!("
                        position: absolute;
                        top: -2px;
                        right: -2px;
                        width: 12px;
                        height: 12px;
                        background: var(--accent);
                        border-radius: 50%;
                        box-shadow: 0 0 12px var(--accent-glow);
                        opacity: {};
                    ", if progress < 100.0 { "1" } else { "0" })} />
                </div>
            </div>

            // Stage indicators
            if !props.stages.is_empty() && !props.compact {
                <div style="
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    margin-top: 12px;
                ">
                    { props.stages.iter().enumerate().map(|(i, stage)| {
                        let stage_state = if i < props.current_stage {
                            "complete"
                        } else if i == props.current_stage {
                            "active"
                        } else {
                            "pending"
                        };

                        let (color, bg) = match stage_state {
                            "complete" => ("var(--risk-low)", "var(--risk-low-bg)"),
                            "active" => ("var(--accent)", "var(--accent-dim)"),
                            _ => ("var(--text-muted)", "transparent"),
                        };

                        html! {
                            <div key={stage.id} style="
                                display: flex;
                                flex-direction: column;
                                align-items: center;
                                gap: 4px;
                                flex: 1;
                            ">
                                <div style={format!("
                                    width: 28px;
                                    height: 28px;
                                    border-radius: 50%;
                                    background: {};
                                    border: 2px solid {};
                                    display: flex;
                                    align-items: center;
                                    justify-content: center;
                                    font-size: 0.75rem;
                                    font-weight: 600;
                                    color: {};
                                    transition: all 0.3s ease;
                                ", bg, color, color)}>
                                    if stage_state == "complete" {
                                        {"✓"}
                                    } else {
                                        {stage.icon}
                                    }
                                </div>
                                <span style={format!("
                                    font-size: 0.6875rem;
                                    font-weight: 500;
                                    color: {};
                                    text-align: center;
                                ", if stage_state == "pending" { "var(--text-muted)" } else { "var(--text-secondary)" })}>
                                    {stage.label}
                                </span>
                            </div>
                        }
                    }).collect::<Html>() }
                </div>
            }

            // Compact stage dots
            if !props.stages.is_empty() && props.compact {
                <div style="
                    display: flex;
                    align-items: center;
                    gap: 4px;
                    margin-top: 8px;
                ">
                    { props.stages.iter().enumerate().map(|(i, stage)| {
                        let dot_color = if i < props.current_stage {
                            "var(--risk-low)"
                        } else if i == props.current_stage {
                            "var(--accent)"
                        } else {
                            "var(--border-subtle)"
                        };

                        html! {
                            <div key={stage.id} title={stage.label} style={format!("
                                width: 8px;
                                height: 8px;
                                border-radius: 50%;
                                background: {};
                                transition: all 0.3s ease;
                            ", dot_color)} />
                        }
                    }).collect::<Html>() }
                </div>
            }
        </div>
    }
}
