# Error Code Catalog

This document provides a comprehensive reference of all error and warning codes emitted by the FlowScope engine during SQL analysis.

## Error Codes by Category

### Parse Errors

#### `PARSE_ERROR`
- **Severity**: Error
- **Description**: SQL syntax cannot be parsed by the engine
- **When Emitted**: When `sqlparser-rs` fails to parse the input SQL
- **Example SQL**:
  ```sql
  SELECT * FORM users;  -- typo: FORM instead of FROM
  ```
- **User Action**: Fix the SQL syntax error
- **Engine Behavior**: Statement-level lineage skipped; continues with remaining statements

#### `DIALECT_FALLBACK`
- **Severity**: Warning
- **Description**: Requested dialect is not supported, falling back to generic dialect
- **When Emitted**: When an unknown or unsupported dialect is specified
- **Example**: `dialect: "oracle"` → fallback to `generic`
- **User Action**: Use a supported dialect (generic, postgres, snowflake, bigquery)
- **Engine Behavior**: Continues analysis using generic dialect parser

---

### Analysis Warnings

#### `UNSUPPORTED_SYNTAX`
- **Severity**: Warning
- **Description**: SQL syntax was parsed successfully but lineage analysis is not implemented for this construct
- **When Emitted**: When encountering valid SQL that the engine doesn't yet handle
- **Example SQL**:
  ```sql
  SELECT * FROM users PIVOT (SUM(amount) FOR category IN ('A', 'B'));
  ```
- **User Action**: Check dialect coverage matrix for current support status
- **Engine Behavior**: Provides partial lineage where possible; marks affected areas as approximate

#### `UNSUPPORTED_RECURSIVE_CTE`
- **Severity**: Warning
- **Description**: Recursive CTEs are not supported in Phase 1
- **When Emitted**: When a CTE references itself
- **Example SQL**:
  ```sql
  WITH RECURSIVE cte AS (
    SELECT 1 AS n
    UNION ALL
    SELECT n + 1 FROM cte WHERE n < 10
  )
  SELECT * FROM cte;
  ```
- **User Action**: Consider refactoring to non-recursive form or wait for Phase 2+ support
- **Engine Behavior**: Treats CTE as opaque; provides table-level lineage only

#### `APPROXIMATE_LINEAGE`
- **Severity**: Info
- **Description**: Column-level lineage is approximate due to missing schema metadata or SELECT *
- **When Emitted**:
  - When `SELECT *` is used without schema metadata
  - When column references cannot be fully resolved
- **Example SQL**:
  ```sql
  SELECT * FROM users;  -- without schema metadata
  ```
- **User Action**: Provide schema metadata in `SchemaMetadata` for precise lineage
- **Engine Behavior**: Provides table-level lineage; column lineage may be incomplete

#### `UNKNOWN_COLUMN`
- **Severity**: Warning
- **Description**: A column reference cannot be resolved against provided schema
- **When Emitted**: When schema is provided but referenced column doesn't exist in any source table
- **Example SQL**:
  ```sql
  -- schema only has: users[id, name]
  SELECT id, email FROM users;  -- 'email' doesn't exist
  ```
- **User Action**: Verify column name or update schema metadata
- **Engine Behavior**: Creates placeholder column node; continues analysis

#### `UNKNOWN_TABLE`
- **Severity**: Warning
- **Description**: A table reference cannot be resolved against provided schema or defaults
- **When Emitted**: When a referenced table is not in schema metadata and cannot be resolved via search path
- **Example SQL**:
  ```sql
  SELECT * FROM missing_table;  -- table not in schema
  ```
- **User Action**: Add table to schema metadata or verify table name
- **Engine Behavior**: Creates placeholder table node; continues analysis

---

### Cross-Statement Issues

#### `UNRESOLVED_REFERENCE`
- **Severity**: Warning
- **Description**: A table/CTE referenced in one statement was not produced by any prior statement
- **When Emitted**: In multi-statement analysis when a reference cannot be resolved
- **Example SQL**:
  ```sql
  -- Statement 1
  SELECT * FROM temp_data;  -- temp_data never created
  ```
- **User Action**: Verify the SQL includes all necessary DDL/DML statements
- **Engine Behavior**: Creates placeholder node in global graph; marks as unresolved

---

### System Issues

#### `CANCELLED`
- **Severity**: Info
- **Description**: Analysis was cancelled before completion (via AbortSignal)
- **When Emitted**: When host application cancels an in-progress analysis
- **User Action**: N/A - intentional cancellation
- **Engine Behavior**: Returns partial results accumulated before cancellation

#### `PAYLOAD_SIZE_WARNING`
- **Severity**: Warning
- **Description**: Request or response payload exceeds recommended size limit
- **When Emitted**: When SQL text or result JSON exceeds 10 MB
- **User Action**: Consider splitting large scripts or analyzing in chunks
- **Engine Behavior**: Continues processing but performance may degrade

#### `MEMORY_LIMIT_EXCEEDED`
- **Severity**: Error
- **Description**: WASM module exceeded memory limits during analysis
- **When Emitted**: When analysis requires more memory than available to WASM
- **User Action**: Reduce SQL complexity or increase WASM memory limits via initWasm options
- **Engine Behavior**: Analysis fails; returns error in result

---

## Error Code Format

All error codes follow the pattern:
- **SCREAMING_SNAKE_CASE**
- Descriptive and self-documenting
- Prefixed by category when appropriate

## Issue Structure

Every issue in the `AnalyzeResult.issues` array has:

```typescript
interface Issue {
  severity: 'error' | 'warning' | 'info';
  code: string;  // One of the codes above
  message: string;  // Human-readable description
  span?: Span;  // Optional source location
  statementIndex?: number;  // Which statement (0-indexed)
}
```

## Issue Handling Best Practices

### For Host Applications:

1. **Always check `summary.hasErrors`** before rendering lineage
2. **Display all warnings** to users so they understand lineage limitations
3. **Link issues to SQL spans** for easy navigation to problematic code
4. **Group issues by severity** for better UX

### For Engine Development:

1. **Be explicit**: Prefer warnings over silent approximations
2. **Include spans**: Always provide span information when available
3. **Actionable messages**: Tell users what to do, not just what went wrong
4. **Consistent codes**: Use existing codes; only add new ones with documentation

## Future Error Codes

These codes are planned but not yet implemented:

- `TYPE_MISMATCH` - Column type incompatibility (Phase 2+)
- `CIRCULAR_DEPENDENCY` - CTEs or tables have circular references
- `AMBIGUOUS_COLUMN` - Column name matches multiple sources
- `DEPRECATED_SYNTAX` - Using deprecated SQL syntax

---

Last Updated: 2025-11-20
