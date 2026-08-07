use yew::prelude::*;

use crate::models::ui::{FixParameter, FixSuggestionView, FixType};

/// Fix panel props
#[derive(Properties, PartialEq)]
pub struct FixPanelProps {
    pub suggestion: FixSuggestionView,
    #[prop_or_default]
    pub on_apply: Option<Callback<String>>,
    #[prop_or_default]
    pub on_parameter_change: Option<Callback<(String, f32)>>,
}

/// Three-panel layout: original, adjustable params, preview
#[function_component(FixPanel)]
pub fn fix_panel(props: &FixPanelProps) -> Html {
    let suggestion = &props.suggestion;
    let fix_type_color = match suggestion.fix_type {
        FixType::Replace => "var(--accent)",
        FixType::Remove => "var(--risk-critical)",
        FixType::Mute => "var(--risk-medium)",
        FixType::Blur => "var(--risk-medium)",
        FixType::Trim => "var(--risk-high)",
        FixType::AddDisclaimer => "var(--risk-low)",
        FixType::ModifyTitle | FixType::ModifyDescription | FixType::ModifyTags => "var(--accent)",
        FixType::Other => "var(--text-muted)",
    };

    let on_apply = {
        let id = suggestion.id.clone();
        let cb = props.on_apply.clone();
        Callback::from(move |_| {
            if let Some(ref cb) = cb {
                cb.emit(id.clone());
            }
        })
    };

    html! {
        <div style="
            background: var(--bg-card);
            border: 1px solid var(--border-subtle);
            border-radius: var(--radius-lg);
            overflow: hidden;
        ">
            // Header
            <div style={format!("
                padding: 12px 16px;
                border-bottom: 1px solid var(--border-subtle);
                display: flex;
                align-items: center;
                justify-content: space-between;
                background: {}10;
            ", fix_type_color)}>
                <div style="display: flex; align-items: center; gap: 8px;">
                    <span style={format!("
                        padding: 2px 8px;
                        border-radius: 4px;
                        font-size: 0.6875rem;
                        font-weight: 600;
                        background: {}20;
                        color: {};
                    ", fix_type_color, fix_type_color)}>
                        {suggestion.fix_type.as_str()}
                    </span>
                    <span style="
                        font-size: 0.875rem;
                        font-weight: 600;
                        color: var(--text-primary);
                    ">
                        {&suggestion.description}
                    </span>
                </div>
                <div style="display: flex; align-items: center; gap: 8px;">
                    <span style="
                        font-size: 0.75rem;
                        color: var(--text-muted);
                    ">
                        {format!("{:.0}% confidence", suggestion.confidence * 100.0)}
                    </span>
                    if suggestion.auto_applicable {
                        <span class="badge badge-low">{"Auto"}</span>
                    }
                </div>
            </div>

            // Three-panel body
            <div style="
                display: grid;
                grid-template-columns: 1fr 1fr 1fr;
                gap: 0;
            ">
                // Panel 1: Original
                <div style="
                    padding: 16px;
                    border-right: 1px solid var(--border-subtle);
                ">
                    <h4 style="
                        font-size: 0.6875rem;
                        font-weight: 600;
                        text-transform: uppercase;
                        letter-spacing: 0.05em;
                        color: var(--text-muted);
                        margin-bottom: 12px;
                    ">
                        {"Original"}
                    </h4>
                    <div style="
                        background: var(--bg-secondary);
                        border: 1px solid var(--border-subtle);
                        border-radius: var(--radius-sm);
                        padding: 12px;
                        font-size: 0.8125rem;
                        color: var(--text-secondary);
                        min-height: 80px;
                        word-break: break-word;
                    ">
                        if suggestion.original_value.is_empty() {
                            <span style="color: var(--text-muted); font-style: italic;">
                                {"(no original value)"}
                            </span>
                        } else {
                            {&suggestion.original_value}
                        }
                    </div>
                </div>

                // Panel 2: Adjustable Parameters
                <div style="
                    padding: 16px;
                    border-right: 1px solid var(--border-subtle);
                ">
                    <h4 style="
                        font-size: 0.6875rem;
                        font-weight: 600;
                        text-transform: uppercase;
                        letter-spacing: 0.05em;
                        color: var(--text-muted);
                        margin-bottom: 12px;
                    ">
                        {"Parameters"}
                    </h4>
                    if suggestion.parameters.is_empty() {
                        <div style="
                            color: var(--text-muted);
                            font-size: 0.8125rem;
                            font-style: italic;
                            padding: 12px 0;
                        ">
                            {"No adjustable parameters"}
                        </div>
                    } else {
                        <div style="display: flex; flex-direction: column; gap: 12px;">
                            { suggestion.parameters.iter().map(|param| {
                                html! {
                                    <ParameterSlider
                                        key={param.name.clone()}
                                        param={param.clone()}
                                        on_change={props.on_parameter_change.clone()}
                                    />
                                }
                            }).collect::<Html>() }
                        </div>
                    }
                </div>

                // Panel 3: Preview
                <div style="padding: 16px;">
                    <h4 style="
                        font-size: 0.6875rem;
                        font-weight: 600;
                        text-transform: uppercase;
                        letter-spacing: 0.05em;
                        color: var(--text-muted);
                        margin-bottom: 12px;
                    ">
                        {"Preview"}
                    </h4>
                    <div style="
                        background: var(--bg-secondary);
                        border: 1px solid var(--border-subtle);
                        border-radius: var(--radius-sm);
                        padding: 12px;
                        font-size: 0.8125rem;
                        color: var(--text-secondary);
                        min-height: 80px;
                        word-break: break-word;
                    ">
                        if suggestion.preview.is_empty() {
                            <span style="color: var(--text-muted); font-style: italic;">
                                {&suggestion.suggested_value}
                            </span>
                        } else {
                            {&suggestion.preview}
                        }
                    </div>
                </div>
            </div>

            // Footer with apply button
            <div style="
                padding: 12px 16px;
                border-top: 1px solid var(--border-subtle);
                display: flex;
                align-items: center;
                justify-content: flex-end;
                gap: 8px;
            ">
                <span style="
                    font-size: 0.75rem;
                    color: var(--text-muted);
                    margin-right: auto;
                ">
                    {format!("ID: {}...", &suggestion.id.chars().take(8).collect::<String>())}
                </span>
                <button class="btn btn-secondary" style="font-size: 0.8125rem;">
                    {"Preview Only"}
                </button>
                <button class="btn btn-primary" style="font-size: 0.8125rem;" onclick={on_apply}>
                    {"Apply Fix"}
                </button>
            </div>
        </div>
    }
}

/// Individual parameter slider
#[derive(Properties, PartialEq)]
struct ParamSliderProps {
    param: FixParameter,
    #[prop_or_default]
    on_change: Option<Callback<(String, f32)>>,
}

#[function_component(ParameterSlider)]
fn parameter_slider(props: &ParamSliderProps) -> Html {
    let param = &props.param;
    let value = use_state(|| param.value);

    let on_input = {
        let name = param.name.clone();
        let on_change = props.on_change.clone();
        let value = value.clone();
        let min = param.min;
        let max = param.max;
        Callback::from(move |e: InputEvent| {
            let input = e.target_dyn_into::<web_sys::HtmlInputElement>();
            if let Some(input) = input {
                let val = input.value_as_number() as f32;
                let clamped = val.clamp(min, max);
                value.set(clamped);
                if let Some(ref cb) = on_change {
                    cb.emit((name.clone(), clamped));
                }
            }
        })
    };

    let percentage = ((param.value - param.min) / (param.max - param.min) * 100.0).clamp(0.0, 100.0);

    html! {
        <div>
            <div style="
                display: flex;
                align-items: center;
                justify-content: space-between;
                margin-bottom: 4px;
            ">
                <label style="
                    font-size: 0.75rem;
                    font-weight: 500;
                    color: var(--text-secondary);
                ">
                    {&param.label}
                </label>
                <span style="
                    font-family: 'JetBrains Mono', monospace;
                    font-size: 0.75rem;
                    font-weight: 600;
                    color: var(--accent);
                ">
                    {format!("{:.1}{}", *value, param.unit)}
                </span>
            </div>
            <div style="
                position: relative;
                height: 20px;
                display: flex;
                align-items: center;
            ">
                <input
                    type="range"
                    min={param.min.to_string()}
                    max={param.max.to_string()}
                    step={param.step.to_string()}
                    value={value.to_string()}
                    oninput={on_input}
                    style="
                        width: 100%;
                        -webkit-appearance: none;
                        appearance: none;
                        height: 4px;
                        border-radius: 2px;
                        background: var(--bg-tertiary);
                        outline: none;
                        cursor: pointer;
                    "
                />
            </div>
            <div style="
                display: flex;
                justify-content: space-between;
                margin-top: 2px;
            ">
                <span style="font-size: 0.625rem; color: var(--text-muted);">
                    {format!("{:.0}{}", param.min, param.unit)}
                </span>
                <span style="font-size: 0.625rem; color: var(--text-muted);">
                    {format!("{:.0}{}", param.max, param.unit)}
                </span>
            </div>
        </div>
    }
}
