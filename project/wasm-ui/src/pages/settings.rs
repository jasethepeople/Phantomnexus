use yew::prelude::*;

use crate::models::ui::{AppSettings, PrivacyStatus, Theme};

/// Settings page with API config, theme selector, and toggles
#[function_component(Settings)]
pub fn settings() -> Html {
    let settings = use_state(AppSettings::default);
    let saved = use_state(|| false);

    let update_api_url = {
        let settings = settings.clone();
        Callback::from(move |e: InputEvent| {
            let input = e.target_dyn_into::<web_sys::HtmlInputElement>();
            if let Some(input) = input {
                let mut s = (*settings).clone();
                s.api_base_url = input.value();
                settings.set(s);
            }
        })
    };

    let update_ws_url = {
        let settings = settings.clone();
        Callback::from(move |e: InputEvent| {
            let input = e.target_dyn_into::<web_sys::HtmlInputElement>();
            if let Some(input) = input {
                let mut s = (*settings).clone();
                s.ws_url = input.value();
                settings.set(s);
            }
        })
    };

    let update_theme = {
        let settings = settings.clone();
        Callback::from(move |e: Event| {
            let select = e.target_dyn_into::<web_sys::HtmlSelectElement>();
            if let Some(select) = select {
                let mut s = (*settings).clone();
                s.theme = match select.value().as_str() {
                    "light" => Theme::Light,
                    "system" => Theme::System,
                    _ => Theme::Dark,
                };
                settings.set(s);
            }
        })
    };

    let toggle_auto_refresh = {
        let settings = settings.clone();
        Callback::from(move |_| {
            let mut s = (*settings).clone();
            s.auto_refresh = !s.auto_refresh;
            settings.set(s);
        })
    };

    let toggle_show_preview = {
        let settings = settings.clone();
        Callback::from(move |_| {
            let mut s = (*settings).clone();
            s.show_preview = !s.show_preview;
            settings.set(s);
        })
    };

    let update_threshold = {
        let settings = settings.clone();
        Callback::from(move |e: InputEvent| {
            let input = e.target_dyn_into::<web_sys::HtmlInputElement>();
            if let Some(input) = input {
                let mut s = (*settings).clone();
                s.confidence_threshold = input.value_as_number() as f32;
                settings.set(s);
            }
        })
    };

    let save_settings = {
        let saved = saved.clone();
        Callback::from(move |_| {
            saved.set(true);
            // In real app, persist to localStorage
            wasm_bindgen_futures::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(2000).await;
                saved.set(false);
            });
        })
    };

    let theme_icon = match settings.theme {
        Theme::Dark => "🌙",
        Theme::Light => "☀",
        Theme::System => "💻",
    };

    html! {
        <div class="page-enter">
            <div class="container" style="max-width: 720px; padding-top: 48px; padding-bottom: 48px;">
                <div style="text-align: center; margin-bottom: 40px;">
                    <h1 style="font-size: 2rem; margin-bottom: 8px;">
                        {"Settings"}
                    </h1>
                    <p style="color: var(--text-secondary);">
                        {"Configure your dashboard preferences and API connections."}
                    </p>
                </div>

                if *saved {
                    <div style="
                        padding: 12px 16px;
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
                        {"Settings saved successfully!"}
                    </div>
                }

                // API Configuration
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
                        display: flex;
                        align-items: center;
                        gap: 8px;
                    ">
                        <span style="font-size: 1.25rem;">{"🔌"}</span>
                        {"API Configuration"}
                    </h2>

                    <div class="form-group">
                        <label class="form-label">{"API Base URL"}</label>
                        <input
                            type="url"
                            class="form-input"
                            value={settings.api_base_url.clone()}
                            oninput={update_api_url}
                            placeholder="http://localhost:8080/api"
                        />
                        <span style="
                            font-size: 0.75rem;
                            color: var(--text-muted);
                            margin-top: 4px;
                            display: block;
                        ">
                            {"The base URL for the Sentinel REST API"}
                        </span>
                    </div>

                    <div class="form-group" style="margin-bottom: 0;">
                        <label class="form-label">{"WebSocket URL"}</label>
                        <input
                            type="url"
                            class="form-input"
                            value={settings.ws_url.clone()}
                            oninput={update_ws_url}
                            placeholder="ws://localhost:8080/ws"
                        />
                        <span style="
                            font-size: 0.75rem;
                            color: var(--text-muted);
                            margin-top: 4px;
                            display: block;
                        ">
                            {"WebSocket endpoint for real-time updates"}
                        </span>
                    </div>
                </div>

                // Appearance
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
                        display: flex;
                        align-items: center;
                        gap: 8px;
                    ">
                        <span style="font-size: 1.25rem;">{theme_icon}</span>
                        {"Appearance"}
                    </h2>

                    <div class="form-group" style="margin-bottom: 0;">
                        <label class="form-label">{"Theme"}</label>
                        <select
                            class="form-input"
                            onchange={update_theme}
                        >
                            <option value="dark" selected={settings.theme == Theme::Dark}>
                                {"🌙 Dark"}
                            </option>
                            <option value="light" selected={settings.theme == Theme::Light}>
                                {"☀ Light"}
                            </option>
                            <option value="system" selected={settings.theme == Theme::System}>
                                {"💻 System"}
                            </option>
                        </select>
                        <span style="
                            font-size: 0.75rem;
                            color: var(--text-muted);
                            margin-top: 4px;
                            display: block;
                        ">
                            {"Choose your preferred color theme"}
                        </span>
                    </div>
                </div>

                // Analysis Preferences
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
                        display: flex;
                        align-items: center;
                        gap: 8px;
                    ">
                        <span style="font-size: 1.25rem;">{"⚙"}</span>
                        {"Analysis Preferences"}
                    </h2>

                    // Auto Refresh Toggle
                    <div style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        padding: 12px 0;
                        border-bottom: 1px solid var(--border-subtle);
                    ">
                        <div>
                            <div style="font-size: 0.875rem; font-weight: 500; color: var(--text-primary);">
                                {"Auto-refresh Jobs"}
                            </div>
                            <div style="font-size: 0.75rem; color: var(--text-muted);">
                                {"Automatically refresh job list every 5 seconds"}
                            </div>
                        </div>
                        <div
                            class={format!("toggle {}", if settings.auto_refresh { "active" } else { "" })}
                            onclick={toggle_auto_refresh}
                        />
                    </div>

                    // Show Preview Toggle
                    <div style="
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        padding: 12px 0;
                        border-bottom: 1px solid var(--border-subtle);
                    ">
                        <div>
                            <div style="font-size: 0.875rem; font-weight: 500; color: var(--text-primary);">
                                {"Show Fix Previews"}
                            </div>
                            <div style="font-size: 0.75rem; color: var(--text-muted);">
                                {"Display preview of changes before applying fixes"}
                            </div>
                        </div>
                        <div
                            class={format!("toggle {}", if settings.show_preview { "active" } else { "" })}
                            onclick={toggle_show_preview}
                        />
                    </div>

                    // Confidence Threshold
                    <div style="padding-top: 16px;">
                        <div style="
                            display: flex;
                            align-items: center;
                            justify-content: space-between;
                            margin-bottom: 8px;
                        ">
                            <label class="form-label" style="margin: 0;">
                                {"Confidence Threshold"}
                            </label>
                            <span style="
                                font-family: 'JetBrains Mono', monospace;
                                font-size: 0.8125rem;
                                font-weight: 600;
                                color: var(--accent);
                            ">
                                {format!("{:.0}%", settings.confidence_threshold * 100.0)}
                            </span>
                        </div>
                        <input
                            type="range"
                            min="0"
                            max="1"
                            step="0.05"
                            value={settings.confidence_threshold.to_string()}
                            oninput={update_threshold}
                            style="
                                width: 100%;
                                -webkit-appearance: none;
                                appearance: none;
                                height: 6px;
                                border-radius: 3px;
                                background: linear-gradient(90deg, var(--accent) {}%, var(--bg-tertiary) {}%);
                                outline: none;
                                cursor: pointer;
                            "
                        />
                        <div style="
                            display: flex;
                            justify-content: space-between;
                            margin-top: 4px;
                        ">
                            <span style="font-size: 0.625rem; color: var(--text-muted);">
                                {"0%"}
                            </span>
                            <span style="font-size: 0.625rem; color: var(--text-muted);">
                                {"50%"}
                            </span>
                            <span style="font-size: 0.625rem; color: var(--text-muted);">
                                {"100%"}
                            </span>
                        </div>
                        <span style="
                            font-size: 0.75rem;
                            color: var(--text-muted);
                            margin-top: 8px;
                            display: block;
                        ">
                            {"Only show violations with confidence above this threshold"}
                        </span>
                    </div>
                </div>

                // About
                <div style="
                    background: var(--bg-card);
                    border: 1px solid var(--border-subtle);
                    border-radius: var(--radius-lg);
                    padding: 24px;
                    margin-bottom: 32px;
                ">
                    <h2 style="
                        font-size: 1rem;
                        font-weight: 600;
                        margin-bottom: 16px;
                        color: var(--text-primary);
                        display: flex;
                        align-items: center;
                        gap: 8px;
                    ">
                        <span style="font-size: 1.25rem;">{"ℹ"}</span>
                        {"About"}
                    </h2>
                    <div style="display: flex; flex-direction: column; gap: 8px;">
                        <div style="
                            display: flex;
                            justify-content: space-between;
                            align-items: center;
                            padding: 8px 0;
                            border-bottom: 1px solid var(--border-subtle);
                        ">
                            <span style="font-size: 0.8125rem; color: var(--text-secondary);">
                                {"Version"}
                            </span>
                            <span style="
                                font-family: 'JetBrains Mono', monospace;
                                font-size: 0.8125rem;
                                color: var(--text-primary);
                            ">
                                {"2.1.0"}
                            </span>
                        </div>
                        <div style="
                            display: flex;
                            justify-content: space-between;
                            align-items: center;
                            padding: 8px 0;
                            border-bottom: 1px solid var(--border-subtle);
                        ">
                            <span style="font-size: 0.8125rem; color: var(--text-secondary);">
                                {"Build"}
                            </span>
                            <span style="
                                font-family: 'JetBrains Mono', monospace;
                                font-size: 0.8125rem;
                                color: var(--text-primary);
                            ">
                                {"2024.01.15-abc123"}
                            </span>
                        </div>
                        <div style="
                            display: flex;
                            justify-content: space-between;
                            align-items: center;
                            padding: 8px 0;
                        ">
                            <span style="font-size: 0.8125rem; color: var(--text-secondary);">
                                {"License"}
                            </span>
                            <span style="font-size: 0.8125rem; color: var(--text-primary);">
                                {"MIT"}
                            </span>
                        </div>
                    </div>
                </div>

                // Save button
                <button
                    class="btn btn-primary"
                    style="width: 100%; padding: 14px; font-size: 1rem;"
                    onclick={save_settings}
                >
                    {"Save Settings"}
                </button>
            </div>
        </div>
    }
}
