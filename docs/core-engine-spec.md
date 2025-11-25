# Core Engine Spec (Rust)

This document defines the **behavior and responsibilities** of the Rust lineage engine (`flowscope-core`) that runs inside WebAssembly (via `flowscope-wasm`).

It intentionally avoids low-level type signatures; it focuses on concepts, stages, and expectations.

## 1. Responsibilities

The core engine must:

1. Accept:
   - A **fully rendered SQL string** (no templating/macros),
   - A **dialect selection**,
   - Optional **schema metadata**, and
   - Some **analysis options**.
2. Parse the SQL into an AST using `sqlparser-rs`.
3. For each parsed statement, compute:
   - A **lineage graph**:
     - Tables and CTEs.
     - Columns (input and output).
     - Data flow and derivation edges.
   - Span information mapping nodes/edges back to source SQL when feasible.
4. Collect and report **issues** for:
   - Parse errors.
   - Unsupported constructs.
   - Approximate behaviors (like unresolved `*`).
5. Return a single structured result summarizing:
   - Per-statement lineage.
   - A deduplicated global lineage graph linking statements together.
   - Global issue list.
   - High-level summary metrics.

## 2. Dialect Support

### 2.1 Initial Dialects

- Generic SQL (fallback).
- Postgres-style.
- Snowflake.
- BigQuery.

Each dialect is mapped to an internal representation and ultimately to a specific `sqlparser-rs` dialect implementation.

### 2.2 Dialect Behavior

- If the dialect is recognized:
  - Use the matching `sqlparser-rs` dialect for parsing.
- If the dialect is unknown or not yet supported:
  - Fall back to Generic.
  - Emit a **warning issue** noting the mismatch.

For unsupported syntax in a given dialect:

- The engine should fail **gracefully**:
  - If parsing fails completely → record an error issue and skip lineage for that statement.
  - If parsing succeeds but a specific AST node kind is not handled → include partial lineage and emit a warning issue for that part.

## 3. Schema Metadata

### 3.1 Purpose

Schema metadata gives the engine knowledge of:

- Which columns exist on which tables.
- Potentially (later) data types or other attributes (not required for current version).

### 3.2 Canonical Structure

`SchemaMetadata` is a structured document:

- `default_catalog?` and `default_schema?` hint at the implicit namespace for unqualified identifiers.
- `search_path?` mirrors warehouse search path semantics (array of `{ catalog?, schema }` entries).
- `case_sensitivity?` lets callers override normalization with `dialect` (default), `lower`, `upper`, or `exact`.
- `tables: Vec<SchemaTable>` where each table has:
  - `catalog?: String`
  - `schema?: String`
  - `name: String`
  - `columns: Vec<ColumnSchema>` (`ColumnSchema.name`, optional `data_type` for future use).

The WASM boundary still accepts the older `Record<string, TableSchema>` form; the JS wrapper rewrites it into the canonical structure so the Rust core deals with exactly one format.

### 3.3 Resolution Rules

- Build a canonical identifier (Catalog, Schema, Name) for each table using provided values or defaults.
- Apply dialect-aware case normalization (`implementation-decisions.md` holds exact rules); override when `case_sensitivity` is specified.
- When resolving table references in SQL:
  - Attempt exact catalog/schema matches first.
  - If only schema/name exists, consult `search_path`.
  - When nothing matches, emit an `UNKNOWN_TABLE` warning and continue with best-effort lineage.

Column names inherit the same normalization rules. When conflicts remain ambiguous after normalization, prefer the first matching entry and emit a warning to alert the caller.

### 3.4 Usage

- With schema present:
  - Expand `*` and `table.*` into explicit columns.
  - Validate existence of referenced columns (emit issues on unknown columns).
  - Provide more accurate and complete column-level lineage.

- Without schema:
  - Perform best-effort lineage:
    - Explicitly referenced columns are tracked.
    - `*` is treated as a generic wildcard; the engine may:
      - Provide table-level lineage only.
      - Optionally create a single "star" column placeholder per source table.
  - Emit warnings that lineage is approximate where relevant.

## 4. Lineage Computation

### 4.1 Scope of Statements

The engine must handle at least:

- `SELECT`:
  - Simple selects.
  - Joins (inner, left/right/full, cross).
  - Aliases (`FROM table t`).
  - Subqueries in `FROM` and in select list.
  - Basic window functions are treated as expressions.

- `WITH` / CTEs:
  - Single and multiple CTEs.
  - Nested CTE references.

- `INSERT INTO ... SELECT`:
  - Optional column lists on target.
  - Mapping select list columns to target columns.

- `CREATE TABLE ... AS SELECT`:
  - treat as a target table similar to `INSERT INTO`.

- `UNION / UNION ALL` and basic set operations:
  - Map columns from both branches into the resulting output.

Explicit exclusions:

- `UPDATE`, `DELETE`, `MERGE`.
- DDL beyond `CREATE TABLE ... AS SELECT`.
- Stored procedures and vendor-specific DDL (can be extended later).

### 4.2 Conceptual Steps

For each parsed statement:

1. **AST normalization**:
   - Ensure consistent handling of:
     - Case sensitivity (e.g., unify internal representation).
     - Quoted identifiers.
     - Fully qualified vs unqualified table names.

2. **Table/CTE discovery**:
   - Traverse `FROM` clauses and CTE definitions.
   - Identify:
     - Base tables/views.
     - Named CTEs and their underlying queries.
     - Aliases for both.

3. **Column resolution**:
   - For each select expression:
     - Determine source columns:
       - Based on table aliases and column references.
       - For expressions (e.g., arithmetic, functions), retain expression text and list of input columns.
     - Determine output column name:
       - Either explicit alias or derived from underlying column/expression.
     - Associate each output column with:
       - Its parent table/CTE (for target statements).
       - Its expression and input column set.

4. **Target mapping (INSERT / CTAS)**:
   - Identify target table.
   - When a column list is present on the target:
     - Map each target column to the corresponding expression from the select list.
   - When no column list is present:
     - Map positions by index (1st select expression → 1st target column, etc.); if schema is known, validate counts.

5. **Set operations**:
   - For `UNION` and similar:
     - Compute lineage for each branch.
     - Map each result column to the corresponding columns in source branches.

6. **Graph assembly**:
   - Create table/CTE nodes (with metadata).
   - Create column nodes for:
     - Inputs (source columns).
     - Outputs (result of the statement, including target columns).
   - Create edges:
     - Ownership (table/CTE → columns).
     - Data flow/derivation:
       - Input column(s) → output column(s).
   - Annotate where relevant with:
     - Expression text.
     - Operation kind (JOIN, UNION, etc.).

7. **Issues & summary**:
   - Record any analysis problems encountered.
   - Update counters for summary metrics.

### 4.3 Treatment of `*`

- If schema is provided:
  - For each `*` or `table.*`:
    - Resolve full set of columns from the schema metadata.
    - Create explicit lineage edges for each column included.

- If schema is not provided:
  - For `SELECT *`:
    - Mark statement as **approximate** column lineage.
    - Provide:
      - Table-level lineage.
      - Optionally a single placeholder "star" column per source table.
  - Emit a warning issue indicating that the lineage might be incomplete or approximate.

### 4.4 Spans and Source Mapping

As far as `sqlparser-rs` allows, capture positional information (spans) for:

- Table references.
- Column references.
- Select expressions.

These spans are used later by the UI to highlight corresponding parts in the original SQL string.

If exact span information is unavailable for some nodes:

- The engine can omit spans for those nodes (null / absent).
- It should not fabricate misleading positions.

## 5. Output Structures (Conceptual)

### 5.1 Lineage Graph

The engine produces a **graph** for each statement, containing:

- Node list:
  - Each node has:
    - Stable identifier.
    - Type (table, CTE, column, etc.).
    - Human-readable label.
    - Optional metadata (e.g. fully qualified names, expression, etc.).
    - Optional source span (start/end offsets in SQL).

- Edge list:
  - Each edge has:
    - Stable identifier.
    - `from` node ID.
    - `to` node ID.
    - Edge type (data flow, derivation, ownership, etc.).
    - Optional metadata (e.g. expression summary, operation label).

### 5.2 Global Lineage Graph

- Deduplicate nodes across every statement into a `GlobalLineage` structure.
- Track `StatementRef`s on each node so UIs can hop back to the statement/span that defined it.
- Emit explicit cross-statement edges when a downstream statement reads something produced earlier.
- Create placeholder nodes for unresolved references and flag them with `UNRESOLVED_REFERENCE` issues.
- Produce this global graph even for single-statement inputs so consumers always have a consistent API.

### 5.3 Issues

For any problems:

- Include:
  - Severity (info, warning, error).
  - Machine-readable code (e.g. `PARSE_ERROR`, `UNSUPPORTED_SYNTAX`, `APPROXIMATE_LINEAGE`).
  - Human-readable message.
  - Optional span.

The engine should err on the side of being explicit when lineage is partial or approximate.

### 5.4 Summary

A small summary object should include:

- Number of statements analyzed.
- Number of tables discovered.
- Number of columns produced.
- Number of issues by severity.
- A boolean flag `has_errors`.

## 6. Cross-Statement Assembly

1. **Collect statement outputs**
   - For each statement, record produced tables/columns along with aliases and spans.
2. **Canonicalize identifiers**
   - Apply schema/dialect normalization to derive `(catalog, schema, name, column?)`.
3. **Deduplicate nodes**
   - Merge nodes that resolve to the same identifier, storing an array of originating statements.
4. **Link dependencies**
   - When a later statement references a node produced earlier, create `global_edges` pointing from producer node → consumer node plus metadata containing both statement indexes.
5. **Surface gaps**
   - Create placeholder nodes when no producer exists and attach warning issues so host apps can signal missing context.
6. **Expose navigation aids**
   - Provide quick lookup tables so the UI can jump from a global node to any statement-level node or span, and vice versa.

The assembly pass must run in linear time relative to total nodes/edges to keep multi-statement workloads interactive.

## 7. Performance & Limits

### 7.1 Target Scale

- Single statement size: a few thousand lines.
- Multi-statement input: up to a few dozen "normal-sized" statements (not enforced strictly at engine level; host apps can impose limits).

### 7.2 Time & Resource Behavior

- Engine should:
  - Avoid pathological recursion patterns.
  - Fail fast on obviously malformed input (without excessive backtracking).
- If a statement is too large or complex:
  - Host apps may decide to:
    - Analyze only table-level lineage.
    - Skip or warn; this is policy outside the engine but the engine should be robust.

## 8. Extensibility Hooks

The design should allow for:

- Adding more dialects without breaking compatible outputs.
- Adding new node or edge types as optional metadata (with backward-compatible defaults).
- Enhancing schema metadata (e.g., types, primary keys) in future versions.
