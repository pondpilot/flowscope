//! FlowScope CLI - SQL lineage analyzer

mod cli;
mod input;
mod output;
mod schema;

use anyhow::{Context, Result};
use clap::Parser;
use flowscope_core::{analyze, AnalyzeRequest, FileSource};
use flowscope_export::export_duckdb;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use cli::{Args, OutputFormat, ViewMode};
use output::{format_json, format_mermaid, format_table, MermaidViewMode};

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
    let schema_metadata = args
        .schema
        .as_ref()
        .map(|path| schema::load_schema_from_ddl(path, dialect))
        .transpose()
        .context("Failed to load schema")?;

    // Build analysis request
    let request = build_request(sources, dialect, schema_metadata);

    // Run analysis
    let result = analyze(&request);

    // Format output
    let output_str = match args.format {
        OutputFormat::Json => format_json(&result, args.compact),
        OutputFormat::Table => format_table(&result, args.quiet, !args.quiet),
        OutputFormat::Mermaid => {
            let view = match args.view {
                ViewMode::Script => MermaidViewMode::Script,
                ViewMode::Table => MermaidViewMode::Table,
                ViewMode::Column => MermaidViewMode::Column,
                ViewMode::Hybrid => MermaidViewMode::Hybrid,
            };
            format_mermaid(&result, view)
        }
        OutputFormat::Duckdb => {
            // DuckDB outputs bytes, not string
            let bytes = export_duckdb(&result).context("Failed to export to DuckDB")?;
            return write_binary_output(&args.output, &bytes, result.summary.has_errors);
        }
    };

    // Write output
    write_output(&args.output, &output_str)?;

    // Print issues to stderr if not quiet and not JSON
    if !args.quiet && args.format != OutputFormat::Json {
        print_issues_to_stderr(&result);
    }

    Ok(result.summary.has_errors)
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
    has_errors: bool,
) -> Result<bool> {
    if let Some(path) = path {
        fs::write(path, content)
            .with_context(|| format!("Failed to write to {}", path.display()))?;
    } else {
        // Binary to stdout - just write raw bytes
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
