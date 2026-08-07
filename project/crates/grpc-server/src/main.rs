//! Sentinel gRPC server entry point.
//!
//! Initialises tracing, loads configuration, creates the `SentinelServer`
//! state object, wires all tonic services, and starts the gRPC server on
//! port 50051 (configurable via `SENTINEL_BIND_ADDR`).
//!
//! Graceful shutdown is handled via `SIGTERM` / `SIGINT` signal handlers.

use anyhow::{Context, Result};
use grpc_server::analysis_service::AnalysisService;
use grpc_server::audio_service::AudioAnalysisService;
use grpc_server::frame_service::FrameExtractionService;
use grpc_server::server::{SentinelServer, ServerConfig};
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::signal;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // -- Tracing subscriber --------------------------------------------------
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("YouTube Sentinel gRPC Server starting...");

    // -- Load configuration --------------------------------------------------
    let config = ServerConfig::from_env()
        .context("Failed to load server configuration from environment")?;

    info!(bind_addr = %config.bind_addr, "Configuration loaded");

    // -- Initialise SentinelServer -------------------------------------------
    let server = SentinelServer::new(config.clone())
        .context("Failed to initialise SentinelServer subsystems")?;

    info!("All subsystems initialised successfully");

    // -- Build tonic services ------------------------------------------------
    let analysis_svc = AnalysisService::new(server.clone());
    let frame_svc = FrameExtractionService::new(server.clone());
    let audio_svc = AudioAnalysisService::new(server.clone());

    // -- Build tonic transport -----------------------------------------------
    let addr = SocketAddr::from_str(&config.bind_addr)
        .with_context(|| format!("Invalid bind address: {}", config.bind_addr))?;

    info!(%addr, "Starting gRPC server");

    // Layer tower middleware (tracing + CORS).
    let layer = tower::ServiceBuilder::new()
        .layer(tower_http::trace::TraceLayer::new_for_grpc())
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any),
        )
        .into_inner();

    // Build routes from all three services.
    let routes = tonic::service::Routes::new(analysis_svc)
        .add_service(frame_svc)
        .add_service(audio_svc);

    // Build and serve – attach graceful shutdown handler.
    let server_handle = tonic::transport::Server::builder()
        .layer(layer)
        .add_routes(routes)
        .serve_with_shutdown(addr, shutdown_signal());

    info!("gRPC server is ready to accept connections");

    if let Err(e) = server_handle.await {
        error!(error = %e, "gRPC server encountered an error");
        return Err(e.into());
    }

    info!("gRPC server shut down gracefully");
    Ok(())
}

// ---------------------------------------------------------------------------
// Graceful shutdown signal
// ---------------------------------------------------------------------------

/// Waits for either `SIGTERM` or `SIGINT` and then returns, causing tonic
/// `serve_with_shutdown` to initiate graceful shutdown.
async fn shutdown_signal() {
    let sigterm = async {
        #[cfg(unix)]
        {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await
                .expect("SIGTERM stream closed");
        }
        #[cfg(not(unix))]
        {
            std::future::pending::<()>().await;
        }
    };

    let sigint = async {
        #[cfg(unix)]
        {
            signal::unix::signal(signal::unix::SignalKind::interrupt())
                .expect("Failed to install SIGINT handler")
                .recv()
                .await
                .expect("SIGINT stream closed");
        }
        #[cfg(not(unix))]
        {
            std::future::pending::<()>().await;
        }
    };

    tokio::select! {
        _ = sigterm => info!("Received SIGTERM, initiating graceful shutdown..."),
        _ = sigint  => info!("Received SIGINT, initiating graceful shutdown..."),
        _ = signal::ctrl_c() => info!("Received Ctrl-C, initiating graceful shutdown..."),
    }
}
