# SQL Dialect Compliance Specification

This specification defines the behavior of the `flowscope` analyzer across different SQL dialects.

## 1. Identifier Normalization

Case sensitivity rules for identifiers (tables, columns, etc.) vary significantly by dialect.

| Dialect | Default Strategy | Quoted Strategy | Special Rules |
| :--- | :--- | :--- | :--- |
| **Postgres** | `LOWER` | Preserved | |
| **Redshift** | `LOWER` | Preserved | |
| **Snowflake** | `UPPER` | Preserved | |
| **BigQuery** | `CASE_INSENSITIVE` | Preserved | **Mixed Mode:** Tables/UDFs are Case-Sensitive; CTEs/Columns are Case-Insensitive. |
| **MySQL** | `CASE_SENSITIVE`* | Preserved | *Depends on OS/Config (lower_case_table_names). Default to Case-Sensitive for safety. |
| **MSSQL** | `CASE_INSENSITIVE` | Preserved | *Depends on Collation. Default to Case-Insensitive. |
| **DuckDB** | `CASE_INSENSITIVE` | Preserved | |
| **ClickHouse**| `CASE_SENSITIVE` | Preserved | |
| **Trino/Presto**| `LOWER` | Preserved | |
| **Hive** | `CASE_INSENSITIVE` | Preserved | |
| **Spark** | `CASE_INSENSITIVE` | Preserved | |
| **SQLite** | `CASE_INSENSITIVE` | Preserved | |
| **Oracle** | `UPPER` | Preserved | |
| **Databricks** | `LOWER` | Preserved | (Inherits from Spark) |

**Implementation Note:**
`flowscope` must implement `IdentifierType` context to handle BigQuery's mixed mode correctly.

## 2. Function Argument Handling

Certain functions use keywords or literals as arguments that should *not* be treated as column references.

### Date/Time Functions

| Function | Dialect | Rule | Example |
| :--- | :--- | :--- | :--- |
| `DATEDIFF` | Redshift, Snowflake, TSQL | Skip Arg 0 | `DATEDIFF(day, start, end)` |
| `DATEDIFF` | MySQL, Spark, Databricks | Skip None | `DATEDIFF(expr1, expr2)` (No unit arg, returns days) |
| `DATEDIFF` | Presto/Trino | Skip Arg 0 | `date_diff(unit, timestamp1, timestamp2)` |
| `DATEDIFF` | Postgres | N/A | Postgres has no DATEDIFF - use `AGE()`, `DATE_PART()`, or subtraction |
| `DATE_TRUNC` | Postgres, Snowflake | Skip Arg 0 | `DATE_TRUNC('month', date)` |
| `DATE_TRUNC` | BigQuery | Skip Arg 1 | `DATE_TRUNC(date, MONTH)` (Arg 1 is unit) |
| `EXTRACT` | All (Standard) | Special AST | `EXTRACT(part FROM date)` is usually parsed as `Expr::Extract` |
| `TIMESTAMP_ADD`| BigQuery | Skip Arg 1 | `TIMESTAMP_ADD(timestamp, INTERVAL n UNIT)` - Arg 0 is timestamp (track), Arg 1 is interval (skip) |
| `DATE_ADD` | Hive | Skip None | `DATE_ADD(date, days)` |
| `DATE_ADD` | Presto | Skip Arg 0 | `DATE_ADD(unit, value, timestamp)` |

### Structural Functions

| Function | Dialect | Rule | Notes |
| :--- | :--- | :--- | :--- |
| `JSON_EXTRACT` | BigQuery | Skip Arg 1 | Path string is a literal |
| `JSON_VALUE` | Various | Skip Arg 1 | Path string is a literal |

## 3. Scoping & Visibility

Rules for column visibility and alias resolution.

| Feature | Postgres | BigQuery | Snowflake | MySQL |
| :--- | :--- | :--- | :--- | :--- |
| **`HAVING` uses Select Alias** | No | Yes | **No** | Yes |
| **`GROUP BY` uses Select Alias**| No | Yes | **No** | Yes |
| **`UNNEST` behavior** | Expression | Table Source | Table Source | N/A |
| **`LATERAL` visibility** | Explicit | Implicit | Implicit | N/A |

## 4. Derived Table Aliasing

Rules for how subqueries in `FROM` must be aliased.

*   **Standard/Postgres/BigQuery/Snowflake:** `SELECT * FROM (SELECT 1) AS x` (Alias **required**)
    *   BigQuery: "Subquery must have an alias"
    *   Snowflake: "Every derived table must have its own alias"
*   **Oracle:** Alias optional (but recommended)

## 5. Unnest/Flatten Syntax

*   **Postgres:** `unnest(array_col)` (Function in FROM)
*   **BigQuery:** `UNNEST(array_col)` (Table source, requires `CROSS JOIN` or comma join)
*   **Snowflake:** `FLATTEN(input => array_col)` (Table function)
*   **Presto/Trino:** `UNNEST(array_col)` (Table source, requires `CROSS JOIN` or comma join)
*   **Spark/Databricks:** `explode(array_col)` (Function in SELECT or LATERAL VIEW)
