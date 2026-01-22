//! REST API handlers for serve mode.
//!
//! This module provides the API endpoints for the web UI to interact with
//! the FlowScope analysis engine.

use std::sync::Arc;

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
}

#[derive(Deserialize)]
struct ExportRequest {
    sql: String,
    #[serde(default)]
    files: Option<Vec<flowscope_core::FileSource>>,
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
    let options = if payload.hide_ctes.is_some() || payload.enable_column_lineage.is_some() {
        Some(flowscope_core::AnalysisOptions {
            hide_ctes: payload.hide_ctes,
            enable_column_lineage: payload.enable_column_lineage,
            ..Default::default()
        })
    } else {
        None
    };

    // Build template config if template mode is specified
    #[cfg(feature = "templating")]
    let template_config = payload.template_mode.as_ref().and_then(|mode| {
        match mode.as_str() {
            "jinja" => Some(flowscope_core::TemplateConfig {
                mode: flowscope_core::TemplateMode::Jinja,
                ..Default::default()
            }),
            "dbt" => Some(flowscope_core::TemplateConfig {
                mode: flowscope_core::TemplateMode::Dbt,
                ..Default::default()
            }),
            _ => None,
        }
    });

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
        template_config: None,
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
    })
}
