//! CLI argument parsing using clap.

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// FlowScope - SQL lineage analyzer
#[derive(Parser, Debug)]
#[command(name = "flowscope")]
#[command(about = "Analyze SQL files for data lineage", long_about = None)]
#[command(version)]
pub struct Args {
    /// SQL files to analyze (reads from stdin if none provided)
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

    /// Output file (defaults to stdout)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Graph detail level for mermaid output
    #[arg(short, long, default_value = "table", value_enum)]
    pub view: ViewMode,

    /// Suppress warnings on stderr
    #[arg(short, long)]
    pub quiet: bool,

    /// Compact JSON output (no pretty-printing)
    #[arg(short, long)]
    pub compact: bool,
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
    /// DuckDB database file
    Duckdb,
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
            "file1.sql",
            "file2.sql",
        ]);
        assert_eq!(args.dialect, DialectArg::Postgres);
        assert_eq!(args.format, OutputFormat::Json);
        assert_eq!(args.schema.unwrap().to_str().unwrap(), "schema.sql");
        assert_eq!(args.output.unwrap().to_str().unwrap(), "output.json");
        assert_eq!(args.view, ViewMode::Column);
        assert!(args.quiet);
        assert!(args.compact);
        assert_eq!(args.files.len(), 2);
    }
}
