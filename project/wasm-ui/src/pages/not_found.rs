use yew::prelude::*;
use yew_router::prelude::*;

use crate::Route;

/// 404 Not Found page
#[function_component(NotFound)]
pub fn not_found() -> Html {
    html! {
        <div class="page-enter" style="
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 60vh;
        ">
            <div style="text-align: center;">
                <div style="
                    font-size: 6rem;
                    font-weight: 900;
                    color: var(--accent);
                    font-family: 'JetBrains Mono', monospace;
                    margin-bottom: 16px;
                    text-shadow: var(--shadow-glow);
                    animation: float 3s ease-in-out infinite;
                ">
                    {"404"}
                </div>
                <h1 style="
                    font-size: 1.5rem;
                    font-weight: 700;
                    margin-bottom: 8px;
                    color: var(--text-primary);
                ">
                    {"Page Not Found"}
                </h1>
                <p style="
                    color: var(--text-secondary);
                    margin-bottom: 32px;
                    max-width: 400px;
                ">
                    {"The page you're looking for doesn't exist or has been moved."}
                </p>
                <Link<Route> to={Route::Home} classes="btn btn-primary">
                    {"← Back to Home"}
                </Link<Route>>
            </div>
        </div>
    }
}
