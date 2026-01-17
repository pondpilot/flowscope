# Dialect Coverage

FlowScope relies on `sqlparser-rs` for parsing and applies its own semantic rules for lineage. Coverage depends on the dialect parser and the analyzer’s supported statements.

## Supported Dialects (API)

These are the dialects exposed by the TypeScript API (`packages/core/src/types.ts`):

- `generic`
- `ansi`
- `bigquery`
- `clickhouse`
- `databricks`
- `duckdb`
- `hive`
- `mssql`
- `mysql`
- `postgres`
- `redshift`
- `snowflake`
- `sqlite`

## Statement Coverage

The analyzer provides lineage for these statement types when parsing succeeds:

- `SELECT` / `WITH` / set operations
- `INSERT INTO ... SELECT`
- `CREATE TABLE` / `CREATE TABLE AS SELECT`
- `CREATE VIEW`
- `UPDATE`
- `DELETE`
- `MERGE`
- `DROP` (schema tracking)

Unsupported constructs emit `UNSUPPORTED_SYNTAX` issues and return partial lineage when possible.

## Dialect Semantics

Dialect-specific normalization, scoping, and function rules are sourced from the semantic specs under:

- `crates/flowscope-core/specs/dialect-semantics/`

See `dialect_compliance_spec.md` and `comprehensive_dialect_rules.md` for details on how that data is used.
