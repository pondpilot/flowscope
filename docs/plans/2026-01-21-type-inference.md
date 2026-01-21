# Plan: Full Type Inference and Validation

Add comprehensive type inference to the SQL analyzer with schema-aware type lookup and type mismatch warnings. This enables users to catch type errors early and provides accurate column types in the lineage output.

## Validation Commands

- `just build-rust`
- `just test-core`
- `just lint-rust`

### Task 1: Unify Type Systems

Currently there are two type enums: `SqlType` in `types.rs` (7 types) and `CanonicalType` in generated code (10 types). This task consolidates them into a single type system based on `CanonicalType`, which has broader coverage including Time, Binary, Json, and Array types.

- [x] Remove `SqlType` enum from `src/analyzer/helpers/types.rs`
- [x] Update `infer_expr_type()` to return `Option<CanonicalType>` instead of `Option<SqlType>`
- [x] Add `impl Display for CanonicalType` in generated code for API output
- [x] Update all call sites that use `SqlType` to use `CanonicalType`
- [x] Update `sql_type_from_data_type()` to return `Option<CanonicalType>`
- [x] Update tests in `types.rs` to use `CanonicalType`
- [x] Verify all existing tests pass

### Task 2: Populate Column Types in Output

The `OutputColumn` struct has a `data_type: Option<String>` field that is often `None`. This task ensures SELECT columns have inferred types populated, making type information available in the API response for downstream tooling.

- [x] In `SelectAnalyzer`, call `infer_expr_type()` for each SELECT item expression
- [x] Store the inferred type as string in `OutputColumn.data_type`
- [x] When resolving wildcard (`*`), propagate types from source columns if known
- [x] When resolving CTE/subquery references, use the stored output column types
- [x] Add test: SELECT with literals has correct types (e.g., `SELECT 1, 'text', true`)
- [x] Add test: SELECT with functions has correct types (e.g., `SELECT COUNT(*), SUM(x)`)
- [x] Add test: CTE column types propagate to outer query

### Task 3: Schema-Aware Type Lookup

Expression type inference cannot determine column types without schema metadata. This task adds schema lookup so that `users.created_at` resolves to `TIMESTAMP` when schema is provided.

- [x] Add `lookup_column_type()` method that checks `TableSchema` for column types
- [x] Extend `infer_expr_type()` to accept schema context (or make it accessible)
- [x] When encountering `Expr::Identifier` or `Expr::CompoundIdentifier`, look up type from schema
- [x] Use `normalize_type_name()` to convert schema type strings to `CanonicalType`
- [x] Fall back to `None` when column not in schema (existing behavior)
- [x] Add test: column reference with schema returns correct type
- [x] Add test: column reference without schema returns `None`
- [x] Add test: qualified column reference (`table.column`) with schema

### Task 4: Type Mismatch Warnings

With types tracked throughout expressions, we can now detect and warn about type mismatches. This helps users catch bugs like comparing strings to integers or adding dates to booleans.

- [ ] Add new issue code `TYPE_MISMATCH` in `issue_codes`
- [ ] In binary comparison operators (=, <, >, etc.), check operand type compatibility
- [ ] In arithmetic operators (+, -, *, /), verify operands are numeric
- [ ] Use `can_implicitly_cast()` to allow compatible implicit conversions
- [ ] Emit `Issue::warning` with descriptive message including both types
- [ ] Include expression span in warning for editor integration
- [ ] Add test: `WHERE id = 'text'` emits warning (INTEGER vs TEXT)
- [ ] Add test: `WHERE name = name` no warning (same types)
- [ ] Add test: `WHERE int_col = float_col` no warning (implicit cast allowed)
- [ ] Add test: `SELECT date_col + bool_col` emits warning
