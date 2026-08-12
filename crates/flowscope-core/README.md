# flowscope-core

Core SQL lineage analysis engine for FlowScope.

## Overview

`flowscope-core` is a Rust library that performs static analysis on SQL queries to extract table and column-level lineage information. It serves as the foundation for the FlowScope ecosystem, powering the WebAssembly bindings and JavaScript packages.

## Features

- **Multi-Dialect Parsing:** Built on `sqlparser-rs`, supporting PostgreSQL, Snowflake, BigQuery, DuckDB, Redshift, MySQL, SQLite, Oracle, Databricks, ClickHouse, and Generic ANSI SQL.
- **Deep Lineage Extraction:**
  - Table-level dependencies (SELECT, INSERT, UPDATE, MERGE, COPY, UNLOAD, etc.)
  - Column-level data flow (including transformations)
  - Cross-statement lineage tracking (CREATE TABLE AS, INSERT INTO ... SELECT)
- **dbt/Jinja Templating:** Preprocess SQL with Jinja or dbt-style templates before analysis, with built-in stubs for `ref()`, `source()`, `config()`, `var()`, and `is_incremental()`.
- **Complex SQL Support:** Handles CTEs (Common Table Expressions), Subqueries, Joins, Unions, Window Functions, and lateral column aliases.
- **Schema Awareness:** Utilize provided schema metadata to validate column references and resolve wildcards (`SELECT *`).
- **Type Inference:** Infer expression types with dialect-aware type compatibility checking.
- **SQL Linting:** 72 lint rules across 9 families (AL, AM, CP, CV, JJ, LT, RF, ST, TQ) with AST-driven semantic checks and token-aware formatting checks. Rules include autofix metadata with safe/unsafe classification.
- **Diagnostics:** Returns structured issues (errors, warnings) with source spans for precise highlighting.

## Structure

```text
src/
├── analyzer.rs              # Main analysis orchestration
├── analyzer/
│   ├── context.rs           # Per-statement state and scope management
│   ├── schema_registry.rs   # Schema metadata and name resolution
│   ├── visitor.rs           # AST visitor for lineage extraction
│   ├── query.rs             # Query analysis (SELECT, subqueries)
│   ├── expression.rs        # Expression and column lineage
│   ├── select_analyzer.rs   # SELECT clause analysis
│   ├── statements.rs        # Statement-level analysis
│   ├── ddl.rs               # DDL statement handling (CREATE, ALTER)
│   ├── cross_statement.rs   # Cross-statement lineage tracking
│   ├── diagnostics.rs       # Issue reporting
│   ├── input.rs             # Input merging and deduplication
│   └── helpers/             # Utility functions
├── linter/                  # SQL lint engine
│   ├── mod.rs               # Linter orchestration
│   ├── config.rs            # Rule configuration
│   ├── document.rs          # Document model (shared tokens)
│   ├── rule.rs              # Rule trait and context
│   ├── visit.rs             # AST visitor for rules
│   └── rules/               # 72 rule implementations
├── parser/                  # SQL dialect handling
├── types/                   # Request/response types
└── lineage/                 # Lineage graph construction
```

## Usage

```rust
use flowscope_core::{analyze, AnalyzeRequest, Dialect};

fn main() {
    let request = AnalyzeRequest {
        sql: "SELECT u.name, o.id FROM users u JOIN orders o ON u.id = o.user_id".to_string(),
        files: None,
        dialect: Dialect::Postgres,
        source_name: Some("example.sql".to_string()),
        options: None,
        schema: None,
        #[cfg(feature = "templating")]
        template_config: None,
    };

    let result = analyze(&request);

    // The complete graph spans every statement.
    println!("Nodes: {:?}", result.nodes);
    println!("Edges: {:?}", result.edges);

    // Use the helpers to inspect one statement's portion of the flat graph.
    for statement in &result.statements {
        let nodes: Vec<_> = result
            .nodes_in_statement(statement.statement_index)
            .collect();
        let edges: Vec<_> = result
            .edges_in_statement(statement.statement_index)
            .collect();
        println!("Statement {} nodes: {nodes:?}", statement.statement_index);
        println!("Statement {} edges: {edges:?}", statement.statement_index);
    }
}
```

### Linting

```rust
use flowscope_core::{analyze, AnalysisOptions, AnalyzeRequest, Dialect, LintConfig};

let request = AnalyzeRequest {
    sql: "select * from users".to_string(),
    files: None,
    dialect: Dialect::Postgres,
    source_name: None,
    options: Some(AnalysisOptions {
        lint: Some(LintConfig::default()),
        ..Default::default()
    }),
    schema: None,
    #[cfg(feature = "templating")]
    template_config: None,
};

let result = analyze(&request);
let lint_issues: Vec<_> = result
    .issues
    .iter()
    .filter(|issue| issue.code.starts_with("LINT_"))
    .collect();
println!("Lint issues: {lint_issues:?}");
```

## Testing

```bash
cargo test
```

## License

Apache 2.0
