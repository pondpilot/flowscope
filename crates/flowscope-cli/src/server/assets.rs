//! Static asset handling with rust-embed.
//!
//! This module embeds the web UI assets from `app/dist/` and serves them
//! via axum handlers. SPA fallback routes non-asset requests to index.html.

use std::sync::Arc;

use axum::{
    extract::Request,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

use super::AppState;

/// Embedded web UI assets from the embedded-app directory.
///
/// The embedded-app directory is generated from `app/dist` during development
/// and shipped with the published crate so serve mode works out of the box.
#[derive(Embed)]
#[folder = "../../embedded-app/"]
#[include = "*.html"]
#[include = "*.js"]
#[include = "*.css"]
#[include = "*.wasm"]
#[include = "*.svg"]
#[include = "*.png"]
#[include = "*.ico"]
#[include = "*.json"]
#[include = "assets/*"]
struct WebAssets;

/// Handler for serving static files with SPA fallback.
pub async fn static_handler(
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
    request: Request,
) -> Response {
    let path = request.uri().path().trim_start_matches('/');

    // Try to serve the exact path first
    if let Some(content) = <WebAssets as Embed>::get(path) {
        return serve_file(path, content.data.as_ref());
    }

    // For paths without extensions (likely SPA routes), serve index.html
    if !path.contains('.') || path.is_empty() {
        if let Some(content) = <WebAssets as Embed>::get("index.html") {
            return serve_file("index.html", content.data.as_ref());
        }
    }

    // 404 for missing assets
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

/// Serve a file with appropriate content-type header.
fn serve_file(path: &str, data: &[u8]) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime.as_ref())],
        data.to_vec(),
    )
        .into_response()
}
