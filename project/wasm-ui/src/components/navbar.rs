use yew::prelude::*;
use yew_router::prelude::*;

use crate::models::ui::ConnectionState;

/// Navigation bar props
#[derive(Properties, PartialEq)]
pub struct NavBarProps {
    #[prop_or(ConnectionState::Disconnected)]
    pub connection_state: ConnectionState,
}

/// Navigation link item
#[function_component(NavBar)]
pub fn navbar(props: &NavBarProps) -> Html {
    html! {
        <nav style="
            position: sticky;
            top: 0;
            z-index: 100;
            background: rgba(15, 15, 26, 0.85);
            backdrop-filter: blur(12px);
            border-bottom: 1px solid var(--border-subtle);
            padding: 0;
        ">
            <div class="container" style="
                display: flex;
                align-items: center;
                justify-content: space-between;
                height: 64px;
            ">
                <NavLogo />
                <NavLinks />
                <NavStatus connection_state={props.connection_state.clone()} />
            </div>
        </nav>
    }
}

#[function_component(NavLogo)]
fn nav_logo() -> Html {
    html! {
        <Link<crate::Route> to={crate::Route::Home} classes="nav-logo-link">
            <div style="
                display: flex;
                align-items: center;
                gap: 12px;
            ">
                <div style="
                    width: 36px;
                    height: 36px;
                    background: linear-gradient(135deg, var(--accent), #0099cc);
                    border-radius: 10px;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    font-weight: 800;
                    font-size: 18px;
                    color: #0f0f1a;
                    box-shadow: var(--shadow-glow);
                ">
                    {"S"}
                </div>
                <span style="
                    font-weight: 700;
                    font-size: 1.125rem;
                    color: var(--text-primary);
                    letter-spacing: -0.02em;
                ">
                    {"YouTube"}
                    <span style="color: var(--accent);">{" Sentinel"}</span>
                </span>
            </div>
        </Link<crate::Route>>
    }
}

#[function_component(NavLinks)]
fn nav_links() -> Html {
    let links = vec![
        ("/", "Home", crate::Route::Home),
        ("/upload", "Upload", crate::Route::Upload),
        ("/jobs", "Jobs", crate::Route::JobList),
        ("/settings", "Settings", crate::Route::Settings),
    ];

    html! {
        <div style="
            display: flex;
            align-items: center;
            gap: 4px;
        ">
            { links.into_iter().map(|(_path, label, route)| {
                html! {
                    <NavItem key={label} label={label} route={route} />
                }
            }).collect::<Html>() }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct NavItemProps {
    label: &'static str,
    route: crate::Route,
}

#[function_component(NavItem)]
fn nav_item(props: &NavItemProps) -> Html {
    html! {
        <Link<crate::Route>
            to={props.route.clone()}
            classes={classes!("nav-item")}
        >
            <span style="
                padding: 8px 16px;
                border-radius: 8px;
                font-size: 0.875rem;
                font-weight: 500;
                color: var(--text-secondary);
                transition: all 0.2s ease;
                display: block;
            ">
                {props.label}
            </span>
        </Link<crate::Route>>
    }
}

#[derive(Properties, PartialEq)]
struct NavStatusProps {
    connection_state: ConnectionState,
}

#[function_component(NavStatus)]
fn nav_status(props: &NavStatusProps) -> Html {
    let (dot_color, label) = match &props.connection_state {
        ConnectionState::Disconnected => ("var(--text-muted)", "Offline"),
        ConnectionState::Connecting => ("var(--accent)", "Connecting..."),
        ConnectionState::Connected => ("var(--success)", "Live"),
        ConnectionState::Error(_) => ("var(--error)", "Error"),
    };

    let pulse_animation = matches!(props.connection_state, ConnectionState::Connecting)
        .then_some("animation: status-dot-pulse 1.5s ease infinite;")
        .unwrap_or("");

    html! {
        <div style="
            display: flex;
            align-items: center;
            gap: 8px;
        ">
            <span style={format!("
                width: 8px;
                height: 8px;
                border-radius: 50%;
                background: {};
                box-shadow: 0 0 8px {};
                display: inline-block;
                {}
            ", dot_color, dot_color, pulse_animation)}>
            </span>
            <span style="
                font-size: 0.75rem;
                font-weight: 500;
                color: var(--text-muted);
            ">
                {label}
            </span>
        </div>
    }
}
