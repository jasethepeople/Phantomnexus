use yew::prelude::*;

/// Risk meter props
#[derive(Properties, PartialEq)]
pub struct RiskMeterProps {
    pub score: f32,
    #[prop_or(120)]
    pub size: u32,
    #[prop_or(false)]
    pub animated: bool,
    #[prop_or(false)]
    pub show_label: bool,
    #[prop_or_default]
    pub label: Option<String>,
}

/// SVG circular risk gauge with color-coded segments
#[function_component(RiskMeter)]
pub fn risk_meter(props: &RiskMeterProps) -> Html {
    let score = props.score.clamp(0.0, 100.0);
    let size = props.size;
    let stroke_width = (size as f32 / 12.0).max(6.0);
    let radius = (size as f32 - stroke_width) / 2.0;
    let center = size as f32 / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius as f64;

    // Calculate the arc length for the score (270 degrees = 75% of circle)
    let arc_fraction = 0.75;
    let score_fraction = score as f64 / 100.0 * arc_fraction;
    let dash_array = format!("{} {}", score_fraction * circumference, circumference);

    // Color based on score
    let color = get_risk_color(score);
    let color_hex = match score as u32 {
        s if s < 25 => "#4ade80",
        s if s < 50 => "#facc15",
        s if s < 75 => "#fb923c",
        _ => "#ef4444",
    };

    // Rotation to start from bottom-left
    let rotation = 135.0;

    // Background arc (gray track)
    let bg_dash = format!("{} {}", arc_fraction * circumference, circumference);

    html! {
        <div style="display: flex; flex-direction: column; align-items: center; gap: 8px;">
            <svg
                width={size.to_string()}
                height={size.to_string()}
                viewBox={format!("0 0 {} {}", size, size)}
                style={if props.animated { "animation: fadeIn 0.8s ease forwards;" } else { "" }}
            >
                <defs>
                    <linearGradient id={format!("risk-grad-{}", size)} x1="0%" y1="0%" x2="100%" y2="100%">
                        <stop offset="0%" stop-color={color_hex} />
                        <stop offset="100%" stop-color={color_hex} stop-opacity="0.6" />
                    </linearGradient>
                    <filter id={format!("glow-{}", size)}>
                        <feGaussianBlur stdDeviation="3" result="coloredBlur"/>
                        <feMerge>
                            <feMergeNode in="coloredBlur"/>
                            <feMergeNode in="SourceGraphic"/>
                        </feMerge>
                    </filter>
                </defs>

                // Background track
                <circle
                    cx={center.to_string()}
                    cy={center.to_string()}
                    r={radius.to_string()}
                    fill="none"
                    stroke="var(--bg-tertiary)"
                    stroke-width={stroke_width.to_string()}
                    stroke-dasharray={bg_dash.clone()}
                    stroke-linecap="round"
                    transform={format!("rotate({}, {}, {})", rotation, center, center)}
                />

                // Risk arc
                <circle
                    cx={center.to_string()}
                    cy={center.to_string()}
                    r={radius.to_string()}
                    fill="none"
                    stroke={format!("url(#risk-grad-{})", size)}
                    stroke-width={stroke_width.to_string()}
                    stroke-dasharray={dash_array}
                    stroke-linecap="round"
                    transform={format!("rotate({}, {}, {})", rotation, center, center)}
                    style="transition: stroke-dasharray 1s ease;"
                    filter={format!("url(#glow-{})", size)}
                />

                // Center dot
                <circle
                    cx={center.to_string()}
                    cy={center.to_string()}
                    r={(stroke_width / 3.0).to_string()}
                    fill={color_hex}
                />

                // Score text
                <text
                    x={center.to_string()}
                    y={format!("{}", center - 4.0)}
                    text-anchor="middle"
                    dominant-baseline="middle"
                    style={format!("
                        font-family: 'JetBrains Mono', monospace;
                        font-size: {}px;
                        font-weight: 700;
                        fill: {};
                    ", size as f32 * 0.2, color_hex)}
                >
                    {format!("{:.0}", score)}
                </text>

                // Percent label
                <text
                    x={center.to_string()}
                    y={format!("{}", center + 10.0)}
                    text-anchor="middle"
                    dominant-baseline="middle"
                    style={format!("
                        font-family: 'Inter', sans-serif;
                        font-size: {}px;
                        font-weight: 500;
                        fill: var(--text-muted);
                    ", size as f32 * 0.08)}
                >
                    {"%"}
                </text>

                // Risk label at bottom
                if props.show_label {
                    <text
                        x={center.to_string()}
                        y={format!("{}", size as f32 - 8.0)}
                        text-anchor="middle"
                        dominant-baseline="middle"
                        style={format!("
                            font-family: 'Inter', sans-serif;
                            font-size: {}px;
                            font-weight: 600;
                            fill: {};
                        ", size as f32 * 0.09, color_hex)}
                    >
                        {get_risk_label(score)}
                    </text>
                }
            </svg>

            if let Some(ref label_text) = props.label {
                <span style="
                    font-size: 0.8125rem;
                    font-weight: 500;
                    color: var(--text-muted);
                ">
                    {label_text}
                </span>
            }
        </div>
    }
}

fn get_risk_color(score: f32) -> &'static str {
    match score as u32 {
        s if s < 25 => "var(--risk-low)",
        s if s < 50 => "var(--risk-medium)",
        s if s < 75 => "var(--risk-high)",
        _ => "var(--risk-critical)",
    }
}

fn get_risk_label(score: f32) -> &'static str {
    match score as u32 {
        s if s < 25 => "LOW",
        s if s < 50 => "MEDIUM",
        s if s < 75 => "HIGH",
        _ => "CRITICAL",
    }
}
