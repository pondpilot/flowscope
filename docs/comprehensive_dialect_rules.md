# Comprehensive SQL Dialect Specification

This document defines the granular behavior of 14 major SQL dialects supported by `flowscope`.

## 1. Identifier Normalization (Case Sensitivity)

How unquoted identifiers are handled. This is critical for resolving table and column references correctly.

| Dialect | Strategy | Rules & Exceptions |
| :--- | :--- | :--- |
| **Postgres** | `LOWERCASE` | Unquoted -> Lower. Quoted -> Preserved. |
| **Redshift** | `LOWERCASE` | Same as Postgres. |
| **Snowflake** | **`UPPERCASE`** | Unquoted -> Upper. Quoted -> Preserved. |
| **Oracle** | **`UPPERCASE`** | Unquoted -> Upper. Quoted -> Preserved. |
| **Exasol** | **`UPPERCASE`** | Unquoted -> Upper. |
| **BigQuery** | `CASE_INSENSITIVE`* | **Mixed Mode:** Tables/UDFs are `CASE_SENSITIVE`. Columns/CTEs are `CASE_INSENSITIVE`. Columns normalized to lower, tables preserved. |
| **MySQL** | `CASE_SENSITIVE`* | *Depends on OS (`lower_case_table_names`). `flowscope` defaults to `CASE_SENSITIVE` for safety. |
| **MSSQL (TSQL)**| `CASE_INSENSITIVE` | Unquoted identifiers are case-insensitive (collation dependent). |
| **ClickHouse** | **`CASE_SENSITIVE`** | Identifiers are case-sensitive by default. |
| **DuckDB** | `CASE_INSENSITIVE` | Unquoted -> Lower. |
| **Hive** | `CASE_INSENSITIVE` | Unquoted -> Lower. |
| **Spark** | `CASE_INSENSITIVE` | Unquoted -> Lower. |
| **Databricks** | `LOWERCASE` | Same as Spark. |
| **Presto/Trino**| `CASE_INSENSITIVE` | Unquoted -> Lower. |
| **SQLite** | `CASE_INSENSITIVE` | Unquoted -> Lower. |

## 2. Null Ordering (Sort Stability)

Where `NULL` values appear in `ORDER BY` clauses by default. This affects lineage for window functions and stable sorts.

| Dialect | Default Behavior |
| :--- | :--- |
| **Postgres** | `NULLS LAST` (Large) |
| **Oracle** | `NULLS LAST` (Large) |
| **Snowflake** | `NULLS LAST` (Large) |
| **ClickHouse** | `NULLS LAST` (Large) |
| **Presto/Trino**| `NULLS LAST` (Large) |
| **DuckDB** | `NULLS LAST` (Large) |
| **BigQuery** | `NULLS FIRST` (Small) |
| **Spark** | `NULLS FIRST` (Small) |
| **MySQL** | `NULLS FIRST` (Small) |
| **SQLite** | `NULLS FIRST` (Small) |

## 3. Pseudo-Columns (Implicit Columns)

Columns that exist on tables but are not part of the DDL. Lineage analysis must not fail when these are referenced.

| Dialect | Pseudo-Columns |
| :--- | :--- |
| **BigQuery** | `_PARTITIONTIME`, `_PARTITIONDATE`, `_TABLE_SUFFIX`, `_FILE_NAME` |
| **Oracle** | `ROWNUM`, `ROWID`, `OBJECT_ID`, `OBJECT_VALUE`, `LEVEL` |
| **Snowflake** | `LEVEL` (Connect By), `METADATA$FILENAME`, `METADATA$FILE_ROW_NUMBER` |
| **Postgres** | `ctid`, `xmin`, `xmax`, `cmin`, `cmax` |

## 4. `UNNEST` / Array Expansion Behavior

How arrays are expanded into rows. This fundamentally changes the scope and cardinality of the query.

| Dialect | Syntax | Behavior |
| :--- | :--- | :--- |
| **BigQuery** | `UNNEST(x) AS y` | **Table Source**. `UNNEST` creates a table. `AS y` aliases the *column*, not the table. Supports implicit unnest. |
| **Postgres** | `unnest(x)` | **Function**. Can appear in SELECT list or FROM. |
| **Snowflake** | `FLATTEN(input=>x)` | **Table Function**. Returns `VALUE`, `KEY`, `INDEX` columns. |
| **Presto/Trino**| `UNNEST(x)` | **Table Source**. Requires `CROSS JOIN` or `,`. |
| **Spark** | `explode(x)` | **Lateral View**. |
| **DuckDB** | `UNNEST(x)` | **Function**. |

## 5. Date/Time Function Semantic Variations

Functions with the same name but different argument structures.

### `DATEDIFF(unit, start, end)` vs `DATEDIFF(end, start)`

*   **3-Argument (Unit First):** Redshift, Snowflake, Presto/Trino, TSQL, DuckDB.
    *   `DATEDIFF(day, start_date, end_date)` / `date_diff('day', start, end)` returns an integer count.
*   **2-Argument (No Unit Literal):** MySQL, Hive, Spark, Databricks, BigQuery (`DATE_DIFF`).
    *   MySQL/Spark/Databricks: `DATEDIFF(end, start)` returns days as integer.
    *   BigQuery: `DATE_DIFF(end_date, start_date)` always returns day counts; other granularities use `DATETIME_DIFF`, `TIMESTAMP_DIFF`, etc.
*   **No DATEDIFF:** Postgres uses `AGE()`, `DATE_PART()`, or date subtraction instead.

### `DATE_TRUNC(unit, date)` vs `TRUNC(date, unit)`

*   **Standard (Postgres/Snowflake):** `DATE_TRUNC('month', date)`
*   **BigQuery:** `DATE_TRUNC(date, MONTH)` (Unit is second argument!)
*   **Oracle:** `TRUNC(date, 'MM')`

## 6. Unique Syntax & Keywords

*   **BigQuery:**
    *   `QUALIFY`: Supported (filtering window functions).
    *   `WINDOW`: Clause comes *after* `QUALIFY`.
    *   `OMITS NULLS`: Specific to array items.
*   **Snowflake:**
    *   `CONNECT BY`: Hierarchical queries (not supported by most others).
    *   `MATCH_RECOGNIZE`: Pattern matching in rows.
    *   `PIVOT/UNPIVOT`: Native syntax.
*   **Postgres:**
    *   `DISTINCT ON (...)`: Specific syntax.
    *   `::`: Cast operator (very common).
*   **MySQL:**
    *   `STRAIGHT_JOIN`: Force join order.
    *   `SQL_CALC_FOUND_ROWS`: Legacy pagination.

## 7. Scoping Rules (Implicit Visibility)

*   **`HAVING` Alias Visibility:**
    *   Allowed in: **BigQuery**, **MySQL**, **SQLite**, **DuckDB**, **Spark/Databricks**, **Redshift**.
    *   Forbidden in: **Postgres**, **Snowflake**, **MSSQL**, **Presto/Trino**.
*   **`GROUP BY` Alias Visibility:**
    *   Allowed in: **BigQuery**, **MySQL**, **SQLite**, **DuckDB**, **Spark/Databricks**, **Redshift**.
    *   Forbidden in: **Postgres**, **Snowflake**, **MSSQL**, **Presto/Trino**.
    *   *Note:* Snowflake evaluates SELECT aliases only after GROUP BY/HAVING; only ORDER BY can see them.
*   **`LATERAL` Visibility:**
    *   **Postgres:** Requires explicit `LATERAL` keyword for cross-references in FROM.
    *   **BigQuery/Snowflake:** Implicit lateral join for `UNNEST`/`FLATTEN`.

## 8. Hints & Optimizer Directives

*   **MySQL:** `TABLE_HINTS = True` (e.g., `USE INDEX`).
*   **MSSQL:** `TABLE_HINTS = True` (`WITH (NOLOCK)`).
*   **Snowflake/BigQuery/Postgres:** No table hints (use distinct syntax or comment hints like `/*+ ... */`).
