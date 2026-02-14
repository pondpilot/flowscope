//! FlowScope CLI - SQL lineage analyzer

use flowscope_cli::cli;
use flowscope_cli::fix::apply_lint_fixes_with_lint_config;
use flowscope_cli::input;
#[cfg(feature = "metadata-provider")]
use flowscope_cli::metadata;
use flowscope_cli::output;
use flowscope_cli::schema;
#[cfg(feature = "serve")]
use flowscope_cli::server;

use anyhow::{Context, Result};
use clap::Parser;
use flowscope_core::{analyze, AnalysisOptions, AnalyzeRequest, FileSource, LintConfig, Severity};
use flowscope_export::{
    export_csv_bundle, export_duckdb, export_html, export_json, export_mermaid, export_sql,
    export_xlsx, ExportFormat, ExportNaming, MermaidView,
};
use is_terminal::IsTerminal;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Instant;

use cli::{Args, OutputFormat, ViewMode};
use output::{format_lint_json, format_lint_results, format_table, FileLintResult, LintIssue};

/// Lint violations found or analysis errors.
const EXIT_FAILURE: u8 = 1;
/// Configuration error (e.g. unsupported format for the given mode).
const EXIT_CONFIG_ERROR: u8 = 66;

fn main() -> ExitCode {
    let args = Args::parse();

    #[cfg(feature = "serve")]
    if args.serve {
        return run_serve_mode(args);
    }

    if args.lint {
        return match run_lint(args) {
            Ok(has_violations) => {
                if has_violations {
                    ExitCode::from(EXIT_FAILURE)
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("flowscope: error: {e:#}");
                ExitCode::from(EXIT_CONFIG_ERROR)
            }
        };
    }

    match run(args) {
        Ok(has_errors) => {
            if has_errors {
                ExitCode::from(EXIT_FAILURE)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("flowscope: error: {e:#}");
            ExitCode::from(EXIT_CONFIG_ERROR)
        }
    }
}

/// Run the CLI in serve mode with embedded web UI.
#[cfg(feature = "serve")]
fn run_serve_mode(args: Args) -> ExitCode {
    use server::ServerConfig;

    #[cfg(feature = "templating")]
    let template_config = args.template.map(|mode| {
        let context = parse_template_vars(&args.template_vars);
        flowscope_core::TemplateConfig {
            mode: mode.into(),
            context,
        }
    });

    // Determine input source: watch directories or static files
    let (watch_dirs, static_files) = if !args.watch.is_empty() {
        // Watch mode takes precedence
        if !args.files.is_empty() {
            eprintln!("flowscope: warning: ignoring positional files when --watch is provided");
        }
        (args.watch.clone(), None)
    } else {
        // Try to read from positional files or stdin
        match input::read_input(&args.files) {
            Ok(files) if !files.is_empty() => (vec![], Some(files)),
            Ok(_) => {
                eprintln!("flowscope: error: no files to serve (use --watch or provide files)");
                return ExitCode::from(EXIT_FAILURE);
            }
            Err(e) => {
                eprintln!("flowscope: error: {e:#}");
                return ExitCode::from(EXIT_FAILURE);
            }
        }
    };

    let config = ServerConfig {
        dialect: args.dialect.into(),
        watch_dirs,
        static_files,
        #[cfg(feature = "metadata-provider")]
        metadata_url: args.metadata_url.clone(),
        #[cfg(not(feature = "metadata-provider"))]
        metadata_url: None,
        #[cfg(feature = "metadata-provider")]
        metadata_schema: args.metadata_schema.clone(),
        #[cfg(not(feature = "metadata-provider"))]
        metadata_schema: None,
        schema_path: args.schema.clone(),
        port: args.port,
        open_browser: args.open,
        #[cfg(feature = "templating")]
        template_config,
    };

    // Create tokio runtime and run server
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    match runtime.block_on(server::run_server(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("flowscope: server error: {e:#}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// Run the CLI in lint mode.
///
/// Analyzes each file individually with linting enabled, collects lint violations,
/// and formats them in a sqlfluff-style report.
fn run_lint(args: Args) -> Result<bool> {
    use output::lint::offset_to_line_col;

    let started_at = Instant::now();

    validate_lint_output_format(args.format)?;

    let mut lint_inputs = input::read_lint_input(&args.files)?;
    let dialect = args.dialect.into();
    let rule_configs = parse_rule_configs_json(args.rule_configs.as_deref())?;
    let lint_config = LintConfig {
        enabled: true,
        disabled_rules: args.exclude_rules.clone(),
        rule_configs,
    };

    #[cfg(feature = "templating")]
    let template_config = args.template.map(|mode| {
        let context = parse_template_vars(&args.template_vars);
        flowscope_core::TemplateConfig {
            mode: mode.into(),
            context,
        }
    });

    if args.fix {
        let mut total_applied = 0usize;
        let mut files_modified = 0usize;
        let mut skipped_due_to_comments = 0usize;
        let mut skipped_due_to_regression = 0usize;
        let mut skipped_due_to_parse_errors = 0usize;
        let mut stdin_modified = false;

        for lint_input in &mut lint_inputs {
            let outcome = match apply_lint_fixes_with_lint_config(
                &lint_input.source.content,
                dialect,
                &lint_config,
            ) {
                Ok(outcome) => outcome,
                Err(err) => {
                    skipped_due_to_parse_errors += 1;
                    if !args.quiet {
                        eprintln!(
                            "flowscope: warning: unable to auto-fix {}: {err}",
                            lint_input.source.name
                        );
                    }
                    continue;
                }
            };

            if outcome.skipped_due_to_comments {
                skipped_due_to_comments += 1;
                continue;
            }

            if outcome.skipped_due_to_regression {
                skipped_due_to_regression += 1;
                continue;
            }

            if !outcome.changed {
                continue;
            }

            total_applied += outcome.counts.total();
            files_modified += 1;
            lint_input.source.content = outcome.sql;

            if let Some(path) = &lint_input.path {
                fs::write(path, &lint_input.source.content)
                    .with_context(|| format!("Failed to write fixed SQL to {}", path.display()))?;
            } else {
                stdin_modified = true;
            }
        }

        if !args.quiet {
            eprintln!(
                "flowscope: applied {total_applied} auto-fix(es) across {files_modified} input(s)"
            );

            if skipped_due_to_comments > 0 {
                eprintln!(
                    "flowscope: skipped auto-fix for {skipped_due_to_comments} input(s) because comments are present"
                );
            }
            if skipped_due_to_regression > 0 {
                eprintln!(
                    "flowscope: skipped auto-fix for {skipped_due_to_regression} input(s) because fixes increased total violations"
                );
            }
            if skipped_due_to_parse_errors > 0 {
                eprintln!(
                    "flowscope: skipped auto-fix for {skipped_due_to_parse_errors} input(s) due to parse errors"
                );
            }
            if stdin_modified {
                eprintln!(
                    "flowscope: auto-fixes were applied to stdin input for linting output only (no file was written)"
                );
            }
        }
    }

    let mut file_results = Vec::with_capacity(lint_inputs.len());
    let mut progress = LintProgressBar::new(lint_inputs.len(), args.quiet);

    for lint_input in &lint_inputs {
        let source = &lint_input.source;
        #[cfg(not(feature = "templating"))]
        let result = analyze(&AnalyzeRequest {
            sql: source.content.clone(),
            files: None,
            dialect,
            source_name: Some(source.name.clone()),
            options: Some(AnalysisOptions {
                lint: Some(lint_config.clone()),
                ..Default::default()
            }),
            schema: None,
        });

        #[cfg(feature = "templating")]
        let result = {
            let request = AnalyzeRequest {
                sql: source.content.clone(),
                files: None,
                dialect,
                source_name: Some(source.name.clone()),
                options: Some(AnalysisOptions {
                    lint: Some(lint_config.clone()),
                    ..Default::default()
                }),
                schema: None,
                template_config: template_config.clone(),
            };

            let result = analyze(&request);
            if template_config.is_none()
                && contains_template_markers(&source.content)
                && has_parse_errors(&result)
            {
                // SQLFluff defaults to templating-aware linting; retry as Jinja when a raw
                // templated file would otherwise only produce parse errors.
                let jinja_retry = analyze(&AnalyzeRequest {
                    sql: source.content.clone(),
                    files: None,
                    dialect,
                    source_name: Some(source.name.clone()),
                    options: Some(AnalysisOptions {
                        lint: Some(lint_config.clone()),
                        ..Default::default()
                    }),
                    schema: None,
                    template_config: Some(flowscope_core::TemplateConfig {
                        mode: flowscope_core::TemplateMode::Jinja,
                        context: std::collections::HashMap::new(),
                    }),
                });

                if has_template_errors(&jinja_retry) {
                    // Fallback to dbt-compatible macro stubs for common Jinja macros
                    // (`ref`, `source`, etc.) when strict Jinja mode fails.
                    analyze(&AnalyzeRequest {
                        sql: source.content.clone(),
                        files: None,
                        dialect,
                        source_name: Some(source.name.clone()),
                        options: Some(AnalysisOptions {
                            lint: Some(lint_config.clone()),
                            ..Default::default()
                        }),
                        schema: None,
                        template_config: Some(flowscope_core::TemplateConfig {
                            mode: flowscope_core::TemplateMode::Dbt,
                            context: std::collections::HashMap::new(),
                        }),
                    })
                } else {
                    jinja_retry
                }
            } else {
                result
            }
        };

        let issues: Vec<LintIssue> = result
            .issues
            .iter()
            .filter(|i| i.code.starts_with("LINT_") || i.severity == Severity::Error)
            .map(|i| {
                let (line, col) = i
                    .span
                    .as_ref()
                    .map(|s| offset_to_line_col(&source.content, s.start))
                    .unwrap_or((1, 1));

                LintIssue {
                    line,
                    col,
                    code: i.code.clone(),
                    message: i.message.clone(),
                    severity: i.severity,
                }
            })
            .collect();

        file_results.push(FileLintResult {
            name: source.name.clone(),
            sql: source.content.clone(),
            issues,
        });
        progress.tick();
    }
    progress.finish();

    let has_violations = file_results.iter().any(|f| !f.issues.is_empty());
    let colored = args.output.is_none() && std::io::stdout().is_terminal();
    let elapsed = started_at.elapsed();

    let output_str = match args.format {
        OutputFormat::Json => format_lint_json(&file_results, args.compact),
        OutputFormat::Table => format_lint_results(&file_results, colored, elapsed),
        _ => unreachable!("lint output format validated before processing"),
    };

    write_output(&args.output, &output_str)?;

    Ok(has_violations)
}

fn parse_rule_configs_json(
    raw: Option<&str>,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    let Some(raw) = raw else {
        return Ok(std::collections::BTreeMap::new());
    };

    let value: serde_json::Value =
        serde_json::from_str(raw).context("Failed to parse --rule-configs JSON")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("--rule-configs must be a JSON object"))?;

    let mut rule_configs = std::collections::BTreeMap::new();
    let mut indentation_legacy = serde_json::Map::new();
    for (rule_ref, options) in object {
        if options.is_object() {
            rule_configs.insert(rule_ref.clone(), options.clone());
            continue;
        }

        // SQLFluff compatibility: allow flat indentation keys at the root of
        // --rule-configs (e.g., {"indent_unit":"tab","tab_space_size":2}).
        if matches!(
            rule_ref.to_ascii_lowercase().as_str(),
            "indent_unit" | "tab_space_size" | "indented_joins" | "indented_using_on"
        ) {
            indentation_legacy.insert(rule_ref.clone(), options.clone());
            continue;
        }

        anyhow::bail!("--rule-configs entry for '{rule_ref}' must be a JSON object");
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
                anyhow::bail!(
                    "--rule-configs entry for 'indentation' must be a JSON object, found {other}"
                );
            }
            None => indentation_legacy,
        };
        rule_configs.insert("indentation".to_string(), serde_json::Value::Object(merged));
    }

    Ok(rule_configs)
}

fn has_parse_errors(result: &flowscope_core::AnalyzeResult) -> bool {
    result
        .issues
        .iter()
        .any(|issue| issue.code == "PARSE_ERROR")
}

fn has_template_errors(result: &flowscope_core::AnalyzeResult) -> bool {
    result
        .issues
        .iter()
        .any(|issue| issue.code == "TEMPLATE_ERROR")
}

#[cfg(feature = "templating")]
fn contains_template_markers(sql: &str) -> bool {
    sql.contains("{{") || sql.contains("{%") || sql.contains("{#")
}

struct LintProgressBar {
    enabled: bool,
    total: usize,
    current: usize,
}

impl LintProgressBar {
    const WIDTH: usize = 30;

    fn new(total: usize, quiet: bool) -> Self {
        let enabled = !quiet && total > 0 && io::stderr().is_terminal();
        let progress = Self {
            enabled,
            total,
            current: 0,
        };

        if progress.enabled {
            progress.render();
        }

        progress
    }

    fn tick(&mut self) {
        if !self.enabled {
            return;
        }

        self.current = self.current.saturating_add(1).min(self.total);
        self.render();
    }

    fn finish(&self) {
        if self.enabled {
            eprintln!();
        }
    }

    fn render(&self) {
        let filled = if self.total == 0 {
            0
        } else {
            self.current * Self::WIDTH / self.total
        };
        let empty = Self::WIDTH - filled;

        eprint!(
            "\rLinting [{:=>filled$}{:empty$}] {}/{}",
            "", "", self.current, self.total
        );
        let _ = io::stderr().flush();
    }
}

fn validate_lint_output_format(format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json | OutputFormat::Table => Ok(()),
        other => {
            let name = match other {
                OutputFormat::Mermaid => "mermaid",
                OutputFormat::Html => "html",
                OutputFormat::Sql => "sql",
                OutputFormat::Csv => "csv",
                OutputFormat::Xlsx => "xlsx",
                OutputFormat::Duckdb => "duckdb",
                _ => "unknown",
            };
            anyhow::bail!("--lint only supports 'table' and 'json' output formats, got '{name}'");
        }
    }
}

fn run(args: Args) -> Result<bool> {
    // Read input files
    let sources = input::read_input(&args.files)?;

    // Load schema if provided
    let dialect = args.dialect.into();

    // Schema can come from DDL file or live database connection
    let schema_metadata = load_schema_metadata(&args, dialect)?;

    // Build template config if specified
    #[cfg(feature = "templating")]
    let template_config = args.template.map(|mode| {
        let context = parse_template_vars(&args.template_vars);
        flowscope_core::TemplateConfig {
            mode: mode.into(),
            context,
        }
    });

    // Build analysis request
    #[cfg(feature = "templating")]
    let request = build_request(sources, dialect, schema_metadata, template_config);
    #[cfg(not(feature = "templating"))]
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
        // Warn if credentials appear to be embedded in the URL
        if url.contains('@') && !url.starts_with("sqlite") {
            eprintln!(
                "flowscope: warning: Database credentials in --metadata-url may be logged in shell history. \
                 Consider using environment variables or a .pgpass file instead."
            );
        }

        let schema = metadata::fetch_metadata_from_database(url, args.metadata_schema.clone())?;
        return Ok(Some(schema));
    }

    // Fall back to DDL file
    args.schema
        .as_ref()
        .map(|path| schema::load_schema_from_ddl(path, dialect))
        .transpose()
        .context("Failed to load schema")
}

/// Parses template variables from KEY=VALUE format into a JSON context.
///
/// Whitespace is trimmed from keys and values for ergonomic CLI usage.
/// Values are parsed as JSON if valid, otherwise treated as strings.
#[cfg(feature = "templating")]
fn parse_template_vars(vars: &[String]) -> std::collections::HashMap<String, serde_json::Value> {
    let mut context = std::collections::HashMap::new();

    for var in vars {
        if let Some((key, value)) = var.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Skip empty keys
            if key.is_empty() {
                continue;
            }

            // Try to parse as JSON first, fall back to string
            let json_value = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
            context.insert(key.to_string(), json_value);
        }
    }

    context
}

#[cfg(feature = "templating")]
fn build_request(
    sources: Vec<FileSource>,
    dialect: flowscope_core::Dialect,
    schema: Option<flowscope_core::SchemaMetadata>,
    template_config: Option<flowscope_core::TemplateConfig>,
) -> AnalyzeRequest {
    if sources.len() == 1 {
        AnalyzeRequest {
            sql: sources[0].content.clone(),
            files: None,
            dialect,
            source_name: Some(sources[0].name.clone()),
            options: None,
            schema,
            template_config,
        }
    } else {
        AnalyzeRequest {
            sql: String::new(),
            files: Some(sources),
            dialect,
            source_name: None,
            options: None,
            schema,
            template_config,
        }
    }
}

#[cfg(not(feature = "templating"))]
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

#[cfg(test)]
mod tests {
    use super::parse_rule_configs_json;

    #[test]
    fn parse_rule_configs_json_accepts_object_map() {
        let parsed = parse_rule_configs_json(Some(
            r#"{"structure.subquery":{"forbid_subquery_in":"both"},"aliasing.unused":{"alias_case_check":"dialect"}}"#,
        ))
        .expect("parse rule configs");

        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed
                .get("structure.subquery")
                .and_then(|value| value.get("forbid_subquery_in"))
                .and_then(|value| value.as_str()),
            Some("both")
        );
    }

    #[test]
    fn parse_rule_configs_json_rejects_non_object_root() {
        let err = parse_rule_configs_json(Some("[]")).expect_err("expected parse error");
        assert!(err.to_string().contains("JSON object"));
    }

    #[test]
    fn parse_rule_configs_json_rejects_non_object_entry() {
        let err = parse_rule_configs_json(Some(r#"{"structure.subquery":"both"}"#))
            .expect_err("expected parse error");
        assert!(err
            .to_string()
            .contains("entry for 'structure.subquery' must be a JSON object"));
    }

    #[test]
    fn parse_rule_configs_json_accepts_flat_indentation_legacy_keys() {
        let parsed = parse_rule_configs_json(Some(r#"{"indent_unit":"tab","tab_space_size":2}"#))
            .expect("parse rule configs");

        let indentation = parsed
            .get("indentation")
            .and_then(serde_json::Value::as_object)
            .expect("indentation object");
        assert_eq!(
            indentation
                .get("indent_unit")
                .and_then(serde_json::Value::as_str),
            Some("tab")
        );
        assert_eq!(
            indentation
                .get("tab_space_size")
                .and_then(serde_json::Value::as_u64),
            Some(2)
        );
    }
}
