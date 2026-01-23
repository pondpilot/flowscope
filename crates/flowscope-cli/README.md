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
      --template <MODE>    Template preprocessing mode [possible values: jinja, dbt]
      --template-var <KEY=VALUE>
                           Template variable (can be repeated)
  -o, --output <FILE>      Output file (defaults to stdout)
      --project-name <PROJECT_NAME>
                           Project name used for default export filenames [default: lineage]
      --export-schema <SCHEMA>
                           Schema name to prefix DuckDB SQL export
  -v, --view <VIEW>        Graph detail level for mermaid output [default: table]
                           [possible values: script, table, column, hybrid]
  -q, --quiet              Suppress warnings on stderr
  -c, --compact            Compact JSON output (no pretty-printing)
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

**Security Note:** Passing credentials in command-line arguments may expose them via shell history and process listings (`ps`). For sensitive environments, consider:
- Using environment variables: `--metadata-url "$DATABASE_URL"`
- Using a `.pgpass` file for PostgreSQL
- Using socket-based authentication where available

This feature requires the `metadata-provider` feature flag (enabled by default). To build without it:

```bash
cargo build -p flowscope-cli --no-default-features
```

### dbt and Jinja Templating

FlowScope can preprocess SQL files that use Jinja or dbt-style templating before analysis. This enables lineage extraction from dbt models without running dbt itself.

```bash
# Analyze a dbt model with ref() and source() macros
flowscope --template dbt models/orders.sql

# Pass variables to templates
flowscope --template dbt --template-var target_schema=prod models/*.sql

# Plain Jinja templating (strict mode - undefined variables cause errors)
flowscope --template jinja --template-var env=production query.sql
```

**dbt mode** includes stub implementations of common macros:
- `ref('model')` / `ref('project', 'model')` - model references
- `source('schema', 'table')` - source table references
- `config(...)` - model configuration (returns empty string)
- `var('name')` / `var('name', 'default')` - variable access
- `is_incremental()` - always returns false for static analysis

Variables passed via `--template-var` are accessible in dbt mode through `var()` and in Jinja mode directly as template variables.

### Serve Mode (Embedded Web UI)

FlowScope can run as a local HTTP server serving the full web UI with a REST API backend. This provides a single-binary deployment where all analysis happens locally.

```bash
# Start server with watched SQL directories
flowscope --serve --watch ./sql --watch ./queries

# Use a custom port (default: 3000)
flowscope --serve --port 8080 --watch ./models

# Auto-open browser on startup
flowscope --serve --watch ./sql --open

# Combine with dialect and database schema
flowscope --serve --watch ./sql -d postgres --metadata-url postgres://user@localhost/db
```

**Server options:**

| Option | Description |
|--------|-------------|
| `--serve` | Start HTTP server with embedded web UI |
| `--port <PORT>` | Server port (default: 3000) |
| `--watch <DIR>` | Directory to watch for SQL files (repeatable) |
| `--open` | Open browser automatically on startup |

**REST API endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check with version |
| `/api/analyze` | POST | Run lineage analysis |
| `/api/completion` | POST | Get code completion items |
| `/api/split` | POST | Split SQL into statements |
| `/api/files` | GET | List watched files with content |
| `/api/schema` | GET | Get schema metadata |
| `/api/export/:format` | POST | Export to json/mermaid/html/csv/xlsx |

### Updating Embedded Assets

Serve mode bundles the React app at compile time. Whenever you change files under `app/`, run:

```bash
just sync-cli-serve-assets
```

This command rebuilds `app/dist` and copies the output into `crates/flowscope-cli/embedded-app`, which is what gets embedded (and published) with the CLI. Regular `cargo install flowscope-cli --features serve` uses these prebuilt assets, so remember to commit the refreshed `embedded-app/` contents when releasing new UI changes.
| `/api/config` | GET | Server configuration |

The file watcher monitors directories for `.sql` file changes with 100ms debouncing, automatically updating the available files in the UI.

**Building with serve mode:**

The serve feature requires the web app to be built first, as it embeds the frontend assets at compile time:

```bash
# Build with serve feature (justfile handles dependencies)
just build-cli-serve

# Or manually:
cd app && yarn build
cargo build -p flowscope-cli --features serve --release
```

### Troubleshooting

**Port already in use:**

If you see "Failed to bind to address" or "Address already in use":

```bash
# Check what's using the port
lsof -i :3000

# Use a different port
flowscope --serve --port 8080 --watch ./sql
```

**Watch directory permissions:**

If files aren't being detected:

- Ensure the watch directory exists and is readable
- Check for symlinks that may require `--follow` (enabled by default)
- Verify `.sql` file extension is lowercase

**Browser doesn't open:**

If `--open` doesn't work:

- Check your system's default browser configuration
- Manually navigate to the URL shown in the console
- On headless servers, `--open` will show a warning but continue running

**Schema not loading:**

If `--metadata-url` isn't providing schema:

- Verify database connectivity: `psql $DATABASE_URL -c '\dt'`
- Check schema permissions for the database user
- Use `--metadata-schema` to filter to a specific schema
