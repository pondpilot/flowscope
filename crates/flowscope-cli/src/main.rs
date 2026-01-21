//! FlowScope CLI - SQL lineage analyzer

mod cli;
mod input;
#[cfg(feature = "metadata-provider")]
mod metadata;
mod output;
mod schema;

use anyhow::{Context, Result};
use clap::Parser;
use flowscope_core::{analyze, AnalyzeRequest, FileSource};
use flowscope_export::{
    export_csv_bundle, export_duckdb, export_html, export_json, export_mermaid, export_sql,
    export_xlsx, ExportFormat, ExportNaming, MermaidView,
};
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use cli::{Args, OutputFormat, ViewMode};
use output::format_table;

fn main() -> ExitCode {
    match run() {
        Ok(has_errors) => {
            if has_errors {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("flowscope: error: {e:#}");
            ExitCode::from(66)
        }
    }
}

fn run() -> Result<bool> {
    let args = Args::parse();

    // Read input files
    let sources = input::read_input(&args.files)?;

    // Load schema if provided
    let dialect = args.dialect.into();

    // Schema can come from DDL file or live database connection
    let schema_metadata = load_schema_metadata(&args, dialect)?;

    // Build analysis request
    let request = build_request(sources, dialect, schema_metadata);

    // Run analysis
    let result = analyze(&request);

    let naming = ExportNaming::new(args.project_name.clone());

    let output_str = match args.format {
        OutputFormat::Json => {
            export_json(&result, args.compact).context("Failed to export JSON")?
        }
        OutputFormat::Table => format_table(&result, args.quiet, !args.quiet),
        OutputFormat::Mermaid => {
            let view = match args.view {
                ViewMode::Script => MermaidView::Script,
                ViewMode::Table => MermaidView::Table,
                ViewMode::Column => MermaidView::Column,
                ViewMode::Hybrid => MermaidView::Hybrid,
            };
            export_mermaid(&result, view).context("Failed to export Mermaid")?
        }
        OutputFormat::Html => export_html(&result, &args.project_name, naming.exported_at())
            .context("Failed to export HTML")?,
        OutputFormat::Sql => export_sql(&result, args.export_schema.as_deref())
            .context("Failed to export DuckDB SQL")?,
        OutputFormat::Csv => {
            let bytes = export_csv_bundle(&result).context("Failed to export CSV archive")?;
            return write_binary_output(
                &args.output,
                &bytes,
                &naming,
                ExportFormat::CsvBundle,
                result.summary.has_errors,
            );
        }
        OutputFormat::Xlsx => {
            let bytes = export_xlsx(&result).context("Failed to export XLSX")?;
            return write_binary_output(
                &args.output,
                &bytes,
                &naming,
                ExportFormat::Xlsx,
                result.summary.has_errors,
            );
        }
        OutputFormat::Duckdb => {
            let bytes = export_duckdb(&result).context("Failed to export DuckDB")?;
            return write_binary_output(
                &args.output,
                &bytes,
                &naming,
                ExportFormat::DuckDb,
                result.summary.has_errors,
            );
        }
    };

    write_output(&args.output, &output_str)?;

    if !args.quiet && args.format != OutputFormat::Json {
        print_issues_to_stderr(&result);
    }

    Ok(result.summary.has_errors)
}

/// Load schema metadata from DDL file or live database connection.
///
/// Priority:
/// 1. If `--metadata-url` is provided, connect to the database and fetch schema
/// 2. If `--schema` is provided, parse the DDL file
/// 3. Otherwise, return None
fn load_schema_metadata(
    args: &Args,
    dialect: flowscope_core::Dialect,
) -> Result<Option<flowscope_core::SchemaMetadata>> {
    // Live database connection takes precedence
    #[cfg(feature = "metadata-provider")]
    if let Some(ref url) = args.metadata_url {
        let schema = metadata::fetch_metadata_from_database(url, args.metadata_schema.clone())
            .map_err(|e| anyhow::anyhow!("Failed to fetch schema from database: {e}"))?;
        return Ok(Some(schema));
    }

    // Fall back to DDL file
    args.schema
        .as_ref()
        .map(|path| schema::load_schema_from_ddl(path, dialect))
        .transpose()
        .context("Failed to load schema")
}

fn build_request(
    sources: Vec<FileSource>,
    dialect: flowscope_core::Dialect,
    schema: Option<flowscope_core::SchemaMetadata>,
) -> AnalyzeRequest {
    if sources.len() == 1 {
        AnalyzeRequest {
            sql: sources[0].content.clone(),
            files: None,
            dialect,
            source_name: Some(sources[0].name.clone()),
            options: None,
            schema,
        }
    } else {
        AnalyzeRequest {
            sql: String::new(),
            files: Some(sources),
            dialect,
            source_name: None,
            options: None,
            schema,
        }
    }
}

fn write_output(path: &Option<std::path::PathBuf>, content: &str) -> Result<()> {
    if let Some(path) = path {
        fs::write(path, content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;
    } else {
        io::stdout()
            .write_all(content.as_bytes())
            .context("Failed to write to stdout")?;
        // Ensure newline at end for terminal output
        if !content.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

fn write_binary_output(
    path: &Option<std::path::PathBuf>,
    content: &[u8],
    naming: &ExportNaming,
    format: ExportFormat,
    has_errors: bool,
) -> Result<bool> {
    let resolved_path = path
        .clone()
        .or_else(|| Some(std::path::PathBuf::from(naming.filename(format))));

    if let Some(path) = resolved_path {
        fs::write(&path, content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;
    } else {
        io::stdout()
            .write_all(content)
            .context("Failed to write to stdout")?;
    }
    Ok(has_errors)
}

fn print_issues_to_stderr(result: &flowscope_core::AnalyzeResult) {
    use flowscope_core::Severity;

    for issue in &result.issues {
        let level = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };

        let location = issue
            .span
            .as_ref()
            .map(|s| format!(" (offset {})", s.start))
            .unwrap_or_default();

        eprintln!("flowscope: {level}:{location} {}", issue.message);
    }
}
