//! CLI argument parsing using clap.

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// FlowScope - SQL lineage analyzer
#[derive(Parser, Debug)]
#[command(name = "flowscope")]
#[command(about = "Analyze SQL files for data lineage", long_about = None)]
#[command(version)]
pub struct Args {
    /// SQL files to analyze (reads from stdin if none provided; --lint also accepts directories)
    #[arg(value_name = "FILES")]
    pub files: Vec<PathBuf>,

    /// SQL dialect
    #[arg(short, long, default_value = "generic", value_enum)]
    pub dialect: DialectArg,

    /// Output format
    #[arg(short, long, default_value = "table", value_enum)]
    pub format: OutputFormat,

    /// Schema DDL file for table/column resolution
    #[arg(short, long, value_name = "FILE")]
    pub schema: Option<PathBuf>,

    /// Database connection URL for live schema introspection
    /// (e.g., postgres://user:pass@host/db, mysql://..., sqlite://...)
    #[cfg(feature = "metadata-provider")]
    #[arg(long, value_name = "URL")]
    pub metadata_url: Option<String>,

    /// Schema name to filter when using --metadata-url
    /// (e.g., 'public' for PostgreSQL, database name for MySQL)
    #[cfg(feature = "metadata-provider")]
    #[arg(long, value_name = "SCHEMA")]
    pub metadata_schema: Option<String>,

    /// Output file (defaults to stdout)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Project name used for default export filenames
    #[arg(long, default_value = "lineage")]
    pub project_name: String,

    /// Schema name to prefix DuckDB SQL export
    #[arg(long, value_name = "SCHEMA")]
    pub export_schema: Option<String>,

    /// Graph detail level for mermaid output
    #[arg(short, long, default_value = "table", value_enum)]
    pub view: ViewMode,

    /// Run SQL linter and report violations
    #[arg(long)]
    pub lint: bool,

    /// Apply deterministic SQL lint auto-fixes in place (requires --lint)
    #[arg(long, requires = "lint")]
    pub fix: bool,

    /// Comma-separated list of lint rule codes to exclude (e.g., LINT_AM_008,LINT_ST_006)
    #[arg(long, value_delimiter = ',')]
    pub exclude_rules: Vec<String>,

    /// Suppress warnings on stderr
    #[arg(short, long)]
    pub quiet: bool,

    /// Compact JSON output (no pretty-printing)
    #[arg(short, long)]
    pub compact: bool,

    /// Template mode for preprocessing SQL (jinja or dbt)
    #[cfg(feature = "templating")]
    #[arg(long, value_enum)]
    pub template: Option<TemplateArg>,

    /// Template variable in KEY=VALUE format (can be repeated)
    #[cfg(feature = "templating")]
    #[arg(long = "template-var", value_name = "KEY=VALUE")]
    pub template_vars: Vec<String>,

    /// Start HTTP server with embedded web UI
    #[cfg(feature = "serve")]
    #[arg(long)]
    pub serve: bool,

    /// Port for HTTP server (default: 3000)
    #[cfg(feature = "serve")]
    #[arg(long, default_value = "3000")]
    pub port: u16,

    /// Directories to watch for SQL files (can be repeated)
    #[cfg(feature = "serve")]
    #[arg(long, value_name = "DIR")]
    pub watch: Vec<PathBuf>,

    /// Open browser automatically when server starts
    #[cfg(feature = "serve")]
    #[arg(long)]
    pub open: bool,
}

/// SQL dialect options
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DialectArg {
    Generic,
    Ansi,
    Bigquery,
    Clickhouse,
    Databricks,
    Duckdb,
    Hive,
    Mssql,
    Mysql,
    Postgres,
    Redshift,
    Snowflake,
    Sqlite,
}

impl From<DialectArg> for flowscope_core::Dialect {
    fn from(d: DialectArg) -> Self {
        match d {
            DialectArg::Generic => flowscope_core::Dialect::Generic,
            DialectArg::Ansi => flowscope_core::Dialect::Ansi,
            DialectArg::Bigquery => flowscope_core::Dialect::Bigquery,
            DialectArg::Clickhouse => flowscope_core::Dialect::Clickhouse,
            DialectArg::Databricks => flowscope_core::Dialect::Databricks,
            DialectArg::Duckdb => flowscope_core::Dialect::Duckdb,
            DialectArg::Hive => flowscope_core::Dialect::Hive,
            DialectArg::Mssql => flowscope_core::Dialect::Mssql,
            DialectArg::Mysql => flowscope_core::Dialect::Mysql,
            DialectArg::Postgres => flowscope_core::Dialect::Postgres,
            DialectArg::Redshift => flowscope_core::Dialect::Redshift,
            DialectArg::Snowflake => flowscope_core::Dialect::Snowflake,
            DialectArg::Sqlite => flowscope_core::Dialect::Sqlite,
        }
    }
}

/// Output format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table format
    Table,
    /// JSON output
    Json,
    /// Mermaid diagram
    Mermaid,
    /// HTML report
    Html,
    /// DuckDB SQL export
    Sql,
    /// CSV archive (zip)
    Csv,
    /// XLSX export
    Xlsx,
    /// DuckDB database file
    Duckdb,
}

/// Template mode for SQL preprocessing
#[cfg(feature = "templating")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TemplateArg {
    /// Plain Jinja2 templating
    Jinja,
    /// dbt-style templating with builtin macros
    Dbt,
}

#[cfg(feature = "templating")]
impl From<TemplateArg> for flowscope_core::TemplateMode {
    fn from(t: TemplateArg) -> Self {
        match t {
            TemplateArg::Jinja => flowscope_core::TemplateMode::Jinja,
            TemplateArg::Dbt => flowscope_core::TemplateMode::Dbt,
        }
    }
}

/// Graph detail level for visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewMode {
    /// Script/file level relationships
    Script,
    /// Table level lineage (default)
    Table,
    /// Column level lineage
    Column,
    /// Hybrid view (scripts + tables)
    Hybrid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialect_conversion() {
        let dialect: flowscope_core::Dialect = DialectArg::Postgres.into();
        assert_eq!(dialect, flowscope_core::Dialect::Postgres);
    }

    #[test]
    fn test_parse_minimal_args() {
        let args = Args::parse_from(["flowscope", "test.sql"]);
        assert_eq!(args.files.len(), 1);
        assert_eq!(args.dialect, DialectArg::Generic);
        assert_eq!(args.format, OutputFormat::Table);
        assert_eq!(args.project_name, "lineage");
        assert!(args.export_schema.is_none());
    }

    #[test]
    fn test_parse_full_args() {
        let args = Args::parse_from([
            "flowscope",
            "-d",
            "postgres",
            "-f",
            "json",
            "-s",
            "schema.sql",
            "-o",
            "output.json",
            "-v",
            "column",
            "--quiet",
            "--compact",
            "--project-name",
            "demo",
            "--export-schema",
            "lineage",
            "file1.sql",
            "file2.sql",
        ]);
        assert_eq!(args.dialect, DialectArg::Postgres);
        assert_eq!(args.format, OutputFormat::Json);
        assert_eq!(args.schema.unwrap().to_str().unwrap(), "schema.sql");
        assert_eq!(args.output.unwrap().to_str().unwrap(), "output.json");
        assert_eq!(args.view, ViewMode::Column);
        assert_eq!(args.project_name, "demo");
        assert_eq!(args.export_schema.as_deref(), Some("lineage"));
        assert!(args.quiet);
        assert!(args.compact);
        assert_eq!(args.files.len(), 2);
    }

    #[test]
    fn test_lint_flag() {
        let args = Args::parse_from(["flowscope", "--lint", "test.sql"]);
        assert!(args.lint);
        assert!(!args.fix);
        assert!(args.exclude_rules.is_empty());
    }

    #[test]
    fn test_lint_fix_flag() {
        let args = Args::parse_from(["flowscope", "--lint", "--fix", "test.sql"]);
        assert!(args.lint);
        assert!(args.fix);
    }

    #[test]
    fn test_fix_requires_lint() {
        let result = Args::try_parse_from(["flowscope", "--fix", "test.sql"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_lint_exclude_rules() {
        let args = Args::parse_from([
            "flowscope",
            "--lint",
            "--exclude-rules",
            "LINT_AM_008,LINT_ST_006",
            "test.sql",
        ]);
        assert!(args.lint);
        assert_eq!(args.exclude_rules, vec!["LINT_AM_008", "LINT_ST_006"]);
    }

    #[test]
    fn test_lint_exclude_rules_repeated() {
        let args = Args::parse_from([
            "flowscope",
            "--lint",
            "--exclude-rules",
            "LINT_AM_008",
            "--exclude-rules",
            "LINT_ST_006",
            "test.sql",
        ]);
        assert_eq!(args.exclude_rules, vec!["LINT_AM_008", "LINT_ST_006"]);
    }

    #[cfg(feature = "serve")]
    #[test]
    fn test_serve_args_defaults() {
        let args = Args::parse_from(["flowscope", "--serve"]);
        assert!(args.serve);
        assert_eq!(args.port, 3000);
        assert!(args.watch.is_empty());
        assert!(!args.open);
    }

    #[cfg(feature = "serve")]
    #[test]
    fn test_serve_args_custom_port() {
        let args = Args::parse_from(["flowscope", "--serve", "--port", "8080"]);
        assert!(args.serve);
        assert_eq!(args.port, 8080);
    }

    #[cfg(feature = "serve")]
    #[test]
    fn test_serve_args_watch_dirs() {
        let args = Args::parse_from([
            "flowscope",
            "--serve",
            "--watch",
            "./sql",
            "--watch",
            "./queries",
        ]);
        assert!(args.serve);
        assert_eq!(args.watch.len(), 2);
        assert_eq!(args.watch[0].to_str().unwrap(), "./sql");
        assert_eq!(args.watch[1].to_str().unwrap(), "./queries");
    }

    #[cfg(feature = "serve")]
    #[test]
    fn test_serve_args_open_browser() {
        let args = Args::parse_from(["flowscope", "--serve", "--open"]);
        assert!(args.serve);
        assert!(args.open);
    }

    #[cfg(feature = "serve")]
    #[test]
    fn test_serve_args_full() {
        let args = Args::parse_from([
            "flowscope",
            "--serve",
            "--port",
            "9000",
            "--watch",
            "./examples",
            "--open",
            "-d",
            "postgres",
        ]);
        assert!(args.serve);
        assert_eq!(args.port, 9000);
        assert_eq!(args.watch.len(), 1);
        assert!(args.open);
        assert_eq!(args.dialect, DialectArg::Postgres);
    }
}
