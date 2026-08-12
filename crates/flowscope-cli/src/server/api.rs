//! REST API handlers for serve mode.
//!
//! This module provides the API endpoints for the web UI to interact with
//! the FlowScope analysis engine.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::AppState;

/// Build the API router with all endpoints.
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/analyze", post(analyze))
        .route("/completion", post(completion))
        .route("/split", post(split))
        .route("/lint-fix", post(lint_fix))
        .route("/files", get(files))
        .route("/schema", get(schema))
        .route("/export/{format}", post(export))
        .route("/config", get(config))
}

// === Request/Response types ===

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Deserialize)]
struct AnalyzeRequest {
    sql: String,
    #[serde(default)]
    files: Option<Vec<flowscope_core::FileSource>>,
    #[serde(default)]
    hide_ctes: Option<bool>,
    #[serde(default)]
    enable_column_lineage: Option<bool>,
    #[serde(default)]
    enable_linting: Option<bool>,
    #[serde(default)]
    template_mode: Option<String>,
}

#[derive(Deserialize)]
struct CompletionRequest {
    sql: String,
    #[serde(alias = "position")]
    cursor_offset: usize,
}

#[derive(Deserialize)]
struct SplitRequest {
    sql: String,
}

#[derive(Serialize)]
struct ConfigResponse {
    dialect: String,
    watch_dirs: Vec<String>,
    has_schema: bool,
    #[cfg(feature = "templating")]
    template_mode: Option<String>,
}

#[derive(Deserialize)]
struct ExportRequest {
    sql: String,
    #[serde(default)]
    files: Option<Vec<flowscope_core::FileSource>>,
}

#[derive(Deserialize)]
struct LintFixRequest {
    sql: String,
    #[serde(default, alias = "include_unsafe_fixes")]
    unsafe_fixes: bool,
    #[serde(default, alias = "legacyAstFixes")]
    legacy_ast_fixes: bool,
    #[serde(default, alias = "exclude_rules")]
    disabled_rules: Vec<String>,
    #[serde(default)]
    rule_configs: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize)]
struct LintFixResponse {
    sql: String,
    changed: bool,
    fix_counts: LintFixCountsResponse,
    skipped_due_to_comments: bool,
    skipped_due_to_regression: bool,
    skipped_counts: LintFixSkippedCountsResponse,
}

#[derive(Serialize)]
struct LintFixCountsResponse {
    total: usize,
}

#[derive(Serialize)]
struct LintFixSkippedCountsResponse {
    unsafe_skipped: usize,
    protected_range_blocked: usize,
    overlap_conflict_blocked: usize,
    display_only: usize,
    blocked_total: usize,
}

// === Handlers ===

/// GET /api/health - Health check with version
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// POST /api/analyze - Run lineage analysis
async fn analyze(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AnalyzeRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let schema = state.schema.read().await.clone();

    // Build analysis options from request
    let options = if payload.hide_ctes.is_some()
        || payload.enable_column_lineage.is_some()
        || payload.enable_linting.is_some()
    {
        Some(flowscope_core::AnalysisOptions {
            hide_ctes: payload.hide_ctes,
            enable_column_lineage: payload.enable_column_lineage,
            lint: payload
                .enable_linting
                .map(|enabled| flowscope_core::LintConfig {
                    enabled,
                    ..Default::default()
                }),
            ..Default::default()
        })
    } else {
        None
    };

    // Build template config if template mode is specified
    #[cfg(feature = "templating")]
    let template_config = resolve_template_config(payload.template_mode.as_deref(), state.as_ref());

    let request = flowscope_core::AnalyzeRequest {
        sql: payload.sql,
        files: payload.files,
        dialect: state.config.dialect,
        source_name: None,
        options,
        schema,
        #[cfg(feature = "templating")]
        template_config,
    };

    let result = flowscope_core::analyze(&request);
    Ok(Json(result))
}

/// POST /api/completion - Get code completion items
async fn completion(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CompletionRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let schema = state.schema.read().await.clone();

    let request = flowscope_core::CompletionRequest {
        sql: payload.sql,
        cursor_offset: payload.cursor_offset,
        dialect: state.config.dialect,
        schema,
    };

    let result = flowscope_core::completion_items(&request);
    Ok(Json(result))
}

/// POST /api/split - Split SQL into statements
async fn split(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SplitRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let request = flowscope_core::StatementSplitRequest {
        sql: payload.sql,
        dialect: state.config.dialect,
    };

    let result = flowscope_core::split_statements(&request);
    Ok(Json(result))
}

/// POST /api/lint-fix - Apply deterministic lint fixes to SQL text.
async fn lint_fix(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LintFixRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let rule_configs = normalize_rule_configs(payload.rule_configs)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))?;

    let lint_config = flowscope_core::LintConfig {
        enabled: true,
        disabled_rules: payload.disabled_rules,
        rule_configs,
    };

    let execution = crate::fix::apply_lint_fixes_with_runtime_options(
        &payload.sql,
        state.config.dialect,
        &lint_config,
        crate::fix::LintFixRuntimeOptions {
            include_unsafe_fixes: payload.unsafe_fixes,
            legacy_ast_fixes: payload.legacy_ast_fixes,
        },
    )
    .map_err(|err| {
        eprintln!("flowscope: lint-fix failed: {err}");
        (
            StatusCode::BAD_REQUEST,
            "Failed to apply lint fixes".to_string(),
        )
    })?;
    let outcome = execution.outcome;
    let candidate_stats = execution.candidate_stats;

    let skipped_counts = LintFixSkippedCountsResponse {
        unsafe_skipped: candidate_stats.blocked_unsafe,
        protected_range_blocked: candidate_stats.blocked_protected_range,
        overlap_conflict_blocked: candidate_stats.blocked_overlap_conflict,
        display_only: candidate_stats.blocked_display_only,
        blocked_total: candidate_stats.blocked,
    };

    Ok(Json(LintFixResponse {
        sql: outcome.sql,
        changed: outcome.changed,
        fix_counts: LintFixCountsResponse {
            total: outcome.counts.total(),
        },
        skipped_due_to_comments: outcome.skipped_due_to_comments,
        skipped_due_to_regression: outcome.skipped_due_to_regression,
        skipped_counts,
    }))
}

/// GET /api/files - List watched files with content
async fn files(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let files = state.files.read().await;
    Json(files.clone())
}

/// GET /api/schema - Get schema metadata
async fn schema(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let schema = state.schema.read().await;
    Json(schema.clone())
}

/// POST /api/export/:format - Export to specified format
async fn export(
    State(state): State<Arc<AppState>>,
    Path(format): Path<String>,
    Json(payload): Json<ExportRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let schema = state.schema.read().await.clone();

    let request = flowscope_core::AnalyzeRequest {
        sql: payload.sql,
        files: payload.files,
        dialect: state.config.dialect,
        source_name: None,
        options: None,
        schema,
        #[cfg(feature = "templating")]
        template_config: state.config.template_config.clone(),
    };

    let result = flowscope_core::analyze(&request);

    match format.as_str() {
        "json" => {
            let output = flowscope_export::export_json(&result, false)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                output,
            )
                .into_response())
        }
        "mermaid" => {
            let output =
                flowscope_export::export_mermaid(&result, flowscope_export::MermaidView::Table)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(([(axum::http::header::CONTENT_TYPE, "text/plain")], output).into_response())
        }
        "html" => {
            let output = flowscope_export::export_html(&result, "lineage", chrono::Utc::now())
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(([(axum::http::header::CONTENT_TYPE, "text/html")], output).into_response())
        }
        "csv" => {
            let bytes = flowscope_export::export_csv_bundle(&result)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((
                [(axum::http::header::CONTENT_TYPE, "application/zip")],
                bytes,
            )
                .into_response())
        }
        "xlsx" => {
            let bytes = flowscope_export::export_xlsx(&result)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok((
                [(
                    axum::http::header::CONTENT_TYPE,
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                )],
                bytes,
            )
                .into_response())
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown export format: {format}"),
        )),
    }
}

/// GET /api/config - Get server configuration
async fn config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let has_schema = state.schema.read().await.is_some();

    Json(ConfigResponse {
        dialect: format!("{:?}", state.config.dialect),
        watch_dirs: state
            .config
            .watch_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        has_schema,
        #[cfg(feature = "templating")]
        template_mode: state
            .config
            .template_config
            .as_ref()
            .map(|cfg| template_mode_to_str(cfg.mode).to_string()),
    })
}

fn normalize_rule_configs(
    raw_configs: BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let mut rule_configs = BTreeMap::new();
    let mut indentation_legacy = serde_json::Map::new();

    for (rule_ref, options) in raw_configs {
        if options.is_object() {
            rule_configs.insert(rule_ref, options);
            continue;
        }

        // SQLFluff compatibility: support legacy indentation keys at root.
        if matches!(
            rule_ref.to_ascii_lowercase().as_str(),
            "indent_unit" | "tab_space_size" | "indented_joins" | "indented_using_on"
        ) {
            indentation_legacy.insert(rule_ref, options);
            continue;
        }

        return Err(format!(
            "'rule_configs' entry for '{rule_ref}' must be a JSON object"
        ));
    }

    if !indentation_legacy.is_empty() {
        let merged = match rule_configs.remove("indentation") {
            Some(serde_json::Value::Object(existing)) => {
                let mut merged = existing;
                for (key, value) in indentation_legacy {
                    merged.insert(key, value);
                }
                merged
            }
            Some(other) => {
                return Err(format!(
                    "'rule_configs' entry for 'indentation' must be a JSON object, found {other}"
                ));
            }
            None => indentation_legacy,
        };

        rule_configs.insert("indentation".to_string(), serde_json::Value::Object(merged));
    }

    Ok(rule_configs)
}

#[cfg(feature = "templating")]
fn resolve_template_config(
    mode: Option<&str>,
    state: &AppState,
) -> Option<flowscope_core::TemplateConfig> {
    match mode {
        Some("raw") => None,
        Some("jinja") => Some(build_template_config(
            flowscope_core::TemplateMode::Jinja,
            state,
        )),
        Some("dbt") => Some(build_template_config(
            flowscope_core::TemplateMode::Dbt,
            state,
        )),
        Some(_) => state.config.template_config.clone(),
        None => state.config.template_config.clone(),
    }
}

#[cfg(feature = "templating")]
fn build_template_config(
    template_mode: flowscope_core::TemplateMode,
    state: &AppState,
) -> flowscope_core::TemplateConfig {
    let context = state
        .config
        .template_config
        .as_ref()
        .map(|cfg| cfg.context.clone())
        .unwrap_or_default();

    flowscope_core::TemplateConfig {
        mode: template_mode,
        context,
    }
}

#[cfg(feature = "templating")]
fn template_mode_to_str(mode: flowscope_core::TemplateMode) -> &'static str {
    match mode {
        flowscope_core::TemplateMode::Raw => "raw",
        flowscope_core::TemplateMode::Jinja => "jinja",
        flowscope_core::TemplateMode::Dbt => "dbt",
    }
}
