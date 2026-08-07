use wasm_bindgen::prelude::*;
use yew::Renderer;

mod app;
mod components;
mod models;
mod pages;
mod services;

use app::App;

/// Initialize the WASM application with panic hook and start the Yew app
#[wasm_bindgen(start)]
pub fn main() {
    // Set up console panic hook for better debugging
    console_error_panic_hook::set_once();

    // Initialize logger
    wasm_logger::init(wasm_logger::Config::default());

    log::info!("YouTube Sentinel UI starting...");

    // Start the Yew application
    Renderer::<App>::new().render();
}

// Re-export commonly used types
pub use app::App as SentinelApp;
pub use models::ui::*;
pub use services::api::ApiClient;
