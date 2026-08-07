use yew::prelude::*;
use yew_router::prelude::*;

use crate::components::navbar::NavBar;
use crate::models::ui::ConnectionState;
use crate::pages::{
    autofix::AutoFix, home::Home, job_detail::JobDetail, job_list::JobList, not_found::NotFound,
    report::Report, settings::Settings, upload::Upload,
};

/// Application routes
#[derive(Debug, Clone, PartialEq, Routable)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/upload")]
    Upload,
    #[at("/jobs")]
    JobList,
    #[at("/jobs/:id")]
    JobDetail { id: String },
    #[at("/jobs/:id/report")]
    Report { id: String },
    #[at("/jobs/:id/fix")]
    AutoFix { id: String },
    #[at("/settings")]
    Settings,
    #[at("/404")]
    NotFound,
    #[not_found]
    #[at("/*path")]
    CatchAll { path: String },
}

/// Switch function to render the correct page for each route
fn switch(route: Route) -> Html {
    match route {
        Route::Home => html! { <Home /> },
        Route::Upload => html! { <Upload /> },
        Route::JobList => html! { <JobList /> },
        Route::JobDetail { id } => html! { <JobDetail {id} /> },
        Route::Report { id } => html! { <Report {id} /> },
        Route::AutoFix { id } => html! { <AutoFix {id} /> },
        Route::Settings => html! { <Settings /> },
        Route::NotFound | Route::CatchAll { .. } => html! { <NotFound /> },
    }
}

/// Footer component
#[function_component(Footer)]
fn footer() -> Html {
    html! {
        <footer style="
            border-top: 1px solid var(--border-subtle);
            background: var(--bg-secondary);
            padding: 32px 0;
            margin-top: auto;
        ">
            <div class="container">
                <div style="
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    flex-wrap: wrap;
                    gap: 16px;
                ">
                    <div style="
                        display: flex;
                        align-items: center;
                        gap: 8px;
                    ">
                        <div style="
                            width: 24px;
                            height: 24px;
                            background: linear-gradient(135deg, var(--accent), #0099cc);
                            border-radius: 6px;
                            display: flex;
                            align-items: center;
                            justify-content: center;
                            font-weight: 800;
                            font-size: 12px;
                            color: #0f0f1a;
                        ">
                            {"S"}
                        </div>
                        <span style="
                            font-size: 0.8125rem;
                            font-weight: 600;
                            color: var(--text-secondary);
                        ">
                            {"YouTube Sentinel"}
                        </span>
                    </div>
                    <div style="
                        display: flex;
                        align-items: center;
                        gap: 24px;
                    ">
                        <span style="font-size: 0.75rem; color: var(--text-muted);">
                            {"v2.1.0"}
                        </span>
                        <span style="font-size: 0.75rem; color: var(--text-muted);">
                            {"© 2024 Sentinel Team"}
                        </span>
                    </div>
                </div>
            </div>
        </footer>
    }
}

/// Main application component with BrowserRouter
#[function_component(App)]
pub fn app() -> Html {
    // Connection state for WebSocket status indicator
    let connection_state = use_state(|| ConnectionState::Disconnected);

    html! {
        <BrowserRouter>
            <div style="
                display: flex;
                flex-direction: column;
                min-height: 100vh;
            ">
                <NavBar connection_state={(*connection_state).clone()} />
                <main style="flex: 1;">
                    <Switch<Route> render={switch} />
                </main>
                <Footer />
            </div>
        </BrowserRouter>
    }
}
