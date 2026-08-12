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
    Router,
};
use flowscope_cli::fix::{apply_lint_fixes_with_runtime_options, LintFixRuntimeOptions};
use flowscope_cli::server::{build_router, state::AppState, state::ServerConfig};
use flowscope_core::{Dialect, FileSource, LintConfig};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tower::ServiceExt;

/// Representative query used to validate safe-vs-unsafe fix behavior.
const SQL_UNSAFE_FIX_REPRESENTATIVE: &str =
    "SELECT t.id\nFROM t\nINNER JOIN (\n    SELECT id\n    FROM u\n) AS u2 ON t.id = u2.id\n";

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

async fn post_json(app: &Router, path: &str, payload: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (status, json)
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

#[tokio::test]
async fn analyze_enables_linting_for_file_payload_when_requested() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/analyze",
        json!({
            "sql": "",
            "files": [{
                "name": "query.sql",
                "content": "SELECT 1 UNION SELECT 2"
            }],
            "enable_linting": true
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "LINT_AM_002"));
}

#[tokio::test]
async fn analyze_keeps_linting_disabled_when_requested() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/analyze",
        json!({
            "sql": "SELECT 1 UNION SELECT 2",
            "enable_linting": false
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"]
            .as_str()
            .is_some_and(|code| code.starts_with("LINT_"))));
}

#[tokio::test]
async fn analyze_does_not_enable_linting_when_omitted() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/analyze",
        json!({
            "sql": "SELECT 1 UNION SELECT 2"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"]
            .as_str()
            .is_some_and(|code| code.starts_with("LINT_"))));
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

// === Lint fix endpoint tests ===

#[tokio::test]
async fn lint_fix_matches_shared_runtime_pipeline_for_cascading_sql() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);
    let sql = "SELECT a +\n b FROM t";

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let expected = apply_lint_fixes_with_runtime_options(
        sql,
        Dialect::Generic,
        &LintConfig {
            enabled: true,
            disabled_rules: vec![],
            rule_configs: std::collections::BTreeMap::new(),
        },
        LintFixRuntimeOptions {
            include_unsafe_fixes: false,
            legacy_ast_fixes: false,
        },
    )
    .expect("runtime lint-fix result");

    assert_eq!(json["sql"], expected.outcome.sql);
    assert_eq!(json["changed"], expected.outcome.changed);
    assert_eq!(json["fix_counts"]["total"], expected.outcome.counts.total());
    assert_eq!(
        json["skipped_counts"]["unsafe_skipped"],
        expected.candidate_stats.blocked_unsafe
    );
    assert_eq!(
        json["skipped_counts"]["protected_range_blocked"],
        expected.candidate_stats.blocked_protected_range
    );
    assert_eq!(
        json["skipped_counts"]["overlap_conflict_blocked"],
        expected.candidate_stats.blocked_overlap_conflict
    );
    assert_eq!(
        json["skipped_counts"]["display_only"],
        expected.candidate_stats.blocked_display_only
    );
    assert_eq!(
        json["skipped_counts"]["blocked_total"],
        expected.candidate_stats.blocked
    );
}

#[tokio::test]
async fn lint_fix_applies_safe_fix_and_reports_counts() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT COUNT (1) FROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(json["skipped_due_to_regression"], false);
    assert!(json["fix_counts"]["total"].as_u64().unwrap() > 0);

    let fixed_sql = json["sql"].as_str().unwrap().to_ascii_uppercase();
    assert!(
        fixed_sql.contains("COUNT(*)") || fixed_sql.contains("COUNT (*)"),
        "fixed SQL was: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_rule_config_enables_cv006_core_autofix() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1",
            "rule_configs": {
                "convention.terminator": {
                    "require_final_semicolon": true
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.trim_end().ends_with(';'),
        "expected CV06 core autofix to append final semicolon: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv006_spacing_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a FROM foo  ;"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a FROM foo;",
        "expected CV06 core autofix to remove whitespace before semicolon"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv002_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT IFNULL(foo, 0) FROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap().to_ascii_uppercase();
    assert!(
        fixed_sql.contains("COALESCE"),
        "expected CV02 core autofix to rewrite IFNULL: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv001_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT * FROM t WHERE a <> b AND c != d"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    let compact: String = fixed_sql.chars().filter(|ch| !ch.is_whitespace()).collect();
    let has_c_style = compact.contains("a!=b") && compact.contains("c!=d");
    let has_ansi_style = compact.contains("a<>b") && compact.contains("c<>d");
    assert!(
        has_c_style || has_ansi_style,
        "expected CV01 core autofix to normalize not-equal style: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv005_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT * FROM t WHERE a = NULL AND b <> NULL"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.contains("a IS NULL"),
        "expected CV05 core autofix to rewrite '= NULL': {fixed_sql}"
    );
    assert!(
        fixed_sql.contains("b IS NOT NULL"),
        "expected CV05 core autofix to rewrite '<> NULL': {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv007_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "(SELECT 1)"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1\n",
        "expected CV07 core autofix to remove outer wrapper brackets"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt006_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT COUNT (1) FROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.contains("COUNT("),
        "expected LT06 core autofix to keep function call parenthesis tight: {fixed_sql}"
    );
    assert!(
        !fixed_sql.contains("COUNT ("),
        "expected LT06 core autofix to remove spacing before function parenthesis: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt005_core_autofix_in_patch_mode_with_config() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);
    let sql = format!("SELECT {} FROM t\n", vec!["column_name"; 60].join(" "));

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql,
            "rule_configs": {
                "layout.long_lines": {
                    "max_line_length": 300
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.lines().all(|line| line.len() <= 300),
        "expected LT005 core autofix output lines to stay under configured threshold: {fixed_sql:?}"
    );
    assert!(
        fixed_sql.lines().count() > 1,
        "expected LT005 core autofix to split an extremely long line: {fixed_sql:?}"
    );
}

#[tokio::test]
async fn lint_fix_applies_al005_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT users.name FROM users AS u JOIN orders AS o ON users.id = orders.user_id\n",
            "disabled_rules": ["LINT_AM_005"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT users.name FROM users JOIN orders ON users.id = orders.user_id\n",
        "expected AL005 core autofix to remove unused table aliases"
    );
}

#[tokio::test]
async fn lint_fix_applies_al001_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT u.id FROM users u\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT u.id FROM users AS u\n",
        "expected AL001 core autofix to insert explicit AS for table aliases"
    );
}

#[tokio::test]
async fn lint_fix_applies_al009_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a AS a FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a FROM t\n",
        "expected AL009 core autofix to remove self-aliasing projection alias"
    );
}

#[tokio::test]
async fn lint_fix_applies_st001_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT CASE WHEN x > 1 THEN 'a' ELSE NULL END FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT CASE WHEN x > 1 THEN 'a' END FROM t\n",
        "expected ST001 core autofix to remove redundant ELSE NULL branch"
    );
}

#[tokio::test]
async fn lint_fix_applies_am001_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT DISTINCT a FROM t GROUP BY a\n",
            "disabled_rules": ["LINT_LT_014"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a FROM t GROUP BY a\n",
        "expected AM001 core autofix to remove DISTINCT when GROUP BY is present"
    );
}

#[tokio::test]
async fn lint_fix_applies_am002_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1 UNION SELECT 2\n",
            "disabled_rules": ["LINT_LT_011"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1 UNION DISTINCT SELECT 2\n",
        "expected AM002 core autofix to expand bare UNION to explicit UNION DISTINCT"
    );
}

#[tokio::test]
async fn lint_fix_applies_am003_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT * FROM t ORDER BY a DESC, b NULLS LAST\n",
            "disabled_rules": ["LINT_LT_014"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT * FROM t ORDER BY a DESC, b ASC NULLS LAST\n",
        "expected AM003 core autofix to add ASC to implicit ORDER BY terms in mixed clauses"
    );
}

#[tokio::test]
async fn lint_fix_applies_am005_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a FROM t JOIN u ON t.id = u.id\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a FROM t INNER JOIN u ON t.id = u.id\n",
        "expected AM005 core autofix to qualify bare JOIN with INNER"
    );
}

#[tokio::test]
async fn lint_fix_applies_am008_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT foo.a, bar.b FROM foo INNER JOIN bar\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT\n    foo.a,\n    bar.b\nFROM foo CROSS JOIN bar\n",
        "expected AM008 core autofix to rewrite conditionless INNER JOIN to CROSS JOIN"
    );
}

#[tokio::test]
async fn lint_fix_applies_st006_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a + 1, a FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT\n    a,\n    a + 1\nFROM t\n",
        "expected ST006 core autofix to reorder simple projection targets before complex expressions"
    );
}

#[tokio::test]
async fn lint_fix_applies_st009_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT foo.a, bar.b FROM foo LEFT JOIN bar ON bar.a = foo.a\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT\n    foo.a,\n    bar.b\nFROM foo LEFT JOIN bar ON foo.a = bar.a\n",
        "expected ST009 core autofix to reorder join predicate source sides"
    );
}

#[tokio::test]
async fn lint_fix_applies_st002_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT CASE WHEN x > 0 THEN true ELSE false END FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT coalesce(x > 0, false) FROM t\n",
        "expected ST002 core autofix to rewrite unnecessary CASE to coalesce"
    );
}

#[tokio::test]
async fn lint_fix_applies_st008_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT DISTINCT(a) FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT DISTINCT a FROM t\n",
        "expected ST008 core autofix to remove DISTINCT parentheses"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt004_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a,b FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert!(
        !json["sql"].as_str().unwrap().contains("a,b"),
        "expected LT004 core autofix to enforce spacing after comma"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt003_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a +\n b FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a\n    + b FROM t\n",
        "expected LT003 core autofix to move trailing operator to leading style"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt001_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT payload->>'id' FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT payload ->> 'id' FROM t\n",
        "expected LT001 core autofix to normalize json arrow spacing"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt002_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1\n   -- comment\nFROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], false);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1\n   -- comment\nFROM t\n",
        "expected LT002 core autofix to normalize comment-line indentation"
    );
}

#[tokio::test]
async fn lint_fix_applies_tq003_core_autofix_in_patch_mode() {
    let mut config = default_config();
    config.dialect = Dialect::Mssql;
    let state = test_state(config, vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1\nGO\nGO\nSELECT 2\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1\nGO\nSELECT 2\n",
        "expected TQ003 core autofix to collapse redundant GO batch separators"
    );
}

#[tokio::test]
async fn lint_fix_applies_tq002_core_autofix_in_patch_mode() {
    let mut config = default_config();
    config.dialect = Dialect::Mssql;
    let state = test_state(config, vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "CREATE PROCEDURE p AS SELECT 1; SELECT 2;\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap().to_ascii_uppercase();
    assert!(
        fixed_sql.contains(" AS BEGIN "),
        "expected TQ002 core autofix to insert BEGIN: {fixed_sql}"
    );
    assert!(
        fixed_sql.contains(" END"),
        "expected TQ002 core autofix to insert END: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_applies_st005_core_autofix_in_unsafe_mode_with_from_config() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT * FROM (SELECT 1) sub\n",
            "unsafe_fixes": true,
            "rule_configs": {
                "structure.subquery": {
                    "forbid_subquery_in": "from"
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "WITH sub AS (SELECT 1)\n\nSELECT * FROM sub\n",
        "expected unsafe ST005 core autofix to rewrite FROM subquery to CTE"
    );
}

#[tokio::test]
async fn lint_fix_applies_cp001_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a from t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a FROM t\n",
        "expected CP001 core autofix to normalize keyword capitalisation"
    );
}

#[tokio::test]
async fn lint_fix_applies_cp003_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT COUNT(*), sum(a) FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT\n    COUNT(*),\n    SUM(a)\nFROM t\n",
        "expected CP003 core autofix to normalize function capitalisation"
    );
}

#[tokio::test]
async fn lint_fix_applies_cp002_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT Col, col FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    // Col refutes lower/upper, leaving capitalise. col then violates capitalise.
    // Consistent policy resolves to capitalise, fixing all identifiers.
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT\n    Col,\n    Col\nFROM T\n",
        "expected CP002 core autofix to normalize identifier capitalisation"
    );
}

#[tokio::test]
async fn lint_fix_applies_cp004_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT NULL, true FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT\n    NULL,\n    TRUE\nFROM t\n",
        "expected CP004 core autofix to normalize literal capitalisation"
    );
}

#[tokio::test]
async fn lint_fix_applies_cp005_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "CREATE TABLE t (a INT, b varchar(10))\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "CREATE TABLE t (a INT, b VARCHAR(10))\n",
        "expected CP005 core autofix to normalize type capitalisation"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv010_core_autofix_in_patch_mode() {
    // CV10 only fires in dialects where both single/double quotes are strings.
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 'abc', \"def\"\n",
            "dialect": "Bigquery",
            "unsafe_fixes": true
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed = json["sql"].as_str().unwrap();
    assert!(
        !fixed.contains("\"def\""),
        "expected CV010 core autofix to convert double-quoted string to single-quoted: {fixed}"
    );
}

#[tokio::test]
async fn lint_fix_applies_rf004_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "select a from users as select\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed = json["sql"].as_str().unwrap();
    // Multi-pass: RF004 renames the keyword alias, then a subsequent pass
    // removes the now-unreferenced alias entirely.
    assert!(
        !fixed.contains(" as select") && !fixed.contains(" AS select"),
        "expected RF004 core autofix to eliminate keyword table alias: {fixed}"
    );
}

#[tokio::test]
async fn lint_fix_applies_rf003_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "select a.id, id2 from a\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "select\n    a.id,\n    a.id2\nfrom a\n",
        "expected RF003 core autofix to qualify unqualified references consistently"
    );
}

#[tokio::test]
async fn lint_fix_applies_rf006_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT \"good_name\" FROM t\n"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT good_name FROM t\n",
        "expected RF006 core autofix to unquote safe identifiers"
    );
}

#[tokio::test]
async fn lint_fix_applies_cv003_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a, FROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a FROM t\n",
        "expected CV003 core autofix to remove trailing comma"
    );
}

#[tokio::test]
async fn lint_fix_rule_config_enables_cv003_require_core_autofix() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a FROM t",
            "rule_configs": {
                "convention.select_trailing_comma": {
                    "select_clause_trailing_comma": "require"
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT a, FROM t\n",
        "expected CV003 require-mode core autofix to insert trailing comma"
    );
}

#[tokio::test]
async fn lint_fix_applies_st012_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1;;"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1;",
        "expected ST012 core autofix to collapse consecutive semicolons"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt012_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1\nFROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1\nFROM t\n",
        "expected LT012 core autofix to enforce exactly one trailing newline"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt013_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "\n\nSELECT 1"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.starts_with("SELECT 1"),
        "expected LT013 core autofix to remove leading blank lines: {fixed_sql:?}"
    );
    assert!(
        !fixed_sql.starts_with('\n'),
        "expected LT013 output to start with SQL text: {fixed_sql:?}"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt014_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT a FROM t\nWHERE a = 1"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.contains("\nFROM t"),
        "expected LT014 core autofix to line-break major clause keyword: {fixed_sql:?}"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt010_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT\nDISTINCT a\nFROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT DISTINCT a\nFROM t\n",
        "expected LT010 core autofix to place DISTINCT on SELECT line"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt011_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1 UNION SELECT 2\nUNION SELECT 3",
            "disabled_rules": ["LINT_AM_002"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1\nUNION\nSELECT 2\nUNION\nSELECT 3\n",
        "expected LT011 core autofix to put set operators on their own line"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt007_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "WITH cte AS (\n  SELECT 1)\nSELECT * FROM cte"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "WITH cte AS (\n    SELECT 1\n)\n\nSELECT * FROM cte\n",
        "expected LT007 core autofix output in patch mode"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt009_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "select\n  a,\n  b,\n  c from x",
            "disabled_rules": ["LINT_LT_014"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "select\n    a,\n    b,\n    c\nfrom x\n",
        "expected LT009 core autofix to place FROM on a new line after final target"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt008_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "WITH cte AS (SELECT 1) SELECT * FROM cte"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "WITH cte AS (SELECT 1)\n\nSELECT * FROM cte\n",
        "expected LT008 core autofix to place SELECT on a new line after CTE close"
    );
}

#[tokio::test]
async fn lint_fix_applies_lt015_core_autofix_in_patch_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "SELECT 1\n\n\nFROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["changed"], true);
    assert_eq!(
        json["sql"].as_str().unwrap(),
        "SELECT 1\n\nFROM t\n",
        "expected LT015 core autofix to collapse excessive blank lines"
    );
}

#[tokio::test]
async fn lint_fix_applies_jj001_core_autofix_only_in_unsafe_mode() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);
    let sql = "SELECT '{{foo}}' AS templated\n";

    let (safe_status, safe_json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql,
            "unsafe_fixes": false
        }),
    )
    .await;
    let (unsafe_status, unsafe_json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql,
            "unsafe_fixes": true
        }),
    )
    .await;

    assert_eq!(safe_status, StatusCode::OK);
    assert_eq!(unsafe_status, StatusCode::OK);
    assert_eq!(
        safe_json["sql"].as_str().unwrap(),
        sql,
        "safe mode should keep JJ001 template edits blocked by protected ranges"
    );
    assert!(
        unsafe_json["sql"].as_str().unwrap().contains("{{ foo }}"),
        "unsafe mode should apply JJ001 core autofix: {}",
        unsafe_json["sql"].as_str().unwrap()
    );
}

#[tokio::test]
async fn lint_fix_safe_vs_unsafe_mode_shows_expected_delta() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);
    let sql = SQL_UNSAFE_FIX_REPRESENTATIVE;

    let (safe_status, safe_json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql,
            "unsafe_fixes": false,
            "legacy_ast_fixes": true
        }),
    )
    .await;
    let (unsafe_status, unsafe_json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql,
            "unsafe_fixes": true,
            "legacy_ast_fixes": true
        }),
    )
    .await;

    assert_eq!(safe_status, StatusCode::OK);
    assert_eq!(unsafe_status, StatusCode::OK);

    let safe_sql = safe_json["sql"].as_str().unwrap().to_ascii_uppercase();
    let unsafe_sql = unsafe_json["sql"].as_str().unwrap().to_ascii_uppercase();

    assert!(
        !safe_sql.starts_with("WITH "),
        "safe mode should not apply ST_005 rewrite: {safe_sql}"
    );
    assert!(
        unsafe_sql.starts_with("WITH "),
        "unsafe mode with legacy AST rewrites should apply ST_005 rewrite: {unsafe_sql}"
    );
    assert!(
        safe_json["skipped_counts"]["unsafe_skipped"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(unsafe_json["skipped_counts"]["unsafe_skipped"], 0);
}

#[tokio::test]
async fn lint_fix_unsafe_without_legacy_ast_rewrites_keeps_st05_shape() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);
    let sql = SQL_UNSAFE_FIX_REPRESENTATIVE;

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": sql,
            "unsafe_fixes": true
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let fixed_sql = json["sql"].as_str().unwrap().to_ascii_uppercase();
    assert!(
        !fixed_sql.contains("WITH SUB AS"),
        "unsafe patch mode without legacy AST rewrites should not emit ST_005 CTE rewrite: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_preserves_comments_while_fixing_non_comment_regions() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": "-- keep this comment\nSELECT COUNT (1) FROM t"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["skipped_due_to_comments"], false);

    let fixed_sql = json["sql"].as_str().unwrap();
    assert!(
        fixed_sql.contains("-- keep this comment"),
        "comment must be preserved: {fixed_sql}"
    );
    assert!(
        fixed_sql.to_ascii_uppercase().contains("COUNT (*)")
            || fixed_sql.to_ascii_uppercase().contains("COUNT(*)"),
        "non-comment fix should still apply: {fixed_sql}"
    );
}

#[tokio::test]
async fn lint_fix_respects_disabled_rules() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let (status, json) = post_json(
        &app,
        "/api/lint-fix",
        json!({
            "sql": SQL_UNSAFE_FIX_REPRESENTATIVE,
            "unsafe_fixes": true,
            "legacy_ast_fixes": true,
            "disabled_rules": ["LINT_ST_005"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!json["sql"]
        .as_str()
        .unwrap()
        .to_ascii_uppercase()
        .contains("WITH SUB AS"));
    assert_eq!(json["skipped_counts"]["unsafe_skipped"], 0);
}

#[tokio::test]
async fn lint_fix_rejects_non_object_rule_config_entries() {
    let state = test_state(default_config(), vec![]);
    let app = build_router(state, 3000);

    let response = app
        .oneshot(
            Request::post("/api/lint-fix")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "sql": "SELECT 1",
                        "rule_configs": {
                            "structure.subquery": "both"
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let message = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        message.contains("'rule_configs' entry for 'structure.subquery' must be a JSON object"),
        "unexpected error message: {message}"
    );
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
