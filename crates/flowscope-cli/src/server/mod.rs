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
use tower_http::limit::RequestBodyLimitLayer;

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

// =============================================================================
// Server Configuration Constants
// =============================================================================
// These limits are chosen to balance usability with resource protection.
// Adjust based on expected workload and available system resources.

/// Maximum request body size (100MB).
///
/// This limit accommodates multi-file analysis requests while preventing
/// denial-of-service attacks via large payloads. 100MB allows ~10,000 files
/// at 10KB average, matching MAX_TOTAL_FILES.
const MAX_REQUEST_BODY_SIZE: usize = 100 * 1024 * 1024;

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
        .layer(RequestBodyLimitLayer::new(MAX_REQUEST_BODY_SIZE))
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM on Unix).
///
/// Handles both SIGINT (Ctrl+C) and SIGTERM for graceful shutdown.
/// SIGTERM is important for containerized/managed environments (Docker, systemd, Kubernetes).
///
/// Signal handler registration failures are logged but don't crash the server,
/// allowing operation in restricted environments where some signals may be unavailable.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            eprintln!("flowscope: warning: failed to install Ctrl+C handler: {e}");
            // Fall back to pending - server will need to be killed externally
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                eprintln!("flowscope: warning: failed to install SIGTERM handler: {e}");
                // Fall back to pending - Ctrl+C will still work
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Maximum file size to read (10MB).
///
/// SQL files larger than this are skipped with a warning. 10MB is generous
/// for SQL (most files are under 100KB) while preventing accidental inclusion
/// of large data dumps or generated files.
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of SQL files to load (10,000).
///
/// Prevents memory exhaustion in very large monorepos. At 10KB average per file,
/// this allows ~100MB of SQL content in memory. Increase if working with larger
/// projects, but monitor memory usage.
const MAX_TOTAL_FILES: usize = 10_000;

/// Scan directories for SQL files, returning both file contents and modification times.
pub fn scan_sql_files(
    dirs: &[PathBuf],
) -> Result<(
    Vec<flowscope_core::FileSource>,
    std::collections::HashMap<PathBuf, std::time::SystemTime>,
)> {
    use std::fs;

    let mut sources = Vec::new();
    let mut mtimes = std::collections::HashMap::new();

    // Pre-compute readable prefixes for each watch directory so files with the same
    // relative path coming from different roots stay unique in the UI.
    let mut base_labels = Vec::with_capacity(dirs.len());
    for dir in dirs {
        let base = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| dir.display().to_string());
        base_labels.push(base);
    }

    let mut label_counts = std::collections::HashMap::new();
    for base in &base_labels {
        *label_counts.entry(base.clone()).or_insert(0) += 1;
    }

    let multi_root = dirs.len() > 1;
    let mut seen_counts = std::collections::HashMap::new();
    let dir_prefixes: Vec<Option<String>> = base_labels
        .iter()
        .map(|base| {
            if !multi_root {
                return None;
            }

            let total = label_counts.get(base).copied().unwrap_or(1);
            if total == 1 {
                return Some(base.clone());
            }

            let entry = seen_counts.entry(base.clone()).or_insert(0);
            *entry += 1;
            if *entry == 1 {
                Some(base.clone())
            } else {
                Some(format!("{base}#{}", *entry))
            }
        })
        .collect();

    for (dir, prefix) in dirs.iter().zip(dir_prefixes.iter()) {
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
                // Check file size before reading to prevent memory exhaustion
                let metadata = fs::metadata(path)
                    .with_context(|| format!("Failed to read metadata for {}", path.display()))?;

                if metadata.len() > MAX_FILE_SIZE {
                    eprintln!(
                        "flowscope: warning: skipping large file (>10MB): {}",
                        path.display()
                    );
                    continue;
                }

                let content = fs::read_to_string(path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;

                // Ensure path is within watch directory - error instead of falling back
                // to prevent exposing absolute paths via the API
                let relative_path = path
                    .strip_prefix(dir)
                    .with_context(|| format!("File outside watch directory: {}", path.display()))?;

                let relative_str = relative_path.to_string_lossy();
                let name = if let Some(prefix) = prefix {
                    format!("{prefix}/{}", relative_str)
                } else {
                    relative_str.to_string()
                };
                sources.push(flowscope_core::FileSource { name, content });

                // Store mtime for change detection
                if let Ok(mtime) = metadata.modified() {
                    mtimes.insert(path.to_path_buf(), mtime);
                }

                // Limit total files to prevent memory exhaustion
                if sources.len() >= MAX_TOTAL_FILES {
                    eprintln!(
                        "flowscope: warning: reached file limit ({}), skipping remaining files",
                        MAX_TOTAL_FILES
                    );
                    return Ok((sources, mtimes));
                }
            }
        }
    }

    Ok((sources, mtimes))
}
