use yew::prelude::*;

use crate::models::ui::{SeverityLevel, TimelineEventType, TimelineEventView};

/// Timeline props
#[derive(Properties, PartialEq)]
pub struct TimelineProps {
    pub events: Vec<TimelineEventView>,
    #[prop_or(400)]
    pub height: u32,
    #[prop_or(false)]
    pub interactive: bool,
}

/// Interactive SVG timeline with color-coded events
#[function_component(Timeline)]
pub fn timeline(props: &TimelineProps) -> Html {
    if props.events.is_empty() {
        return html! {
            <div style="
                text-align: center;
                padding: 40px;
                color: var(--text-muted);
            ">
                {"No timeline events available"}
            </div>
        };
    }

    let total_duration = props.events.last().map(|e| e.timestamp).unwrap_or(1.0);
    let max_timestamp = total_duration.max(1.0);

    // Dimensions
    let width = 800u32;
    let height = props.height;
    let padding_left = 60u32;
    let padding_right = 20u32;
    let padding_top = 30u32;
    let padding_bottom = 50u32;
    let plot_width = width - padding_left - padding_right;
    let plot_height = height - padding_top - padding_bottom;

    // Timeline y position
    let timeline_y = padding_top + plot_height / 2;

    // Collect tick positions
    let tick_count = 8u32;
    let ticks: Vec<(f64, u32)> = (0..=tick_count)
        .map(|i| {
            let frac = i as f64 / tick_count as f64;
            let ts = frac * max_timestamp;
            let x = padding_left + (frac * plot_width as f64) as u32;
            (ts, x)
        })
        .collect();

    // Calculate event positions
    let event_positions: Vec<(f64, u32, u32)> = props
        .events
        .iter()
        .map(|event| {
            let x = padding_left + ((event.timestamp / max_timestamp) * plot_width as f64) as u32;
            let y = timeline_y;
            (event.timestamp, x, y)
        })
        .collect();

    // Selected event for detail view
    let selected_event = use_state(|| None::<usize>);

    let select_event = {
        let selected = selected_event.clone();
        Callback::from(move |idx: usize| {
            selected.set(Some(idx));
        })
    };

    html! {
        <div>
            <svg
                width="100%"
                viewBox={format!("0 0 {} {}", width, height)}
                style="max-width: 100%;"
            >
                <defs>
                    // Gradient definitions for glow effects
                    <radialGradient id="glow-info">
                        <stop offset="0%" stop-color="var(--accent)" stop-opacity="0.6"/>
                        <stop offset="100%" stop-color="var(--accent)" stop-opacity="0"/>
                    </radialGradient>
                    <radialGradient id="glow-warning">
                        <stop offset="0%" stop-color="var(--risk-medium)" stop-opacity="0.6"/>
                        <stop offset="100%" stop-color="var(--risk-medium)" stop-opacity="0"/>
                    </radialGradient>
                    <radialGradient id="glow-violation">
                        <stop offset="0%" stop-color="var(--risk-critical)" stop-opacity="0.6"/>
                        <stop offset="100%" stop-color="var(--risk-critical)" stop-opacity="0"/>
                    </radialGradient>
                    <radialGradient id="glow-fix">
                        <stop offset="0%" stop-color="var(--risk-low)" stop-opacity="0.6"/>
                        <stop offset="100%" stop-color="var(--risk-low)" stop-opacity="0"/>
                    </radialGradient>
                    <radialGradient id="glow-milestone">
                        <stop offset="0%" stop-color="var(--info)" stop-opacity="0.6"/>
                        <stop offset="100%" stop-color="var(--info)" stop-opacity="0"/>
                    </radialGradient>
                </defs>

                // Main timeline line
                <line
                    x1={padding_left.to_string()}
                    y1={timeline_y.to_string()}
                    x2={(width - padding_right).to_string()}
                    y2={timeline_y.to_string()}
                    stroke="var(--border-active)"
                    stroke-width="2"
                    stroke-linecap="round"
                />

                // Timeline glow
                <line
                    x1={padding_left.to_string()}
                    y1={timeline_y.to_string()}
                    x2={(width - padding_right).to_string()}
                    y2={timeline_y.to_string()}
                    stroke="var(--accent)"
                    stroke-width="1"
                    stroke-linecap="round"
                    opacity="0.3"
                />

                // Tick marks and labels
                { ticks.iter().map(|(ts, x)| {
                    let mins = (ts / 60.0).floor() as u32;
                    let secs = (ts % 60.0) as u32;
                    let label = format!("{:02}:{:02}", mins, secs);

                    html! {
                        <g key={*ts}>
                            <line
                                x1={x.to_string()}
                                y1={(timeline_y - 6).to_string()}
                                x2={x.to_string()}
                                y2={(timeline_y + 6).to_string()}
                                stroke="var(--text-muted)"
                                stroke-width="1"
                                opacity="0.5"
                            />
                            <text
                                x={x.to_string()}
                                y={(height - 15).to_string()}
                                text-anchor="middle"
                                style="
                                    font-family: 'JetBrains Mono', monospace;
                                    font-size: 10px;
                                    fill: var(--text-muted);
                                "
                            >
                                {label}
                            </text>
                        </g>
                    }
                }).collect::<Html>() }

                // Grid lines
                { ticks.iter().map(|(_ts, x)| {
                    html! {
                        <line
                            key={format!("grid-{}", x)}
                            x1={x.to_string()}
                            y1={padding_top.to_string()}
                            x2={x.to_string()}
                            y2={(padding_top + plot_height).to_string()}
                            stroke="var(--border-subtle)"
                            stroke-width="1"
                            opacity="0.2"
                            stroke-dasharray="2,4"
                        />
                    }
                }).collect::<Html>() }

                // Events
                { props.events.iter().enumerate().zip(event_positions.iter()).map(|((idx, event), (_ts, x, _y))| {
                    let color = event.event_type.css_color();
                    let glow_id = match event.event_type {
                        TimelineEventType::Info => "glow-info",
                        TimelineEventType::Warning => "glow-warning",
                        TimelineEventType::Violation => "glow-violation",
                        TimelineEventType::Fix => "glow-fix",
                        TimelineEventType::Milestone => "glow-milestone",
                    };

                    let is_selected = selected_event.as_ref() == Some(&idx);
                    let radius = if is_selected { 10 } else { 7 };
                    let y_offset = if idx % 2 == 0 { -20i32 } else { 20i32 };
                    let label_y = timeline_y as i32 + y_offset;

                    let on_click = {
                        let select = select_event.clone();
                        let i = idx;
                        Callback::from(move |_| {
                            select.emit(i);
                        })
                    };

                    html! {
                        <g
                            key={event.id.clone()}
                            onclick={on_click}
                            style="cursor: pointer;"
                        >
                            // Glow circle
                            <circle
                                cx={x.to_string()}
                                cy={timeline_y.to_string()}
                                r="18"
                                fill={format!("url(#{)", glow_id)}
                                opacity={if is_selected { "0.8" } else { "0.4" }}
                            />

                            // Main dot
                            <circle
                                cx={x.to_string()}
                                cy={timeline_y.to_string()}
                                r={radius.to_string()}
                                fill={color}
                                stroke="var(--bg-primary)"
                                stroke-width="2"
                                style="transition: all 0.2s ease;"
                            />

                            // Stem line
                            <line
                                x1={x.to_string()}
                                y1={timeline_y.to_string()}
                                x2={x.to_string()}
                                y2={label_y.to_string()}
                                stroke={color}
                                stroke-width="1"
                                opacity="0.5"
                            />

                            // Event label
                            <text
                                x={x.to_string()}
                                y={(label_y - 6).to_string()}
                                text-anchor="middle"
                                style={format!("
                                    font-family: 'Inter', sans-serif;
                                    font-size: 10px;
                                    font-weight: 600;
                                    fill: {};
                                ", color)}
                            >
                                {&event.label}
                            </text>

                            // Event type badge
                            <text
                                x={x.to_string()}
                                y={(label_y + 8).to_string()}
                                text-anchor="middle"
                                style="
                                    font-family: 'Inter', sans-serif;
                                    font-size: 9px;
                                    fill: var(--text-muted);
                                "
                            >
                                {format!("{}", event.timestamp)}
                            </text>
                        </g>
                    }
                }).collect::<Html>() }
            </svg>

            // Selected event detail
            if let Some(idx) = *selected_event {
                if let Some(event) = props.events.get(idx) {
                    <div style="
                        margin-top: 16px;
                        padding: 16px;
                        background: var(--bg-secondary);
                        border: 1px solid var(--border-subtle);
                        border-radius: var(--radius-md);
                        animation: fadeIn 0.2s ease forwards;
                    ">
                        <div style="
                            display: flex;
                            align-items: center;
                            gap: 8px;
                            margin-bottom: 8px;
                        ">
                            <span style={format!("
                                width: 8px;
                                height: 8px;
                                border-radius: 50%;
                                background: {};
                                display: inline-block;
                            ", event.event_type.css_color())} />
                            <span style="
                                font-weight: 600;
                                font-size: 0.875rem;
                                color: var(--text-primary);
                            ">
                                {&event.label}
                            </span>
                            <span class={format!("badge {}", match event.event_type {
                                TimelineEventType::Info => "badge-accent",
                                TimelineEventType::Warning => "badge-medium",
                                TimelineEventType::Violation => "badge-critical",
                                TimelineEventType::Fix => "badge-low",
                                TimelineEventType::Milestone => "badge-accent",
                            })}>
                                {match event.event_type {
                                    TimelineEventType::Info => "Info",
                                    TimelineEventType::Warning => "Warning",
                                    TimelineEventType::Violation => "Violation",
                                    TimelineEventType::Fix => "Fix",
                                    TimelineEventType::Milestone => "Milestone",
                                }}
                            </span>
                        </div>
                        <p style="
                            font-size: 0.8125rem;
                            color: var(--text-secondary);
                            margin: 0;
                        ">
                            {&event.description}
                        </p>
                    </div>
                }
            }
        </div>
    }
}
