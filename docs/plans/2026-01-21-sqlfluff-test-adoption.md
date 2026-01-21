# Plan: Adopt SQLFluff SQL Parsing Tests

Adopt high-value SQL parsing tests from SQLFluff to improve flowscope's test coverage. Tests will be added to existing dialect fixture directories with both parsing snapshots and lineage assertions.

Source: `/home/sasha/Developer/tries/2026-01-20-sqlfluff/test/fixtures/dialects/`

## Validation Commands

- `just test-rust`
- `just test-lineage`

### Task 1: Tier 1 Edge Cases

Add 5 high-value edge case fixtures that catch parser bugs and improve robustness. These tests cover comment handling, deeply nested joins, expression recursion stress testing, empty input, and PostgreSQL array slicing syntax.

- [x] Add `fixtures/generic/bracket_in_comment.sql` - SQL with comments containing brackets/parens
- [x] Add `fixtures/generic/nested_joins.sql` - Joins nested up to 3 levels with parentheses
- [x] Add `fixtures/generic/expression_recursion.sql` - 40+ OR conditions stress test
- [x] Add `fixtures/generic/empty_file.sql` - Empty input edge case (tested via snapshot, not fixture - empty fixtures cause validation error)
- [x] Add `fixtures/postgres/array_slicing.sql` - Array slice syntax `a[:], b[:1], c[2:], d[2:3]`
- [x] Add parsing snapshot tests for all 5 fixtures
- [x] Add lineage assertions for SELECT-based fixtures (nested_joins, array_slicing)
- [x] Run `just test-rust` and verify all tests pass

### Task 2: Tier 2 PostgreSQL Dialect Depth

Add PostgreSQL-specific features that extend lineage coverage for LATERAL joins, aggregate FILTER clauses, and advanced GROUP BY variants (CUBE, ROLLUP, GROUPING SETS).

- [x] Add `fixtures/postgres/lateral_join.sql` - LATERAL subquery joins
- [x] Add `fixtures/postgres/filter_clause.sql` - `COUNT(*) FILTER (WHERE ...)` syntax
- [x] Add `fixtures/postgres/group_by_cube_rollup.sql` - CUBE, ROLLUP, GROUPING SETS
- [x] Add parsing snapshot tests for all 3 fixtures
- [x] Add lineage assertions tracking columns through LATERAL and aggregations
- [x] Run `just test-rust` and verify all tests pass

### Task 3: Tier 3 Snowflake Features

Add Snowflake-specific features including time travel queries, lateral FLATTEN with joins, and higher-order functions (FILTER, TRANSFORM, REDUCE with lambda expressions).

- [x] Add `fixtures/snowflake/time_travel.sql` - `AT(TIMESTAMP => ...)`, `BEFORE(STATEMENT => ...)`
- [x] Add `fixtures/snowflake/lateral_flatten.sql` - `lateral flatten(input => col)` after JOIN
- [x] Add `fixtures/snowflake/higher_order_functions.sql` - FILTER, TRANSFORM, REDUCE with lambdas
- [x] Add `fixtures/snowflake/group_by_cube_rollup.sql` - Snowflake CUBE/ROLLUP syntax
- [x] Add parsing snapshot tests for all 4 fixtures
- [x] Add lineage assertions for FLATTEN output columns and HOF results
- [x] Run `just test-rust` and verify all tests pass

### Task 4: Tier 4 BigQuery Features

Add BigQuery-specific features including hyphenated project/dataset identifiers, UNNEST patterns for array expansion, and SELECT * EXCEPT/REPLACE modifiers.

- [ ] Add `fixtures/bigquery/hyphenated_refs.sql` - `project-a.dataset-b.table-c` syntax
- [ ] Add `fixtures/bigquery/unnest_patterns.sql` - `UNNEST(array_col) AS item` variations
- [ ] Add `fixtures/bigquery/select_except_replace.sql` - `SELECT * EXCEPT(col) REPLACE(expr AS col)`
- [ ] Add parsing snapshot tests for all 3 fixtures
- [ ] Add lineage assertions for UNNEST expansion and EXCEPT/REPLACE column filtering
- [ ] Run `just test-rust` and verify all tests pass
