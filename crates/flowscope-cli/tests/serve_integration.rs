//! Integration tests for serve mode.
//!
//! These tests spawn an actual HTTP server and make real HTTP requests
//! to verify the full request/response cycle.

#![cfg(feature = "serve")]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use flowscope_cli::server::{build_router, scan_sql_files, state::AppState, state::ServerConfig};
use flowscope_core::{Dialect, FileSource};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Find an available port for testing.
fn get_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Create a test AppState directly without disk I/O.
fn test_state_from_files(config: ServerConfig, files: Vec<FileSource>) -> Arc<AppState> {
    Arc::new(AppState {
        config,
        files: RwLock::new(files),
        schema: RwLock::new(None),
        mtimes: RwLock::new(HashMap::new()),
    })
}

/// Spawn a test server and return the base URL.
async fn spawn_test_server(
    config: ServerConfig,
    files: Vec<FileSource>,
) -> (String, tokio::task::JoinHandle<()>) {
    let port = config.port;
    let state = test_state_from_files(config, files);
    let app = build_router(state, port);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    (format!("http://127.0.0.1:{}", port), handle)
}

// === Integration test: Server startup and health endpoint ===

#[tokio::test]
async fn server_starts_and_responds_to_health() {
    let port = get_available_port();
    let config = ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let (base_url, server_handle) = spawn_test_server(config, vec![]).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/health", base_url))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());

    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());

    server_handle.abort();
}

// === Integration test: Analyze endpoint with sample SQL ===

#[tokio::test]
async fn analyze_endpoint_processes_complex_query() {
    let port = get_available_port();
    let config = ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let (base_url, server_handle) = spawn_test_server(config, vec![]).await;

    let client = reqwest::Client::new();

    // Test with a complex query involving joins and subqueries
    let response = client
        .post(format!("{}/api/analyze", base_url))
        .json(&json!({
            "sql": r#"
                WITH active_users AS (
                    SELECT user_id, name
                    FROM users
                    WHERE status = 'active'
                )
                SELECT
                    au.name,
                    COUNT(o.id) as order_count,
                    SUM(o.total) as total_spent
                FROM active_users au
                LEFT JOIN orders o ON au.user_id = o.user_id
                GROUP BY au.name
            "#
        }))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());

    let body: Value = response.json().await.unwrap();

    // Verify the analysis result structure
    assert!(body["statements"].is_array());
    let statements = body["statements"].as_array().unwrap();
    assert!(!statements.is_empty());

    // The result should contain lineage information (nodes and edges arrays)
    let first_stmt = &statements[0];
    assert!(first_stmt["nodes"].is_array());
    assert!(first_stmt["edges"].is_array());

    server_handle.abort();
}

#[tokio::test]
async fn analyze_endpoint_with_multiple_files() {
    let port = get_available_port();
    let config = ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let files = vec![
        FileSource {
            name: "views/user_summary.sql".to_string(),
            content: "CREATE VIEW user_summary AS SELECT id, name FROM users".to_string(),
        },
        FileSource {
            name: "queries/report.sql".to_string(),
            content: "SELECT * FROM user_summary".to_string(),
        },
    ];

    let (base_url, server_handle) = spawn_test_server(config, files.clone()).await;

    let client = reqwest::Client::new();

    // Analyze with files from state
    let response = client
        .post(format!("{}/api/analyze", base_url))
        .json(&json!({
            "sql": "SELECT name FROM user_summary",
            "files": files
        }))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());

    let body: Value = response.json().await.unwrap();
    assert!(body["statements"].is_array());

    server_handle.abort();
}

// === Integration test: File watcher triggers update ===

#[tokio::test]
async fn file_watcher_detects_sql_files() {
    // Create a temp directory with SQL files
    let temp_dir = TempDir::new().unwrap();
    let sql_file = temp_dir.path().join("test.sql");
    std::fs::write(&sql_file, "SELECT 1").unwrap();

    // Verify scan_sql_files works
    let (files, _mtimes) = scan_sql_files(&[temp_dir.path().to_path_buf()]).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "test.sql");
    assert_eq!(files[0].content, "SELECT 1");
}

#[tokio::test]
async fn file_watcher_filters_non_sql_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create both SQL and non-SQL files
    std::fs::write(temp_dir.path().join("query.sql"), "SELECT 1").unwrap();
    std::fs::write(temp_dir.path().join("readme.md"), "# Docs").unwrap();
    std::fs::write(temp_dir.path().join("config.json"), "{}").unwrap();

    let (files, _mtimes) = scan_sql_files(&[temp_dir.path().to_path_buf()]).unwrap();

    // Only SQL files should be included
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "query.sql");
}

#[tokio::test]
async fn file_watcher_handles_nested_directories() {
    let temp_dir = TempDir::new().unwrap();

    // Create nested directory structure
    let subdir = temp_dir.path().join("views");
    std::fs::create_dir(&subdir).unwrap();

    std::fs::write(temp_dir.path().join("main.sql"), "SELECT 1").unwrap();
    std::fs::write(subdir.join("user_view.sql"), "SELECT 2").unwrap();

    let (files, _mtimes) = scan_sql_files(&[temp_dir.path().to_path_buf()]).unwrap();

    assert_eq!(files.len(), 2);

    let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"main.sql"));
    assert!(names.contains(&"views/user_view.sql"));
}

#[tokio::test]
async fn scan_sql_files_prefixes_multiple_watch_dirs() {
    let temp_dir = TempDir::new().unwrap();
    let first_dir = temp_dir.path().join("alpha_project");
    let second_dir = temp_dir.path().join("beta_project");
    std::fs::create_dir_all(first_dir.join("models")).unwrap();
    std::fs::create_dir_all(second_dir.join("models")).unwrap();

    std::fs::write(first_dir.join("models/foo.sql"), "SELECT 1").unwrap();
    std::fs::write(second_dir.join("models/foo.sql"), "SELECT 2").unwrap();

    let (files, _mtimes) = scan_sql_files(&[first_dir.clone(), second_dir.clone()]).unwrap();

    assert_eq!(files.len(), 2);
    let mut names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
    names.sort();

    let mut expected = vec![
        "alpha_project/models/foo.sql".to_string(),
        "beta_project/models/foo.sql".to_string(),
    ];
    expected.sort();

    assert_eq!(names, expected);
}

#[tokio::test]
async fn scan_sql_files_disambiguates_duplicate_watch_names() {
    let temp_dir = TempDir::new().unwrap();
    let first_dir = temp_dir.path().join("shared");
    let second_dir = temp_dir.path().join("nested").join("shared");
    std::fs::create_dir_all(first_dir.join("models")).unwrap();
    std::fs::create_dir_all(second_dir.join("models")).unwrap();

    std::fs::write(first_dir.join("models/foo.sql"), "SELECT 1").unwrap();
    std::fs::write(second_dir.join("models/foo.sql"), "SELECT 2").unwrap();

    let (files, _mtimes) = scan_sql_files(&[first_dir.clone(), second_dir.clone()]).unwrap();

    assert_eq!(files.len(), 2);
    let mut names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
    names.sort();

    let mut expected = vec![
        "shared/models/foo.sql".to_string(),
        "shared#2/models/foo.sql".to_string(),
    ];
    expected.sort();

    assert_eq!(names, expected);
}

#[tokio::test]
async fn app_state_reload_updates_files() {
    let temp_dir = TempDir::new().unwrap();
    std::fs::write(temp_dir.path().join("initial.sql"), "SELECT 1").unwrap();

    let config = ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![temp_dir.path().to_path_buf()],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port: 3000,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    // Create state with initial files
    let (files, mtimes) = scan_sql_files(&config.watch_dirs).unwrap();
    let state = Arc::new(AppState {
        config,
        files: RwLock::new(files),
        schema: RwLock::new(None),
        mtimes: RwLock::new(mtimes),
    });

    // Verify initial state
    {
        let files = state.files.read().await;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "initial.sql");
    }

    // Add a new file
    std::fs::write(temp_dir.path().join("new.sql"), "SELECT 2").unwrap();

    // Reload files
    state.reload_files().await.unwrap();

    // Verify updated state
    {
        let files = state.files.read().await;
        assert_eq!(files.len(), 2);
    }
}

// === Export endpoint integration tests ===

#[tokio::test]
async fn export_html_returns_valid_html() {
    let port = get_available_port();
    let config = ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let (base_url, server_handle) = spawn_test_server(config, vec![]).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/export/html", base_url))
        .json(&json!({
            "sql": "SELECT id, name FROM users"
        }))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("content-type").unwrap(), "text/html");

    let body = response.text().await.unwrap();
    assert!(body.contains("<!DOCTYPE html>") || body.contains("<html"));

    server_handle.abort();
}

#[tokio::test]
async fn export_csv_returns_zip() {
    let port = get_available_port();
    let config = ServerConfig {
        dialect: Dialect::Generic,
        watch_dirs: vec![],
        static_files: None,
        metadata_url: None,
        metadata_schema: None,
        port,
        open_browser: false,
        schema_path: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let (base_url, server_handle) = spawn_test_server(config, vec![]).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/export/csv", base_url))
        .json(&json!({
            "sql": "SELECT id FROM users"
        }))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/zip"
    );

    // Verify we got binary data
    let bytes = response.bytes().await.unwrap();
    assert!(!bytes.is_empty());

    server_handle.abort();
}
