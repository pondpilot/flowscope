//! HTTP server module for serve mode.
//!
//! This module provides a local HTTP server that serves the embedded web UI
//! and exposes a REST API for SQL lineage analysis.

pub mod api;
mod assets;
pub mod state;
mod watcher;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::cors::CorsLayer;

pub use state::{AppState, ServerConfig};

/// Run the HTTP server with embedded web UI.
///
/// This function blocks until the server is shut down (e.g., via Ctrl+C).
pub async fn run_server(config: ServerConfig) -> Result<()> {
    let state = Arc::new(AppState::new(config.clone()).await?);

    // Start file watcher in background
    let watcher_state = Arc::clone(&state);
    let watcher_handle = tokio::spawn(async move {
        if let Err(e) = watcher::start_watcher(watcher_state).await {
            eprintln!("flowscope: watcher error: {e}");
        }
    });

    let app = build_router(state, config.port);

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));

    // Bind to port first to ensure it's available before opening browser
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind to address")?;

    println!("flowscope: server listening on http://{addr}");

    // Open browser if requested (only after successful bind)
    if config.open_browser {
        let url = format!("http://localhost:{}", config.port);
        if let Err(e) = open::that(&url) {
            eprintln!("flowscope: warning: failed to open browser: {e}");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    watcher_handle.abort();
    println!("\nflowscope: server stopped");

    Ok(())
}

/// Build the main router with all routes.
pub fn build_router(state: Arc<AppState>, port: u16) -> Router {
    // Restrict CORS to same-origin to prevent cross-site requests from reading local files.
    // The server only binds to localhost, but without CORS restrictions any website could
    // make requests to http://127.0.0.1:<port> and read the user's SQL files.
    let allowed_origins = [
        format!("http://localhost:{port}").parse().unwrap(),
        format!("http://127.0.0.1:{port}").parse().unwrap(),
    ];

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    Router::new()
        .nest("/api", api::api_routes())
        .fallback(assets::static_handler)
        .with_state(state)
        .layer(cors)
}

/// Wait for shutdown signal (Ctrl+C).
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
}

/// Scan directories for SQL files.
pub fn scan_sql_files(dirs: &[PathBuf]) -> Result<Vec<flowscope_core::FileSource>> {
    use std::fs;

    let mut sources = Vec::new();

    for dir in dirs {
        if !dir.exists() {
            eprintln!(
                "flowscope: warning: watch directory does not exist: {}",
                dir.display()
            );
            continue;
        }

        // Don't follow symlinks to prevent accessing files outside watched directories.
        // A symlink could point to sensitive files that shouldn't be exposed via the API.
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "sql") {
                let content = fs::read_to_string(path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let name = path
                    .strip_prefix(dir)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();
                sources.push(flowscope_core::FileSource { name, content });
            }
        }
    }

    Ok(sources)
}
