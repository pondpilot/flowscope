# Dialect Coverage Matrix

This document shows SQL feature support across different dialects in FlowScope.

## Supported Dialects

| Dialect | Parser | Status |
|---------|--------|--------|
| `generic` | sqlparser-rs Generic | ✅ Stable |
| `postgres` | sqlparser-rs PostgreSQL | ✅ Stable |
| `snowflake` | sqlparser-rs Snowflake | ✅ Stable |
| `bigquery` | sqlparser-rs BigQuery | ✅ Stable |

## Statement Support

| Statement Type | Generic | Postgres | Snowflake | BigQuery |
|----------------|---------|----------|-----------|----------|
| `SELECT` | ✅ | ✅ | ✅ | ✅ |
| `SELECT ... JOIN` | ✅ | ✅ | ✅ | ✅ |
| `SELECT ... UNION` | ✅ | ✅ | ✅ | ✅ |
| `WITH ... SELECT` (CTE) | ✅ | ✅ | ✅ | ✅ |
| `INSERT INTO ... SELECT` | ✅ | ✅ | ✅ | ✅ |
| `CREATE TABLE AS SELECT` | ✅ | ✅ | ✅ | ✅ |
| Subqueries in FROM | ✅ | ✅ | ✅ | ✅ |
| `UPDATE` | ⏳ | ⏳ | ⏳ | ⏳ |
| `DELETE` | ⏳ | ⏳ | ⏳ | ⏳ |
| `MERGE` | ⏳ | ⏳ | ⏳ | ⏳ |

**Legend:** ✅ Supported | ⚠️ Partial | ⏳ Planned | ❌ Not Supported

## JOIN Types

| Join Type | Generic | Postgres | Snowflake | BigQuery |
|-----------|---------|----------|-----------|----------|
| `INNER JOIN` | ✅ | ✅ | ✅ | ✅ |
| `LEFT JOIN` | ✅ | ✅ | ✅ | ✅ |
| `RIGHT JOIN` | ✅ | ✅ | ✅ | ✅ |
| `FULL OUTER JOIN` | ✅ | ✅ | ✅ | ✅ |
| `CROSS JOIN` | ✅ | ✅ | ✅ | ✅ |
| `NATURAL JOIN` | ✅ | ✅ | ✅ | ✅ |

## Set Operations

| Operation | Generic | Postgres | Snowflake | BigQuery |
|-----------|---------|----------|-----------|----------|
| `UNION` | ✅ | ✅ | ✅ | ✅ |
| `UNION ALL` | ✅ | ✅ | ✅ | ✅ |
| `INTERSECT` | ✅ | ✅ | ✅ | ✅ |
| `EXCEPT` | ✅ | ✅ | ✅ | ✅ |

## CTE Features

| Feature | Generic | Postgres | Snowflake | BigQuery |
|---------|---------|----------|-----------|----------|
| Basic CTE | ✅ | ✅ | ✅ | ✅ |
| Multiple CTEs | ✅ | ✅ | ✅ | ✅ |
| CTE referencing CTE | ✅ | ✅ | ✅ | ✅ |
| Recursive CTE | ⚠️ | ⚠️ | ⚠️ | ⚠️ |

**Note:** Recursive CTEs are parsed but generate `UNSUPPORTED_RECURSIVE_CTE` warning. Lineage for recursive references may be incomplete.

## Dialect-Specific Syntax

### Snowflake

| Feature | Status |
|---------|--------|
| `QUALIFY` clause | ✅ Parsed |
| `FLATTEN` / `LATERAL` | ✅ Parsed |
| `ILIKE` operator | ✅ Parsed |
| `INSERT OVERWRITE` | ✅ Parsed |
| `CLUSTER BY` | ✅ Parsed |

### BigQuery

| Feature | Status |
|---------|--------|
| Backtick identifiers | ✅ Parsed |
| `UNNEST` | ✅ Parsed |
| STRUCT access | ✅ Parsed |
| Partitioned tables | ✅ Parsed |

### PostgreSQL

| Feature | Status |
|---------|--------|
| `INTERVAL` expressions | ✅ Parsed |
| Array operators | ✅ Parsed |
| `DATE_TRUNC` | ✅ Parsed |
| Schema-qualified names | ✅ Parsed |

## Lineage Features

| Feature | Status |
|---------|--------|
| Table-level lineage | ✅ Stable |
| Column-level lineage | ⏳ Phase 2 |
| `SELECT *` expansion | ⏳ Phase 2 |
| Expression tracking | ⏳ Phase 2 |
| Cross-statement lineage | ✅ Stable |

## Test Coverage

Each dialect has test fixtures covering:
- Basic SELECT
- JOIN queries
- CTE queries
- INSERT INTO SELECT
- CREATE TABLE AS SELECT

See `crates/flowscope-core/tests/fixtures/` for examples.
