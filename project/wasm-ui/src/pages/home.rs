use yew::prelude::*;
use yew_router::prelude::*;

use crate::Route;

/// Animated stat counter
#[derive(Properties, PartialEq)]
struct StatProps {
    value: &'static str,
    label: &'static str,
    suffix: &'static str,
    delay: &'static str,
}

#[function_component(Stat)]
fn stat(props: &StatProps) -> Html {
    html! {
        <div style={format!("
            text-align: center;
            padding: 24px;
            animation: slideUp 0.6s ease forwards;
            animation-delay: {};
            opacity: 0;
        ", props.delay)}>
            <div style="
                font-size: 2.5rem;
                font-weight: 800;
                color: var(--accent);
                font-family: 'JetBrains Mono', monospace;
                margin-bottom: 4px;
            ">
                {props.value}
                <span style="font-size: 1.25rem; color: var(--text-muted);">
                    {props.suffix}
                </span>
            </div>
            <div style="
                font-size: 0.875rem;
                color: var(--text-muted);
                font-weight: 500;
            ">
                {props.label}
            </div>
        </div>
    }
}

/// Feature card
#[derive(Properties, PartialEq)]
struct FeatureProps {
    icon: &'static str,
    title: &'static str,
    description: &'static str,
    delay: &'static str,
}

#[function_component(Feature)]
fn feature(props: &FeatureProps) -> Html {
    html! {
        <div class="card" style={format!("
            animation: slideUp 0.6s ease forwards;
            animation-delay: {};
            opacity: 0;
        ", props.delay)}>
            <div style={format!("
                width: 48px;
                height: 48px;
                border-radius: 12px;
                background: var(--accent-dim);
                color: var(--accent);
                display: flex;
                align-items: center;
                justify-content: center;
                font-size: 1.5rem;
                margin-bottom: 16px;
            ")}>
                {props.icon}
            </div>
            <h3 style="
                font-size: 1.125rem;
                font-weight: 600;
                color: var(--text-primary);
                margin-bottom: 8px;
            ">
                {props.title}
            </h3>
            <p style="
                font-size: 0.875rem;
                color: var(--text-secondary);
                line-height: 1.6;
                margin: 0;
            ">
                {props.description}
            </p>
        </div>
    }
}

/// Home page with hero, animated stats, and feature grid
#[function_component(Home)]
pub fn home() -> Html {
    let features = vec![
        ("🔍", "AI-Powered Analysis", "Advanced machine learning models analyze your video content for potential compliance risks before publication."),
        ("⚡", "Real-Time Monitoring", "Watch analysis progress live with WebSocket streaming. Get instant feedback as each pipeline stage completes."),
        ("🛡", "Risk Assessment", "Comprehensive risk scoring across copyright, community guidelines, and monetization dimensions."),
        ("🔧", "Auto-Fix Suggestions", "Get actionable fix suggestions with adjustable parameters. Preview changes before applying them."),
        ("📊", "Detailed Reports", "Full analysis reports with timeline visualization, violation breakdowns, and compliance recommendations."),
        ("🔌", "API Integration", "Seamlessly integrate with your existing workflow via REST API and WebSocket endpoints."),
    ];

    html! {
        <div class="page-enter">
            // Hero Section
            <section style="
                text-align: center;
                padding: 80px 0 60px;
                position: relative;
                overflow: hidden;
            ">
                // Background glow effect
                <div style="
                    position: absolute;
                    top: 50%;
                    left: 50%;
                    transform: translate(-50%, -50%);
                    width: 600px;
                    height: 600px;
                    background: radial-gradient(circle, rgba(0, 212, 255, 0.08) 0%, transparent 70%);
                    pointer-events: none;
                " />

                <div class="container">
                    <div style="
                        display: inline-flex;
                        align-items: center;
                        gap: 8px;
                        padding: 6px 16px;
                        background: var(--accent-dim);
                        border-radius: 20px;
                        margin-bottom: 24px;
                        animation: fadeIn 0.5s ease forwards;
                    ">
                        <span style="
                            width: 8px;
                            height: 8px;
                            border-radius: 50%;
                            background: var(--accent);
                            display: inline-block;
                        " />
                        <span style="
                            font-size: 0.8125rem;
                            font-weight: 500;
                            color: var(--accent);
                        ">
                            {"AI-Powered Content Compliance"}
                        </span>
                    </div>

                    <h1 style="
                        font-size: 3.5rem;
                        font-weight: 900;
                        line-height: 1.1;
                        margin-bottom: 20px;
                        animation: slideUp 0.6s ease forwards;
                    ">
                        {"Protect Your Content"}
                        <br />
                        <span style="
                            background: linear-gradient(135deg, var(--accent), #00a8d4);
                            -webkit-background-clip: text;
                            -webkit-text-fill-color: transparent;
                            background-clip: text;
                        ">
                            {"Before You Publish"}
                        </span>
                    </h1>

                    <p style="
                        font-size: 1.125rem;
                        color: var(--text-secondary);
                        max-width: 600px;
                        margin: 0 auto 40px;
                        line-height: 1.7;
                        animation: slideUp 0.6s ease forwards;
                        animation-delay: 0.1s;
                        opacity: 0;
                    ">
                        {"YouTube Sentinel analyzes your videos for copyright issues, community guideline violations, and monetization risks using advanced AI — all before you hit publish."}
                    </p>

                    <div style="
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        gap: 16px;
                        animation: slideUp 0.6s ease forwards;
                        animation-delay: 0.2s;
                        opacity: 0;
                    ">
                        <Link<Route> to={Route::Upload} classes="btn btn-primary" style="padding: 12px 28px; font-size: 1rem;">
                            {"Analyze a Video"}
                        </Link<Route>>
                        <Link<Route> to={Route::JobList} classes="btn btn-secondary" style="padding: 12px 28px; font-size: 1rem;">
                            {"View Jobs"}
                        </Link<Route>>
                    </div>
                </div>
            </section>

            // Stats Section
            <section style="
                padding: 40px 0;
                border-top: 1px solid var(--border-subtle);
                border-bottom: 1px solid var(--border-subtle);
                background: var(--bg-secondary);
            ">
                <div class="container">
                    <div style="
                        display: grid;
                        grid-template-columns: repeat(4, 1fr);
                        gap: 0;
                    ">
                        <Stat value="99.7" label="Accuracy Rate" suffix="%" delay="0.3s" />
                        <Stat value="< 3" label="Analysis Time" suffix="min" delay="0.4s" />
                        <Stat value="50K" label="Videos Analyzed" suffix="+" delay="0.5s" />
                        <Stat value="15" label="Risk Categories" suffix="+" delay="0.6s" />
                    </div>
                </div>
            </section>

            // Features Section
            <section style="padding: 80px 0;">
                <div class="container">
                    <div style="
                        text-align: center;
                        margin-bottom: 60px;
                        animation: slideUp 0.6s ease forwards;
                        animation-delay: 0.3s;
                        opacity: 0;
                    ">
                        <h2 style="
                            font-size: 2rem;
                            font-weight: 800;
                            margin-bottom: 12px;
                        ">
                            {"Everything You Need for"}
                            <span style="color: var(--accent);">{" Content Safety"}</span>
                        </h2>
                        <p style="
                            font-size: 1rem;
                            color: var(--text-secondary);
                            max-width: 500px;
                            margin: 0 auto;
                        ">
                            {"Comprehensive tools to analyze, monitor, and fix compliance issues in your videos."}
                        </p>
                    </div>

                    <div class="grid grid-cols-3">
                        { features.into_iter().enumerate().map(|(i, (icon, title, desc))| {
                            html! {
                                <Feature
                                    key={i}
                                    icon={icon}
                                    title={title}
                                    description={desc}
                                    delay={format!("{}s", 0.4 + i as f32 * 0.1)}
                                />
                            }
                        }).collect::<Html>() }
                    </div>
                </div>
            </section>

            // CTA Section
            <section style="
                padding: 60px 0;
                text-align: center;
                background: var(--bg-secondary);
                border-top: 1px solid var(--border-subtle);
                animation: slideUp 0.6s ease forwards;
                animation-delay: 0.8s;
                opacity: 0;
            ">
                <div class="container">
                    <h2 style="
                        font-size: 1.75rem;
                        font-weight: 800;
                        margin-bottom: 16px;
                    ">
                        {"Ready to Secure Your Content?"}
                    </h2>
                    <p style="
                        font-size: 1rem;
                        color: var(--text-secondary);
                        margin-bottom: 32px;
                        max-width: 500px;
                        margin-left: auto;
                        margin-right: auto;
                    ">
                        {"Upload your first video and get a comprehensive compliance analysis in under 3 minutes."}
                    </p>
                    <Link<Route> to={Route::Upload} classes="btn btn-primary" style="padding: 14px 32px; font-size: 1rem;">
                        {"Get Started Free"}
                    </Link<Route>>
                </div>
            </section>
        </div>
    }
}
