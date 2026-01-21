# flowscope-cli

Command-line interface for the FlowScope SQL lineage analyzer.

## Features

- **Multi-File Analysis**: Analyze single or multiple SQL files, or pipe input from stdin.
- **Dialect Support**: Supports all FlowScope dialects (PostgreSQL, Snowflake, BigQuery, Generic, etc.).
- **Schema Awareness**: Load DDL files to enable precise column-level lineage and validation.
- **Multiple Output Formats**:
  - `table`: Human-readable text summary (default).
  - `json`: Structured JSON output for programmatic integration.
  - `mermaid`: Generate Mermaid diagrams for visualization.
  - `sql`: DuckDB SQL export (DDL + INSERT statements).
  - `csv`: ZIP archive with CSV exports for scripts, tables, mappings, and issues.
  - `xlsx`: Excel workbook with summary and lineage sheets.
  - `html`: Self-contained HTML report.
  - `duckdb`: DuckDB database file (native builds only).
- **View Modes**:
  - `table`: Table-level lineage (default).
  - `column`: Detailed column-level data flow.
  - `script`: File-level dependencies.
  - `hybrid`: Combined view of scripts and tables.

## Installation

```bash
cargo install --path crates/flowscope-cli
```

## Usage

```bash
# Analyze a single file
flowscope query.sql

# Analyze multiple files with a specific dialect
flowscope -d snowflake etl/*.sql

# Analyze from stdin
cat query.sql | flowscope

# Generate a Mermaid diagram
flowscope -f mermaid -v column query.sql > lineage.mmd
```

## Options

```text
Usage: flowscope [OPTIONS] [FILES]...

Arguments:
  [FILES]...  SQL files to analyze (reads from stdin if none provided)

Options:
  -d, --dialect <DIALECT>  SQL dialect [default: generic]
                           [possible values: generic, ansi, bigquery, clickhouse, databricks, duckdb, hive, mssql, mysql, postgres, redshift, snowflake, sqlite]
  -f, --format <FORMAT>    Output format [default: table]
                           [possible values: table, json, mermaid, html, sql, csv, xlsx, duckdb]
  -s, --schema <FILE>      Schema DDL file for table/column resolution
      --metadata-url <URL> Database connection URL for live schema introspection
                           (e.g., postgres://user:pass@host/db, mysql://..., sqlite://...)
      --metadata-schema <SCHEMA>
                           Schema name to filter when using --metadata-url
  -o, --output <FILE>      Output file (defaults to stdout)
      --project-name <PROJECT_NAME>
                           Project name used for default export filenames [default: lineage]
      --export-schema <SCHEMA>
                           Schema name to prefix DuckDB SQL export
  -v, --view <VIEW>        Graph detail level for mermaid output [default: table]
                           [possible values: script, table, column, hybrid]
  -q, --quiet              Suppress warnings on stderr
      --compact            Compact JSON output (no pretty-printing)
  -h, --help               Print help
  -V, --version            Print version
```

## Examples

### JSON Output

```bash
flowscope -f json -d postgres query.sql
```

### Mermaid Diagram with Schema

Load a schema DDL file to resolve wildcards and validate columns, then generate a column-level diagram:

```bash
flowscope -s schema.sql -f mermaid -v column query.sql
```

### CSV Archive Export

```bash
flowscope -f csv -o lineage.csv.zip query.sql
```

### Live Database Schema Introspection

Instead of providing a DDL schema file, you can connect directly to a database to fetch schema metadata at runtime. This enables accurate `SELECT *` resolution without manual schema maintenance.

Supported databases:
- PostgreSQL (`postgres://` or `postgresql://`)
- MySQL/MariaDB (`mysql://` or `mariadb://`)
- SQLite (`sqlite://`)

```bash
# PostgreSQL: fetch schema from public schema
flowscope --metadata-url postgres://user:pass@localhost/mydb query.sql

# PostgreSQL: filter to a specific schema
flowscope --metadata-url postgres://user:pass@localhost/mydb --metadata-schema analytics query.sql

# MySQL: fetch schema from the connected database
flowscope --metadata-url mysql://user:pass@localhost/mydb query.sql

# SQLite: fetch schema from a local database file
flowscope --metadata-url sqlite:///path/to/database.db query.sql
```

When both `--metadata-url` and `-s/--schema` are provided, the live database connection takes precedence.

This feature requires the `metadata-provider` feature flag (enabled by default). To build without it:

```bash
cargo build -p flowscope-cli --no-default-features
```
