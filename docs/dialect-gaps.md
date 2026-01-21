# Dialect Support Gaps

FlowScope uses [sqlparser-rs](https://github.com/sqlparser-rs/sqlparser-rs) for SQL parsing.
Some dialects supported by SQLLineage (via SQLFluff) are not yet available.

## Supported Dialects

| Dialect | FlowScope | SQLLineage | Notes |
|---------|:---------:|:----------:|-------|
| ANSI | Yes | Yes | FlowScope also supports Generic dialect |
| BigQuery | Yes | Yes | |
| ClickHouse | Yes | Yes | |
| Databricks | Yes | Yes | |
| DuckDB | Yes | Yes | |
| Hive | Yes | Yes | |
| MS SQL (T-SQL) | Yes | Yes | |
| MySQL | Yes | Yes | |
| PostgreSQL | Yes | Yes | |
| Redshift | Yes | Yes | |
| Snowflake | Yes | Yes | |
| SQLite | Yes | Yes | |

## Missing Dialects

These dialects are supported by SQLLineage (via SQLFluff) but not available in FlowScope:

| Dialect | Blocking Issue | Workaround |
|---------|----------------|------------|
| Oracle | Not in sqlparser-rs | Use Generic dialect |
| Spark SQL | Not in sqlparser-rs | Use Hive or Databricks dialect |
| Athena | Not in sqlparser-rs | Use Generic dialect |
| Vertica | Not in sqlparser-rs | Use Generic dialect |
| DB2 | Not in sqlparser-rs | Use Generic dialect |
| Teradata | Not in sqlparser-rs | Use Generic dialect |
| Trino | Not in sqlparser-rs | Use Generic dialect |
| Impala | Not in sqlparser-rs | Use Hive dialect |
| MariaDB | Not in sqlparser-rs | Use MySQL dialect |
| Greenplum | Not in sqlparser-rs | Use PostgreSQL dialect |
| Exasol | Not in sqlparser-rs | Use Generic dialect |
| Materialize | Not in sqlparser-rs | Use PostgreSQL dialect |
| StarRocks | Not in sqlparser-rs | Use Generic dialect |
| Apache Doris | Not in sqlparser-rs | Use Generic dialect |
| Apache Flink SQL | Not in sqlparser-rs | Use Generic dialect |
| SOQL | Not in sqlparser-rs | Not applicable |

## Workaround Strategy

When using an unsupported dialect, select the closest compatible dialect:

1. **Oracle, DB2, Teradata, Vertica, Exasol** - Use `Generic` dialect
2. **Spark SQL** - Use `Hive` or `Databricks` dialect (Databricks is Spark-based)
3. **Athena** - Use `Generic` dialect (Athena is Presto/Trino-based)
4. **Trino** - Use `Generic` dialect
5. **MariaDB** - Use `MySQL` dialect (MariaDB is MySQL-compatible)
6. **Greenplum, Materialize** - Use `PostgreSQL` dialect (PostgreSQL-based)
7. **Impala** - Use `Hive` dialect (similar SQL semantics)

The `Generic` dialect is the most permissive and accepts syntax from multiple dialects.

## Contributing

To add dialect support:

1. Check if [sqlparser-rs](https://github.com/sqlparser-rs/sqlparser-rs) supports the dialect
2. Add the variant to the `Dialect` enum in `crates/flowscope-core/src/types/request.rs`
3. Add the mapping in `to_sqlparser_dialect()` method
4. Update `specs/dialect-semantics/dialects.json` with normalization rules
5. Add dialect-specific test fixtures in `crates/flowscope-core/tests/fixtures/`
6. Document any syntax limitations

## Tracking Upstream

To request a new dialect in sqlparser-rs:

1. Check [existing issues](https://github.com/sqlparser-rs/sqlparser-rs/issues) for the dialect
2. Open a new issue if none exists
3. Reference the issue in this document for tracking

### Open Upstream Issues

- Oracle: No active issue (complex PL/SQL support needed)
- Spark SQL: No dedicated dialect (use Hive or Databricks as workaround)
