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
                           [possible values: table, json, mermaid]
  -s, --schema <FILE>      Schema DDL file for table/column resolution
  -o, --output <FILE>      Output file (defaults to stdout)
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
