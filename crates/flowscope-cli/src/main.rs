//! FlowScope CLI - SQL lineage analyzer

use flowscope_cli::cli;
use flowscope_cli::fix::{
    apply_lint_fixes_with_runtime_options, FixCandidateStats, LintFixExecution,
    LintFixRuntimeOptions,
};
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
    dali_compat, export_csv_bundle, export_duckdb, export_html, export_json, export_mermaid,
    export_sql, export_xlsx, ExportFormat, ExportNaming, MermaidView,
};
use is_terminal::IsTerminal;
use rayon::prelude::*;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cli::{Args, OutputFormat, ViewMode};
use output::{format_lint_json, format_lint_results, format_table, FileLintResult, LintIssue};

/// Lint violations found or analysis errors.
const EXIT_FAILURE: u8 = 1;
/// Configuration error (e.g. unsupported format for the given mode).
const EXIT_CONFIG_ERROR: u8 = 66;

enum LintFixComputation {
    Success(LintFixExecution),
    ParseError(String),
}

fn lint_fix_runtime_options_from_args(args: &Args) -> LintFixRuntimeOptions {
    LintFixRuntimeOptions {
        include_unsafe_fixes: args.unsafe_fixes,
        legacy_ast_fixes: args.legacy_ast_fixes,
    }
}

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
    let started_at = Instant::now();
    let fix_runtime_options = lint_fix_runtime_options_from_args(&args);

    if !args.fix_only {
        validate_lint_output_format(args.format)?;
    } else if !matches!(args.format, OutputFormat::Table) && !args.quiet {
        eprintln!("flowscope: warning: --format is ignored with --fix-only");
    }

    let respect_gitignore = !args.no_respect_gitignore;
    let lint_jobs = resolve_lint_jobs(args.jobs);
    let lint_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(lint_jobs)
        .build()
        .context("Failed to create lint worker pool")?;
    let mut lint_inputs =
        lint_pool.install(|| input::read_lint_input(&args.files, respect_gitignore))?;
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

    let mut fix_elapsed = None;
    if args.fix {
        if !args.quiet {
            eprintln!("flowscope: phase 1/2 applying fixes");
        }
        let fix_started_at = Instant::now();
        let mut total_applied = 0usize;
        let mut files_modified = 0usize;
        let mut skipped_due_to_comments = 0usize;
        let mut skipped_due_to_regression = 0usize;
        let mut skipped_due_to_parse_errors = 0usize;
        let mut skipped_or_blocked_candidates = FixCandidateStats::default();
        let mut stdin_modified = false;
        let progress = LintProgressBar::new("Phase 1/2 Fixing", lint_inputs.len(), args.quiet);

        let fix_computations = lint_pool.install(|| {
            let progress = progress.clone();
            lint_inputs
                .par_iter()
                .map(|lint_input| {
                    let result = match apply_lint_fixes_with_runtime_options(
                        &lint_input.source.content,
                        dialect,
                        &lint_config,
                        fix_runtime_options,
                    ) {
                        Ok(execution) => LintFixComputation::Success(execution),
                        Err(err) => LintFixComputation::ParseError(err.to_string()),
                    };
                    progress.tick();
                    result
                })
                .collect::<Vec<_>>()
        });
        progress.finish();

        for (lint_input, fix_computation) in lint_inputs.iter_mut().zip(fix_computations) {
            let lint_fix_execution = match fix_computation {
                LintFixComputation::Success(execution) => execution,
                LintFixComputation::ParseError(err) => {
                    skipped_due_to_parse_errors += 1;
                    skipped_or_blocked_candidates.skipped += 1;
                    if !args.quiet {
                        eprintln!(
                            "flowscope: warning: unable to auto-fix {}: {err}",
                            lint_input.source.name
                        );
                    }
                    continue;
                }
            };
            skipped_or_blocked_candidates.merge(lint_fix_execution.candidate_stats);
            let outcome = lint_fix_execution.outcome;

            if outcome.skipped_due_to_comments {
                skipped_due_to_comments += 1;
                skipped_or_blocked_candidates.blocked += 1;
                continue;
            }

            if outcome.skipped_due_to_regression {
                skipped_due_to_regression += 1;
                skipped_or_blocked_candidates.blocked += 1;
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
                if args.fix_only {
                    eprintln!(
                        "flowscope: auto-fixes were applied to stdin input (emitting fixed SQL output)"
                    );
                } else {
                    eprintln!(
                        "flowscope: auto-fixes were applied to stdin input for linting output only (no file was written)"
                    );
                }
            }

            let skipped_or_blocked_total = skipped_or_blocked_candidates.total_skipped_or_blocked();
            if skipped_or_blocked_total > 0 {
                if args.show_fixes {
                    eprintln!(
                        "flowscope: skipped/blocked fix candidates: {} (skipped: {}, blocked: {}, unsafe blocked: {}, display-only blocked: {}, protected-range blocked: {}, overlap-conflict blocked: {})",
                        skipped_or_blocked_total,
                        skipped_or_blocked_candidates.skipped,
                        skipped_or_blocked_candidates.blocked,
                        skipped_or_blocked_candidates.blocked_unsafe,
                        skipped_or_blocked_candidates.blocked_display_only,
                        skipped_or_blocked_candidates.blocked_protected_range,
                        skipped_or_blocked_candidates.blocked_overlap_conflict,
                    );
                } else {
                    eprintln!(
                        "flowscope: skipped/blocked fix candidates: {skipped_or_blocked_total} (use --show-fixes for details)"
                    );
                }
            } else if args.show_fixes {
                eprintln!(
                    "flowscope: skipped/blocked fix candidates: 0 (skipped: 0, blocked: 0, unsafe blocked: 0, display-only blocked: 0, protected-range blocked: 0, overlap-conflict blocked: 0)"
                );
            }
        }

        fix_elapsed = Some(fix_started_at.elapsed());
    }

    if args.fix_only {
        if !args.quiet {
            if let Some(fix_elapsed) = fix_elapsed {
                eprintln!(
                    "flowscope: phase timing: fix={}",
                    format_cli_elapsed(fix_elapsed)
                );
            }
        }

        let stdin_output = lint_inputs
            .iter()
            .find(|lint_input| lint_input.path.is_none())
            .map(|lint_input| lint_input.source.content.as_str());
        if let Some(sql) = stdin_output {
            write_output(&args.output, sql)?;
        }

        return Ok(false);
    }

    if !args.quiet {
        if args.fix {
            eprintln!("flowscope: phase 2/2 linting post-fix inputs");
        } else {
            eprintln!("flowscope: phase 1/1 linting inputs");
        }
    }
    let lint_started_at = Instant::now();

    #[cfg(not(feature = "templating"))]
    let file_results = lint_pool.install(|| {
        let label = if args.fix {
            "Phase 2/2 Linting"
        } else {
            "Linting"
        };
        let progress = LintProgressBar::new(label, lint_inputs.len(), args.quiet);
        let progress_for_workers = progress.clone();
        let results = lint_inputs
            .par_iter()
            .map(|lint_input| {
                let result = analyze_lint_source(&lint_input.source, dialect, &lint_config);
                progress_for_workers.tick();
                result
            })
            .collect::<Vec<_>>();
        progress.finish();
        results
    });

    #[cfg(feature = "templating")]
    let file_results = lint_pool.install(|| {
        let label = if args.fix {
            "Phase 2/2 Linting"
        } else {
            "Linting"
        };
        let progress = LintProgressBar::new(label, lint_inputs.len(), args.quiet);
        let progress_for_workers = progress.clone();
        let results = lint_inputs
            .par_iter()
            .map(|lint_input| {
                let result = analyze_lint_source(
                    &lint_input.source,
                    dialect,
                    &lint_config,
                    template_config.clone(),
                );
                progress_for_workers.tick();
                result
            })
            .collect::<Vec<_>>();
        progress.finish();
        results
    });
    let lint_elapsed = lint_started_at.elapsed();

    if !args.quiet {
        if let Some(fix_elapsed) = fix_elapsed {
            eprintln!(
                "flowscope: phase timing: fix={} lint={}",
                format_cli_elapsed(fix_elapsed),
                format_cli_elapsed(lint_elapsed)
            );
        } else {
            eprintln!(
                "flowscope: phase timing: lint={}",
                format_cli_elapsed(lint_elapsed)
            );
        }
    }

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

fn resolve_lint_jobs(jobs: Option<usize>) -> usize {
    jobs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    })
}

fn format_cli_elapsed(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs >= 1.0 {
        format!("{secs:.2}s")
    } else if elapsed.as_millis() >= 1 {
        format!("{}ms", elapsed.as_millis())
    } else {
        format!("{}us", elapsed.as_micros())
    }
}

fn to_file_lint_result(
    source: &FileSource,
    result: flowscope_core::AnalyzeResult,
) -> FileLintResult {
    use output::lint::offset_to_line_col;

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

    FileLintResult {
        name: source.name.clone(),
        sql: source.content.clone(),
        issues,
    }
}

#[cfg(not(feature = "templating"))]
fn analyze_lint_source(
    source: &FileSource,
    dialect: flowscope_core::Dialect,
    lint_config: &LintConfig,
) -> FileLintResult {
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

    to_file_lint_result(source, result)
}

#[cfg(feature = "templating")]
fn analyze_lint_source(
    source: &FileSource,
    dialect: flowscope_core::Dialect,
    lint_config: &LintConfig,
    template_config: Option<flowscope_core::TemplateConfig>,
) -> FileLintResult {
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
    let result = if template_config.is_none()
        && contains_template_markers(&source.content)
        && has_parse_errors(&result)
    {
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
    };

    to_file_lint_result(source, result)
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

#[derive(Clone)]
struct LintProgressBar {
    inner: Arc<LintProgressState>,
}

struct LintProgressState {
    enabled: bool,
    label: &'static str,
    total: usize,
    current: AtomicUsize,
    render_lock: Mutex<()>,
}

impl LintProgressBar {
    const WIDTH: usize = 30;

    fn new(label: &'static str, total: usize, quiet: bool) -> Self {
        let enabled = !quiet && total > 0 && io::stderr().is_terminal();
        let progress = Self {
            inner: Arc::new(LintProgressState {
                enabled,
                label,
                total,
                current: AtomicUsize::new(0),
                render_lock: Mutex::new(()),
            }),
        };

        if progress.inner.enabled {
            progress.render();
        }

        progress
    }

    fn tick(&self) {
        if !self.inner.enabled {
            return;
        }

        self.inner.current.fetch_add(1, Ordering::Relaxed);
        self.render();
    }

    fn finish(&self) {
        if !self.inner.enabled {
            return;
        }

        self.render();
        eprintln!();
    }

    fn render(&self) {
        if !self.inner.enabled {
            return;
        }

        let _render_guard = self.inner.render_lock.lock().expect("progress render lock");
        let current = self
            .inner
            .current
            .load(Ordering::Relaxed)
            .min(self.inner.total);
        let filled = if self.inner.total == 0 {
            0
        } else {
            current * Self::WIDTH / self.inner.total
        };
        let empty = Self::WIDTH - filled;

        eprint!(
            "\r{} [{:=>filled$}{:empty$}] {}/{}",
            self.inner.label, "", "", current, self.inner.total
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
                OutputFormat::Dali => "dali",
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
        OutputFormat::Dali => {
            let sql = if !request.sql.is_empty() {
                request.sql.clone()
            } else if let Some(ref files) = request.files {
                files
                    .iter()
                    .map(|f| f.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            };
            if args.compact {
                dali_compat::export_dali_compat_compact(&result, &sql)
            } else {
                dali_compat::export_dali_compat(&result, &sql)
            }
        }
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
