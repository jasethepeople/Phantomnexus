use wasm_bindgen::JsCast;
use web_sys::{DragEvent, Event, FileList, HtmlInputElement};
use yew::prelude::*;

use crate::models::ui::{AnalysisStatus, PrivacyStatus, SubmitAnalysisRequest};
use crate::services::api::ApiClient;

/// Upload page with drag & drop, metadata form, and validation
#[function_component(Upload)]
pub fn upload() -> Html {
    let title = use_state(|| String::new());
    let description = use_state(|| String::new());
    let tags = use_state(|| String::new());
    let category = use_state(|| String::new());
    let privacy = use_state(|| PrivacyStatus::Public);
    let language = use_state(|| "en".to_string());
    let video_url = use_state(|| String::new());
    let selected_file = use_state(|| None::<String>);
    let is_drag_over = use_state(|| false);
    let is_uploading = use_state(|| false);
    let upload_progress = use_state(|| 0f32);
    let error_msg = use_state(|| None::<String>);
    let success_msg = use_state(|| None::<String>);

    // Validation
    let has_title = !title.is_empty();
    let has_content = !video_url.is_empty() || selected_file.is_some();
    let is_valid = has_title && has_content;

    // Drag handlers
    let on_drag_over = {
        let is_drag_over = is_drag_over.clone();
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            is_drag_over.set(true);
        })
    };

    let on_drag_leave = {
        let is_drag_over = is_drag_over.clone();
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            is_drag_over.set(false);
        })
    };

    let on_drop = {
        let is_drag_over = is_drag_over.clone();
        let selected_file = selected_file.clone();
        Callback::from(move |e: DragEvent| {
            e.prevent_default();
            is_drag_over.set(false);
            if let Some(files) = e.data_transfer().and_then(|dt| dt.files()) {
                if let Some(file) = files.get(0) {
                    selected_file.set(Some(file.name()));
                }
            }
        })
    };

    // File input handler
    let on_file_change = {
        let selected_file = selected_file.clone();
        Callback::from(move |e: Event| {
            let input = e.target_dyn_into::<HtmlInputElement>();
            if let Some(input) = input {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        selected_file.set(Some(file.name()));
                    }
                }
            }
        })
    };

    // Submit handler
    let on_submit = {
        let title = title.clone();
        let description = description.clone();
        let tags = tags.clone();
        let category = category.clone();
        let privacy = privacy.clone();
        let language = language.clone();
        let video_url = video_url.clone();
        let selected_file = selected_file.clone();
        let is_uploading = is_uploading.clone();
        let upload_progress = upload_progress.clone();
        let error_msg = error_msg.clone();
        let success_msg = success_msg.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            if !is_valid {
                error_msg.set(Some("Please fill in all required fields".to_string()));
                return;
            }

            is_uploading.set(true);
            upload_progress.set(0.0);
            error_msg.set(None);
            success_msg.set(None);

            let request = SubmitAnalysisRequest {
                title: (*title).clone(),
                description: (*description).clone(),
                tags: (*tags).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                video_data: None,
                video_url: if (*video_url).is_empty() { None } else { Some((*video_url).clone()) },
                privacy: (*privacy).clone(),
                language: (*language).clone(),
                category: (*category).clone(),
            };

            // Simulate upload progress
            let progress = upload_progress.clone();
            let is_up = is_uploading.clone();
            let err = error_msg.clone();
            let succ = success_msg.clone();

            wasm_bindgen_futures::spawn_local(async move {
                // Simulate progress steps
                for i in [10, 30, 55, 80, 100].iter() {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                    progress.set(*i as f32);
                }

                let client = ApiClient::new("http://localhost:8080/api".to_string());
                match client.submit_analysis(request).await {
                    Ok(response) => {
                        succ.set(Some(format!("Analysis submitted! Job ID: {}", response.job_id)));
                    }
                    Err(e) => {
                        err.set(Some(format!("Failed to submit: {}", e)));
                    }
                }
                is_up.set(false);
            });
        })
    };

    let drag_zone_class = if *is_drag_over {
        "drag-zone drag-over"
    } else {
        "drag-zone"
    };

    html! {
        <div class="page-enter">
            <div class="container" style="max-width: 720px; padding-top: 48px; padding-bottom: 48px;">
                <div style="text-align: center; margin-bottom: 40px;">
                    <h1 style="font-size: 2rem; margin-bottom: 8px;">
                        {"Analyze a Video"}
                    </h1>
                    <p style="color: var(--text-secondary);">
                        {"Upload a video file or provide a URL for compliance analysis."}
                    </p>
                </div>

                if let Some(ref err) = *error_msg {
                    <div style="
                        padding: 12px 16px;
                        background: var(--risk-critical-bg);
                        border: 1px solid rgba(239, 68, 68, 0.3);
                        border-radius: var(--radius-md);
                        color: var(--risk-critical);
                        font-size: 0.875rem;
                        margin-bottom: 24px;
                        animation: fadeIn 0.3s ease;
                    ">
                        {err}
                    </div>
                }

                if let Some(ref succ) = *success_msg {
                    <div style="
                        padding: 12px 16px;
                        background: var(--risk-low-bg);
                        border: 1px solid rgba(74, 222, 128, 0.3);
                        border-radius: var(--radius-md);
                        color: var(--risk-low);
                        font-size: 0.875rem;
                        margin-bottom: 24px;
                        animation: fadeIn 0.3s ease;
                    ">
                        {succ}
                    </div>
                }

                <form {onsubmit}>
                    // Video Upload Section
                    <div style="
                        background: var(--bg-card);
                        border: 1px solid var(--border-subtle);
                        border-radius: var(--radius-lg);
                        padding: 24px;
                        margin-bottom: 24px;
                    ">
                        <h2 style="
                            font-size: 1rem;
                            font-weight: 600;
                            margin-bottom: 16px;
                            color: var(--text-primary);
                        ">
                            {"Video Source"}
                        </h2>

                        // Drag & Drop Zone
                        <div
                            class={drag_zone_class}
                            ondragover={on_drag_over}
                            ondragleave={on_drag_leave}
                            ondrop={on_drop}
                            style="margin-bottom: 16px;"
                        >
                            <div style="
                                width: 56px;
                                height: 56px;
                                border-radius: 50%;
                                background: var(--accent-dim);
                                color: var(--accent);
                                display: flex;
                                align-items: center;
                                justify-content: center;
                                font-size: 1.5rem;
                                margin: 0 auto 16px;
                            ">
                                {"📁"}
                            </div>
                            <p style="
                                font-size: 0.875rem;
                                font-weight: 500;
                                color: var(--text-primary);
                                margin-bottom: 4px;
                            ">
                                if let Some(ref filename) = *selected_file {
                                    {format!("Selected: {}", filename)}
                                } else {
                                    {"Drag & drop your video here"}
                                }
                            </p>
                            <p style="
                                font-size: 0.8125rem;
                                color: var(--text-muted);
                                margin-bottom: 16px;
                            ">
                                {"MP4, MOV, AVI up to 500MB"}
                            </p>
                            <label style="
                                display: inline-block;
                                padding: 8px 20px;
                                background: var(--bg-tertiary);
                                border: 1px solid var(--border-subtle);
                                border-radius: var(--radius-md);
                                color: var(--text-secondary);
                                font-size: 0.8125rem;
                                font-weight: 500;
                                cursor: pointer;
                                transition: all 0.2s ease;
                            "
                            onmouseenter={Callback::from(|e| {
                                let el = e.target_dyn_into::<web_sys::HtmlElement>();
                                if let Some(el) = el { el.style().set_property("border-color", "var(--border-active)").ok(); }
                            })}
                            onmouseleave={Callback::from(|e| {
                                let el = e.target_dyn_into::<web_sys::HtmlElement>();
                                if let Some(el) = el { el.style().set_property("border-color", "var(--border-subtle)").ok(); }
                            })}
                            >
                                {"Browse Files"}
                                <input
                                    type="file"
                                    accept="video/*"
                                    style="display: none;"
                                    onchange={on_file_change}
                                />
                            </label>
                        </div>

                        // Or divider
                        <div style="
                            display: flex;
                            align-items: center;
                            gap: 16px;
                            margin-bottom: 16px;
                        ">
                            <div style="flex: 1; height: 1px; background: var(--border-subtle);" />
                            <span style="
                                font-size: 0.75rem;
                                color: var(--text-muted);
                                font-weight: 500;
                            ">
                                {"OR"}
                            </span>
                            <div style="flex: 1; height: 1px; background: var(--border-subtle);" />
                        </div>

                        // URL input
                        <div class="form-group" style="margin-bottom: 0;">
                            <label class="form-label">{"Video URL"}</label>
                            <input
                                type="url"
                                class="form-input"
                                placeholder="https://youtube.com/watch?v=..."
                                value={(*video_url).clone()}
                                oninput={Callback::from(move |e: InputEvent| {
                                    let input = e.target_dyn_into::<HtmlInputElement>();
                                    if let Some(input) = input {
                                        video_url.set(input.value());
                                    }
                                })}
                            />
                        </div>
                    </div>

                    // Metadata Form
                    <div style="
                        background: var(--bg-card);
                        border: 1px solid var(--border-subtle);
                        border-radius: var(--radius-lg);
                        padding: 24px;
                        margin-bottom: 24px;
                    ">
                        <h2 style="
                            font-size: 1rem;
                            font-weight: 600;
                            margin-bottom: 20px;
                            color: var(--text-primary);
                        ">
                            {"Video Metadata"}
                        </h2>

                        // Title
                        <div class="form-group">
                            <label class="form-label">
                                {"Title"}
                                <span style="color: var(--risk-critical);">{" *"}</span>
                            </label>
                            <input
                                type="text"
                                class="form-input"
                                placeholder="Enter video title"
                                value={(*title).clone()}
                                required=true
                                oninput={Callback::from(move |e: InputEvent| {
                                    let input = e.target_dyn_into::<HtmlInputElement>();
                                    if let Some(input) = input {
                                        title.set(input.value());
                                    }
                                })}
                            />
                            if !has_title {
                                <span style="font-size: 0.75rem; color: var(--text-muted); margin-top: 4px; display: block;">
                                    {"Title is required"}
                                </span>
                            }
                        </div>

                        // Description
                        <div class="form-group">
                            <label class="form-label">{"Description"}</label>
                            <textarea
                                class="form-input"
                                placeholder="Enter video description"
                                rows=4
                                value={(*description).clone()}
                                oninput={Callback::from(move |e: InputEvent| {
                                    let input = e.target_dyn_into::<web_sys::HtmlTextAreaElement>();
                                    if let Some(input) = input {
                                        description.set(input.value());
                                    }
                                })}
                            />
                        </div>

                        // Tags
                        <div class="form-group">
                            <label class="form-label">{"Tags (comma-separated)"}</label>
                            <input
                                type="text"
                                class="form-input"
                                placeholder="gaming, tutorial, review"
                                value={(*tags).clone()}
                                oninput={Callback::from(move |e: InputEvent| {
                                    let input = e.target_dyn_into::<HtmlInputElement>();
                                    if let Some(input) = input {
                                        tags.set(input.value());
                                    }
                                })}
                            />
                        </div>

                        // Category & Privacy row
                        <div style="
                            display: grid;
                            grid-template-columns: 1fr 1fr;
                            gap: 16px;
                        ">
                            <div class="form-group">
                                <label class="form-label">{"Category"}</label>
                                <select
                                    class="form-input"
                                    value={(*category).clone()}
                                    onchange={Callback::from(move |e: Event| {
                                        let select = e.target_dyn_into::<web_sys::HtmlSelectElement>();
                                        if let Some(select) = select {
                                            category.set(select.value());
                                        }
                                    })}
                                >
                                    <option value="">{"Select category"}</option>
                                    <option value="entertainment">{"Entertainment"}</option>
                                    <option value="education">{"Education"}</option>
                                    <option value="gaming">{"Gaming"}</option>
                                    <option value="music">{"Music"}</option>
                                    <option value="sports">{"Sports"}</option>
                                    <option value="technology">{"Technology"}</option>
                                    <option value="news">{"News"}</option>
                                    <option value="other">{"Other"}</option>
                                </select>
                            </div>

                            <div class="form-group">
                                <label class="form-label">{"Privacy"}</label>
                                <select
                                    class="form-input"
                                    onchange={Callback::from(move |e: Event| {
                                        let select = e.target_dyn_into::<web_sys::HtmlSelectElement>();
                                        if let Some(select) = select {
                                            privacy.set(match select.value().as_str() {
                                                "unlisted" => PrivacyStatus::Unlisted,
                                                "private" => PrivacyStatus::Private,
                                                _ => PrivacyStatus::Public,
                                            });
                                        }
                                    })}
                                >
                                    <option value="public">{"Public"}</option>
                                    <option value="unlisted">{"Unlisted"}</option>
                                    <option value="private">{"Private"}</option>
                                </select>
                            </div>
                        </div>

                        // Language
                        <div class="form-group" style="margin-bottom: 0;">
                            <label class="form-label">{"Language"}</label>
                            <select
                                class="form-input"
                                value={(*language).clone()}
                                onchange={Callback::from(move |e: Event| {
                                    let select = e.target_dyn_into::<web_sys::HtmlSelectElement>();
                                    if let Some(select) = select {
                                        language.set(select.value());
                                    }
                                })}
                            >
                                <option value="en">{"English"}</option>
                                <option value="es">{"Spanish"}</option>
                                <option value="fr">{"French"}</option>
                                <option value="de">{"German"}</option>
                                <option value="ja">{"Japanese"}</option>
                                <option value="ko">{"Korean"}</option>
                                <option value="zh">{"Chinese"}</option>
                                <option value="hi">{"Hindi"}</option>
                            </select>
                        </div>
                    </div>

                    // Upload progress
                    if *is_uploading {
                        <div style="
                            background: var(--bg-card);
                            border: 1px solid var(--border-subtle);
                            border-radius: var(--radius-lg);
                            padding: 24px;
                            margin-bottom: 24px;
                            text-align: center;
                        ">
                            <div class="animate-spin" style="
                                width: 32px;
                                height: 32px;
                                border: 3px solid var(--border-subtle);
                                border-top-color: var(--accent);
                                border-radius: 50%;
                                margin: 0 auto 16px;
                            " />
                            <p style="
                                font-size: 0.875rem;
                                font-weight: 500;
                                color: var(--text-primary);
                                margin-bottom: 12px;
                            ">
                                {"Submitting analysis..."}
                            </p>
                            <div class="progress-track">
                                <div class="progress-fill" style={format!("width: {}%;", *upload_progress)} />
                            </div>
                            <p style="
                                font-size: 0.75rem;
                                color: var(--text-muted);
                                margin-top: 8px;
                            ">
                                {format!("{:.0}%", *upload_progress)}
                            </p>
                        </div>
                    }

                    // Submit button
                    <button
                        type="submit"
                        class="btn btn-primary"
                        style="width: 100%; padding: 14px; font-size: 1rem;"
                        disabled={!is_valid || *is_uploading}
                    >
                        if *is_uploading {
                            {"Submitting..."}
                        } else {
                            {"Start Analysis"}
                        }
                    </button>
                </form>
            </div>
        </div>
    }
}
