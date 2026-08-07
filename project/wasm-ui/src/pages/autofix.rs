use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::fix_panel::FixPanel;
use crate::models::ui::{
    AnalysisStatus, FixParameter, FixSuggestionView, FixType, SeverityLevel, ViolationType,
};
use crate::Route;

/// Auto-fix page props
#[derive(Properties, PartialEq)]
pub struct AutoFixProps {
    pub id: String,
}

/// Fix workflow: violation list, suggested fixes, apply-all button
#[function_component(AutoFix)]
pub fn auto_fix(props: &AutoFixProps) -> Html {
    let selected_fixes = use_state(|| Vec::<String>::new());
    let apply_all_progress = use_state(|| None::<(usize, usize)>); // (done, total)
    let show_applied = use_state(|| false);

    // Mock fix suggestions
    let suggestions = vec![
        FixSuggestionView {
            id: "fix_1".to_string(),
            violation_id: "v1".to_string(),
            fix_type: FixType::Replace,
            description: "Replace copyrighted audio segment".to_string(),
            original_value: "Audio segment: 'Song Name' by Artist @ 02:15-03:42".to_string(),
            suggested_value: "Replace with royalty-free music from Audio Library".to_string(),
            parameters: vec![
                FixParameter::new("start_time", "Start Time", 135.0, 0.0, 1800.0, 1.0, "s"),
                FixParameter::new("end_time", "End Time", 222.0, 0.0, 1800.0, 1.0, "s"),
                FixParameter::new("fade_duration", "Fade Duration", 2.0, 0.0, 10.0, 0.5, "s"),
            ],
            preview: "Audio segment will be replaced with similar royalty-free music. Fade in/out will be applied at boundaries.".to_string(),
            confidence: 0.92,
            auto_applicable: true,
        },
        FixSuggestionView {
            id: "fix_2".to_string(),
            violation_id: "v2".to_string(),
            fix_type: FixType::Mute,
            description: "Mute strong language instances".to_string(),
            original_value: "3 instances of strong language detected in transcript".to_string(),
            suggested_value: "Mute audio at profanity timestamps with 0.5s fade".to_string(),
            parameters: vec![
                FixParameter::new("fade_duration", "Mute Fade", 0.5, 0.0, 2.0, 0.1, "s"),
                FixParameter::new("beep_volume", "Beep Volume", 0.3, 0.0, 1.0, 0.05, ""),
            ],
            preview: "Audio will be muted at detected profanity timestamps with a brief fade. Optional beep tone can be added.".to_string(),
            confidence: 0.88,
            auto_applicable: true,
        },
        FixSuggestionView {
            id: "fix_3".to_string(),
            violation_id: "v3".to_string(),
            fix_type: FixType::Replace,
            description: "Increase thumbnail text size".to_string(),
            original_value: "Thumbnail text: 8pt font size".to_string(),
            suggested_value: "Thumbnail text: 18pt font size with bold styling".to_string(),
            parameters: vec![
                FixParameter::new("font_size", "Font Size", 18.0, 12.0, 36.0, 1.0, "pt"),
                FixParameter::new("contrast", "Contrast Boost", 1.3, 1.0, 2.0, 0.1, "x"),
            ],
            preview: "Thumbnail text will be enlarged and bolded for better mobile readability. Contrast will be enhanced.".to_string(),
            confidence: 0.85,
            auto_applicable: true,
        },
        FixSuggestionView {
            id: "fix_4".to_string(),
            violation_id: "v4".to_string(),
            fix_type: FixType::AddDisclaimer,
            description: "Add gameplay disclaimer".to_string(),
            original_value: "No disclaimer present".to_string(),
            suggested_value: "This video features gameplay footage used under fair use for commentary and educational purposes.".to_string(),
            parameters: vec![
                FixParameter::new("duration", "Display Duration", 5.0, 3.0, 10.0, 0.5, "s"),
                FixParameter::new("opacity", "Text Opacity", 0.9, 0.5, 1.0, 0.05, ""),
            ],
            preview: "A brief disclaimer will be added at the beginning of the video explaining the fair use of gameplay footage.".to_string(),
            confidence: 0.78,
            auto_applicable: false,
        },
        FixSuggestionView {
            id: "fix_5".to_string(),
            violation_id: "v5".to_string(),
            fix_type: FixType::Remove,
            description: "Remove non-compliant external links".to_string(),
            original_value: "5 external links in description (2 potentially unsafe)".to_string(),
            suggested_value: "Remove 2 non-compliant links, keep 3 verified links".to_string(),
            parameters: vec![
                FixParameter::new("keep_count", "Links to Keep", 3.0, 0.0, 5.0, 1.0, ""),
            ],
            preview: "Non-compliant external links will be removed from the video description. Safe, verified links will remain.".to_string(),
            confidence: 0.82,
            auto_applicable: true,
        },
    ];

    let toggle_fix = {
        let selected = selected_fixes.clone();
        Callback::from(move |fix_id: String| {
            let mut new_selected = (*selected).clone();
            if new_selected.contains(&fix_id) {
                new_selected.retain(|id| id != &fix_id);
            } else {
                new_selected.push(fix_id);
            }
            selected.set(new_selected);
        })
    };

    let apply_all = {
        let selected = selected_fixes.clone();
        let progress = apply_all_progress.clone();
        let show_applied = show_applied.clone();
        let total = suggestions.len();
        Callback::from(move |_| {
            if selected.is_empty() {
                // Select all auto-applicable fixes
                let auto_fixes: Vec<String> = suggestions
                    .iter()
                    .filter(|s| s.auto_applicable)
                    .map(|s| s.id.clone())
                    .collect();
                selected.set(auto_fixes.clone());
            }

            let selected_ids = (*selected).clone();
            let progress = progress.clone();
            let show_applied = show_applied.clone();

            wasm_bindgen_futures::spawn_local(async move {
                progress.set(Some((0, selected_ids.len())));
                for i in 0..selected_ids.len() {
                    gloo_timers::future::TimeoutFuture::new(500).await;
                    progress.set(Some((i + 1, selected_ids.len())));
                }
                show_applied.set(true);
                gloo_timers::future::TimeoutFuture::new(2000).await;
                progress.set(None);
            });
        })
    };

    let auto_fixable_count = suggestions.iter().filter(|s| s.auto_applicable).count();

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
                            {"Auto-Fix"}
                        </h1>
                        <p style="color: var(--text-secondary); font-size: 0.875rem;">
                            {format!("{} suggested fixes for job {}", suggestions.len(), &props.id[..8])}
                        </p>
                    </div>
                    <div style="display: flex; gap: 8px;">
                        <span style="
                            font-size: 0.8125rem;
                            color: var(--text-muted);
                            padding: 8px 16px;
                            background: var(--bg-card);
                            border-radius: var(--radius-md);
                        ">
                            {format!("{} auto-fixable", auto_fixable_count)}
                        </span>
                        <button
                            class="btn btn-primary"
                            onclick={apply_all}
                            disabled={apply_all_progress.is_some()}
                        >
                            if let Some((done, total)) = *apply_all_progress {
                                {format!("Applying... {}/{}", done, total)}
                            } else {
                                {"Apply All Fixes"}
                            }
                        </button>
                    </div>
                </div>

                // Applied success message
                if *show_applied {
                    <div style="
                        padding: 16px;
                        background: var(--risk-low-bg);
                        border: 1px solid rgba(74, 222, 128, 0.3);
                        border-radius: var(--radius-md);
                        color: var(--risk-low);
                        font-size: 0.875rem;
                        font-weight: 500;
                        margin-bottom: 24px;
                        text-align: center;
                        animation: fadeIn 0.3s ease;
                    ">
                        {"All fixes have been applied successfully! A new version will be processed."}
                    </div>
                }

                // Fix selection summary
                if !(*selected_fixes).is_empty() {
                    <div style="
                        padding: 12px 16px;
                        background: var(--accent-dim);
                        border-radius: var(--radius-md);
                        margin-bottom: 24px;
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                    ">
                        <span style="font-size: 0.8125rem; color: var(--accent); font-weight: 500;">
                            {format!("{} fix{} selected", selected_fixes.len(), if selected_fixes.len() > 1 { "es" } else { "" })}
                        </span>
                        <button
                            class="btn btn-ghost"
                            style="font-size: 0.75rem; padding: 4px 10px;"
                            onclick={Callback::from({
                                let selected = selected_fixes.clone();
                                move |_| selected.set(Vec::new())
                            })}
                        >
                            {"Clear"}
                        </button>
                    </div>
                }

                // Fix Panels
                <div style="display: flex; flex-direction: column; gap: 24px;">
                    { suggestions.iter().map(|suggestion| {
                        let is_selected = (*selected_fixes).contains(&suggestion.id);
                        let toggle = toggle_fix.clone();
                        let id = suggestion.id.clone();

                        html! {
                            <div key={suggestion.id.clone()}>
                                <div style="
                                    display: flex;
                                    align-items: center;
                                    gap: 12px;
                                    margin-bottom: 8px;
                                ">
                                    <input
                                        type="checkbox"
                                        checked={is_selected}
                                        onclick={Callback::from(move |_| {
                                            toggle.emit(id.clone());
                                        })}
                                        style="
                                            width: 18px;
                                            height: 18px;
                                            accent-color: var(--accent);
                                            cursor: pointer;
                                        "
                                    />
                                    <span style="font-size: 0.8125rem; color: var(--text-muted);">
                                        if is_selected {
                                            {"Selected for apply"}
                                        } else {
                                            {"Click to select"}
                                        }
                                    </span>
                                </div>
                                <FixPanel
                                    suggestion={suggestion.clone()}
                                    on_apply={Some(Callback::from({
                                        let toggle = toggle_fix.clone();
                                        move |fix_id: String| {
                                            // Toggle selection when apply is clicked
                                            let t = toggle.clone();
                                            t.emit(fix_id);
                                        }
                                    }))}
                                />
                            </div>
                        }
                    }).collect::<Html>() }
                </div>

                // Bottom action bar
                <div style="
                    position: sticky;
                    bottom: 24px;
                    margin-top: 32px;
                    padding: 16px 24px;
                    background: var(--bg-card);
                    border: 1px solid var(--border-active);
                    border-radius: var(--radius-lg);
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    box-shadow: var(--shadow-lg);
                    backdrop-filter: blur(8px);
                ">
                    <div>
                        <span style="font-size: 0.875rem; color: var(--text-secondary);">
                            if (*selected_fixes).is_empty() {
                                {"No fixes selected"}
                            } else {
                                {format!("{} fix{} selected", selected_fixes.len(), if selected_fixes.len() > 1 { "es" } else { "" })}
                            }
                        </span>
                    </div>
                    <button
                        class="btn btn-primary"
                        onclick={apply_all}
                        disabled={apply_all_progress.is_some()}
                    >
                        if let Some((done, total)) = *apply_all_progress {
                            {format!("Applying {}/{}...", done, total)}
                        } else {
                            {"Apply Selected Fixes"}
                        }
                    </button>
                </div>
            </div>
        </div>
    }
}
