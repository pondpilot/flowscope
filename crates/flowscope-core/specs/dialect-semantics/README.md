# SQL Dialect Semantic Specifications

Semantic knowledge for SQL dialects used in flowscope's analyzer.

## Used by build.rs

These files are read by `build.rs` to generate Rust code in `src/generated/`:

| File | Generates | Description |
|------|-----------|-------------|
| `dialects.json` | `case_sensitivity.rs` | Normalization strategies, pseudocolumns, identifier quotes |
| `functions.json` | `functions.rs` | Function classification (aggregate/window/udtf sets) |
| `scoping_rules.toml` | `scoping_rules.rs` | Alias visibility rules (GROUP BY, HAVING, ORDER BY, LATERAL) |
| `dialect_behavior.toml` | `function_rules.rs` | NULL ordering, UNNEST behavior, date function arg skip rules |
| `normalization_overrides.toml` | `case_sensitivity.rs` | Dialects with custom normalize_identifier logic |

## Not Yet Used (Reference Data)

These files contain useful data for future features but aren't wired into code generation yet:

| File | Potential Use |
|------|---------------|
| `dialect_keywords.toml` | Reserved keyword validation |
| `dialect_specific_functions.toml` | Dialect-specific function availability checks |
| `function_return_types.toml` | Type inference for expressions |
| `type_system.toml` | Type coercion and casting rules |
| `table_generating_functions.toml` | UDTF detection with is_table_source flag |

## Key Data Points

### Function Categories
- 57 aggregate functions
- 13 window functions
- 5 UDTFs (table-generating)
- 371 scalar functions

### Normalization Strategies
- `case_insensitive`: BigQuery, DuckDB, Spark, Trino, Redshift, Hive, Presto, SQLite, Databricks, TSQL
- `case_sensitive`: ClickHouse, MySQL, Doris, StarRocks
- `lowercase`: Postgres, Drill, Teradata, Tableau
- `uppercase`: Snowflake, Oracle

### Alias Visibility in GROUP BY
- **Allows**: BigQuery, ClickHouse, Databricks, DuckDB, Hive, MySQL, Redshift, Spark, SQLite
- **Denies**: Postgres, Presto, Trino, Oracle, TSQL, Snowflake, Teradata

### NULL Ordering (ORDER BY default)
- `nulls_are_large` (NULLS LAST): Postgres, Oracle, Snowflake, Redshift
- `nulls_are_small` (NULLS FIRST): BigQuery, MySQL, Spark, SQLite, TSQL
- `nulls_are_last`: ClickHouse, DuckDB, Presto, Trino

### UNNEST Behavior
- **Implicit** (no CROSS JOIN needed): BigQuery, Redshift
- **Explicit** (requires CROSS JOIN): All others

### Custom Normalization Logic
Some dialects have complex normalization that can't be captured by a single strategy:
- **BigQuery**: Mixed case sensitivity
  - CTEs: case-insensitive (lowercased)
  - UDFs: case-sensitive (preserved)
  - Qualified tables: case-sensitive (preserved)
  - Unqualified identifiers: case-insensitive (lowercased)

### Table-Generating Functions (UDTFs)
Functions that produce rows rather than scalars (affect lineage graph structure):
- `UNNEST`, `EXPLODE`, `POSEXPLODE` - array expansion
- `LATERAL` - correlated subquery
- `GENERATE_SERIES`, `GENERATE_DATE_ARRAY` - row generators
- `VALUES` - inline table constructor

### Scope-Creating Expressions
Expressions that create new column visibility scopes:
- `Subquery` - isolated scope
- `CTE` - named scope
- `DerivedTable` - table scope
- `UDTF` - table function scope
- `Lateral` - allows outer scope references
- `Union/Intersect/Except` - combined scopes

## Regenerating

The TOML files were manually curated based on vendor documentation and testing.
