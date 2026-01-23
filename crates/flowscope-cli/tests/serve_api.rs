//! Unit tests for serve mode API handlers.
//!
//! These tests verify the API endpoints work correctly with mock state,
//! without starting a full HTTP server.

#![cfg(feature = "serve")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use flowscope_cli::server::{build_router, state::AppState, state::ServerConfig};
use flowscope_core::{Dialect, FileSource};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tower::ServiceExt;

/// Create a test AppState without loading files from disk.
fn test_state(config: ServerConfig, files: Vec<FileSource>) -> Arc<AppState> {
    Arc::new(AppState {
        config,
        files: RwLock::new(files),
        schema: RwLock::new(None),
        mtimes: RwLock::new(HashMap::new()),
    })
}

fn default_config() -> ServerConfig {
    ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port: 3000,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    }
}

// === Health endpoint tests ===

#[tokio::test]
async fn health_returns_ok_status() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
}

// === Analyze endpoint tests ===

#[tokio::test]
async fn analyze_simple_select() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/analyze")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT id, name FROM users"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Check that analysis result has expected structure
    assert!(json["statements"].is_array());
}

#[tokio::test]
async fn analyze_with_join() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/analyze")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Verify we got statements back
    assert!(json["statements"].is_array());
    assert!(!json["statements"].as_array().unwrap().is_empty());
}

// === Completion endpoint tests ===

#[tokio::test]
async fn completion_returns_items() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/completion")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT ",
                        "cursor_offset": 7
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Completion response should have items array
    assert!(json["items"].is_array());
}

// === Split endpoint tests ===

#[tokio::test]
async fn split_multiple_statements() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/split")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT 1; SELECT 2; SELECT 3"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Should have statements array
    assert!(json["statements"].is_array());
    assert_eq!(json["statements"].as_array().unwrap().len(), 3);
}

// === Files endpoint tests ===

#[tokio::test]
async fn files_returns_empty_when_no_files() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(Request::get("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_array());
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn files_returns_loaded_files() {
    let files = vec![
        FileSource {
            name: "queries.sql".to_string(),
            content: "SELECT * FROM users".to_string(),
        },
        FileSource {
            name: "reports/summary.sql".to_string(),
            content: "SELECT COUNT(*) FROM orders".to_string(),
        },
    ];

    let state = test_state(default_config(), files);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(Request::get("/api/files").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);

    let first = &json[0];
    assert_eq!(first["name"], "queries.sql");
    assert_eq!(first["content"], "SELECT * FROM users");
}

// === Schema endpoint tests ===

#[tokio::test]
async fn schema_returns_null_when_no_schema() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(Request::get("/api/schema").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.is_null());
}

// === Config endpoint tests ===

#[tokio::test]
async fn config_returns_server_configuration() {
    let config = ServerConfig {
        dialect: Dialect::Postgres,
        watch_dirs: vec![PathBuf::from("/tmp/sql")],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port: 8080,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let state = test_state(config, vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["dialect"], "Postgres");
    assert!(json["watch_dirs"].is_array());
    assert_eq!(json["has_schema"], false);
}

// === Export endpoint tests ===

#[tokio::test]
async fn export_json_format() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/export/json")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT id FROM users"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
}

#[tokio::test]
async fn export_mermaid_format() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/export/mermaid")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT id FROM users"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/plain"
    );
}

#[tokio::test]
async fn export_unknown_format_returns_error() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/export/unknown")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT id FROM users"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
