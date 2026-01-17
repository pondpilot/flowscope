# SQL Dialect Compliance Specification

This document describes how FlowScope applies dialect-specific semantics during analysis. The canonical data lives in `crates/flowscope-core/specs/dialect-semantics/` and is compiled into Rust via `build.rs`.

## Supported Dialects

FlowScope currently exposes these dialects through its public API:

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

## Semantic Rules Applied

The analyzer applies dialect-aware rules for:

- Identifier normalization and case sensitivity
- Alias visibility in `GROUP BY`, `HAVING`, and `ORDER BY`
- UNNEST/table-generating function behavior
- Function argument handling (date/time and structural functions)
- NULL ordering defaults

## Source of Truth

See the dialect semantic specs for precise behavior:

- `crates/flowscope-core/specs/dialect-semantics/dialects.json`
- `crates/flowscope-core/specs/dialect-semantics/scoping_rules.toml`
- `crates/flowscope-core/specs/dialect-semantics/dialect_behavior.toml`
- `crates/flowscope-core/specs/dialect-semantics/functions.json`
- `crates/flowscope-core/specs/dialect-semantics/normalization_overrides.toml`

If the public API does not expose a dialect listed in the specs, it is considered internal reference data.
