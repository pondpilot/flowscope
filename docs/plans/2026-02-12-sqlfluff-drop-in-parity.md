# SQLFluff Drop-In Parity Plan

Status: In Progress
Created: 2026-02-12
Last Updated: 2026-02-13
Owner: flowscope-core

## Goal

Make FlowScope a drop-in replacement for SQLFluff linting. Users migrating from
SQLFluff should be able to use the same rule codes, the same dotted rule names,
and the same `--noqa` annotations without behavior surprises.

This plan covers three axes:

1. **Code alignment** — FlowScope rule codes must map 1:1 to SQLFluff codes.
2. **Name and description alignment** — Rule metadata must use SQLFluff canonical names.
3. **Semantic depth** — Rules marked "partial" should close remaining behavioral gaps.

## Current State

- 72 SQLFluff rules are mapped to 72 FlowScope rules.
- 12 rules are "close" (materially equivalent semantics).
- 60 rules are "partial" (narrower scope, missing config, or regex-based).
- **27 rule codes are misnumbered** relative to SQLFluff (see Appendix A).
- Rule names use human-readable labels; SQLFluff uses dotted identifiers.
- Fix coverage: 62/72 (10 rules lack `--fix` support).

## Progress Snapshot (2026-02-13)

### Completed

- Phase 1 code alignment is implemented at runtime/output level:
  - Lint issues now emit SQLFluff-aligned canonical codes for AL/AM/CV/ST mappings.
  - Core + parity rule code constants and integration tests were updated to canonical numbering.
  - CLI fix filtering now uses canonicalized rule resolution.
  - Core rule module files were physically renamed to canonical code numbers and registry paths were updated.
- Phase 2 plumbing is implemented:
  - `Issue` includes `sqlfluff_name`.
  - Lint pipeline attaches `sqlfluff_name` to emitted issues.
  - `LintConfig.disabled_rules` supports canonical code forms and SQLFluff dotted names.
- Phase 5 (`--noqa`) support is implemented:
  - Parses `-- noqa` and `-- noqa: ...` comments from tokenizer comments.
  - Builds line-based suppression map and filters issues before final output.
  - Supports canonical/shorthand/dotted rule references via canonicalization.
- API schema snapshot updated to include lint output changes (`sqlfluffName`).
- Phase 4 fix-gap items from this plan are implemented:
  - `AL_005` fixer path is wired to canonical code gating.
  - `CV_008` fixer rewrites simple `RIGHT JOIN` patterns to `LEFT JOIN` with table reorder.
- Phase 1 docs cleanup is implemented:
  - `docs/sqlfluff-gap-matrix.md` mapping rows now use canonical SQLFluff-aligned FlowScope rule codes and updated module references.
- Phase 3 Tier 1 progress:
  - `ST_005` moved from parity regex handling to a dedicated core AST rule implementation.
  - `AL_004` moved from parity into a dedicated core AST rule (`al_004.rs`) and parity registration was removed.
  - `AL_001` moved from parity into a dedicated core rule module (`al_001.rs`) and parity registration was removed.
  - `AL_001` was further upgraded to AST-driven table-factor alias traversal with token-aware `AS` detection.
  - `AL_002` moved from parity into a dedicated core rule module (`al_002.rs`) and parity registration was removed.
  - `AL_002` was further upgraded to AST-driven SELECT projection alias traversal with token-aware `AS` detection.
  - `AL_001`/`AL_002` now support SQLFluff-style `aliasing` mode (`explicit`/`implicit`) via `lint.ruleConfigs`.
  - `AL_008` moved from parity into a dedicated core AST rule (`al_008.rs`) and parity registration was removed.
  - `AL_005` was expanded to cover derived-table aliases while adding SQLFluff-compatible `LATERAL` and `VALUES` exceptions to reduce false positives.
  - `CV_003` moved from parity into a dedicated core rule module (`cv_003.rs`) and parity registration was removed.
  - `CV_003` was further upgraded from regex scanning to token/depth-aware trailing-comma detection in SELECT clauses.
  - `CV_006` moved from parity into a dedicated core rule module (`cv_006.rs`) and parity registration was removed.
  - `ST_010` constant-expression detection scope was broadened beyond SELECT traversal to also check `UPDATE`/`DELETE` predicates and `MERGE ... ON`.
  - `ST_011` semantic scope was expanded from outer joins to all join types (excluding apply joins).
- Additional AST-driven migration progress beyond Tier 1:
  - `AL_006` moved from parity regex handling to a dedicated core AST rule (`al_006.rs`).
  - `AL_006` now supports configurable `min_alias_length` / `max_alias_length` through `lint.ruleConfigs`.
  - `AL_003` now supports configurable `allow_scalar` through `lint.ruleConfigs`.
  - `AL_007` moved from parity regex handling to a dedicated core AST rule (`al_007.rs`).
  - `AL_009` moved from parity regex handling to a dedicated core AST rule (`al_009.rs`).
  - `RF_004` moved from parity handling to a dedicated core rule module (`rf_004.rs`).
  - `RF_004` was further upgraded to AST-driven table/join alias analysis, eliminating SQL-string false positives from non-SQL string literals.
  - `RF_005` moved from parity handling to a dedicated core rule module (`rf_005.rs`).
  - `RF_005` was further upgraded to AST-driven quoted-identifier traversal, replacing raw quote-regex scanning.
  - `RF_006` moved from parity handling to a dedicated core rule module (`rf_006.rs`).
  - `RF_006` was further upgraded to AST-driven quoted-identifier traversal, replacing raw quote-regex scanning.
  - `ST_002` moved from parity regex handling to a dedicated core AST rule (`st_002.rs`).
  - `ST_006` moved from parity regex handling to a dedicated core AST rule (`st_006.rs`).
  - `ST_008` moved from parity regex handling to a dedicated core AST rule (`st_008.rs`).
  - `ST_012` moved from parity handling to a dedicated core rule module (`st_012.rs`).
  - `ST_012` was further upgraded from regex scanning to tokenizer-level semicolon sequencing, eliminating string-literal/comment false positives for consecutive-semicolon detection.
  - `TQ_001` moved from parity handling to a dedicated core rule module (`tq_001.rs`).
  - `TQ_001` was further upgraded to AST-driven procedure-name analysis via `Statement::CreateProcedure`, replacing SQL text regex scanning.
  - `TQ_002` moved from parity handling to a dedicated core rule module (`tq_002.rs`).
  - `TQ_002` was further upgraded to AST-driven procedure-body analysis via `CreateProcedure` `ConditionalStatements::BeginEnd` detection, replacing SQL text regex scanning.
  - `TQ_003` moved from parity handling to a dedicated core rule module (`tq_003.rs`).
  - `TQ_003` was further upgraded from regex scanning to token/line-aware repeated-`GO` separator detection, reducing string-literal false positives.
  - `CV_001` moved from parity handling to a dedicated core rule module (`cv_001.rs`).
  - `CV_001` was further upgraded from regex checks to lexer-style operator scanning that ignores comments/quoted strings for mixed `<>`/`!=` detection.
  - `CV_001` now supports `preferred_not_equal_style` (`consistent`/`c_style`/`ansi`) through `lint.ruleConfigs`.
  - `CV_004` now supports `prefer_count_1` / `prefer_count_0` through `lint.ruleConfigs`.
  - `CV_006` now supports `multiline_newline` / `require_final_semicolon` through `lint.ruleConfigs`.
  - `CV_007` moved from parity handling to a dedicated core rule module (`cv_007.rs`).
  - `CV_007` was further upgraded to AST-driven statement-shape detection (`Statement::Query` + wrapper `SetExpr::Query`), replacing SQL text `starts_with('(')/ends_with(')')` heuristics.
  - `CV_009` moved from parity handling to a dedicated core rule module (`cv_009.rs`).
  - `CV_009` was further upgraded to AST-driven traversal (table names, aliases, expression identifiers), replacing raw regex scanning and reducing string/comment false positives.
  - `CV_009` now supports configurable `blocked_words` / `blocked_regex` through `lint.ruleConfigs`.
  - `CV_010` moved from parity handling to a dedicated core rule module (`cv_010.rs`).
  - `CV_010` was further upgraded to AST-driven double-quoted identifier traversal, replacing raw quote-regex scanning.
  - `CV_010` now supports `preferred_quoted_literal_style` through `lint.ruleConfigs`.
  - `CV_011` moved from parity handling to a dedicated core rule module (`cv_011.rs`).
  - `CV_011` was further upgraded to AST-driven cast-kind traversal (`CastKind::{Cast, TryCast, SafeCast, DoubleColon}`), replacing raw SQL `::`/`CAST(` regex scanning and reducing string-literal false positives.
  - `CV_011` now supports `preferred_type_casting_style` through `lint.ruleConfigs`.
  - `JJ_001` moved from parity handling to a dedicated core rule module (`jj_001.rs`).
  - `JJ_001` was further upgraded from regex matching to deterministic delimiter scanning for Jinja padding checks.
  - `LT_010` moved from parity handling to a dedicated core rule module (`lt_010.rs`).
  - `LT_010` was further upgraded from regex scanning to tokenizer line-aware SELECT modifier checks.
  - `LT_011` moved from parity handling to a dedicated core rule module (`lt_011.rs`).
  - `LT_011` was further upgraded from regex scanning to tokenizer line-aware set-operator placement checks.
  - `LT_012` moved from parity handling to a dedicated core rule module (`lt_012.rs`).
  - `LT_013` moved from parity handling to a dedicated core rule module (`lt_013.rs`).
  - `LT_013` was further upgraded from regex matching to direct leading-blank-line scanning.
  - `LT_015` moved from parity handling to a dedicated core rule module (`lt_015.rs`).
  - `LT_015` was further upgraded from regex matching to direct blank-line run detection.
  - `LT_002` moved from parity handling to a dedicated core rule module (`lt_002.rs`).
  - `LT_003` moved from parity handling to a dedicated core rule module (`lt_003.rs`).
  - `LT_003` was further upgraded from regex scanning to tokenizer line-aware trailing-operator checks.
  - `LT_004` moved from parity handling to a dedicated core rule module (`lt_004.rs`).
  - `LT_004` was further upgraded from regex scanning to tokenizer-driven comma-spacing checks.
  - `LT_001` moved from parity handling to a dedicated core rule module (`lt_001.rs`).
  - `LT_001` was further upgraded from regex matching to deterministic layout-pattern scanners (JSON arrow/type/index/numeric-scale/EXISTS line form).
  - `LT_005` moved from parity handling to a dedicated core rule module (`lt_005.rs`).
  - `LT_006` moved from parity handling to a dedicated core rule module (`lt_006.rs`).
  - `LT_006` was further upgraded from regex masking to token-stream function-call spacing checks with context guards.
  - `LT_007` moved from parity handling to a dedicated core rule module (`lt_007.rs`).
  - `LT_007` was further upgraded from regex matching to deterministic CTE `WITH <ident> AS SELECT` sequence scanning.
  - `LT_008` moved from parity handling to a dedicated core rule module (`lt_008.rs`).
  - `LT_009` moved from parity handling to a dedicated core rule module (`lt_009.rs`).
  - `LT_009` was further upgraded from regex masking to tokenizer-located SELECT-line target counting.
  - `LT_014` moved from parity handling to a dedicated core rule module (`lt_014.rs`).
  - `LT_014` was further upgraded from regex masking to token/line-aware major-clause placement checks.
  - `CP_001` moved from parity handling to a dedicated core rule module (`cp_001.rs`).
  - `CP_002` moved from parity handling to a dedicated core rule module (`cp_002.rs`).
  - `CP_003` moved from parity handling to a dedicated core rule module (`cp_003.rs`).
  - `CP_004` moved from parity handling to a dedicated core rule module (`cp_004.rs`).
  - `CP_004` was further upgraded from regex masking to tokenizer-driven literal-token collection (`NULL`/`TRUE`/`FALSE`), reducing string/comment false positives.
  - `CP_005` moved from parity handling to a dedicated core rule module (`cp_005.rs`).
  - `CP_001` was further upgraded from regex masking to tokenizer-driven tracked-keyword collection.
  - `CP_002` was further upgraded from regex masking to token-stream identifier/function classification.
  - `CP_003` was further upgraded from regex scanning to token-stream function-call detection.
  - `CP_005` was further upgraded from regex masking to tokenizer-driven type-keyword collection.
  - Legacy `parity.rs` monolith was retired; rule registration now points only to dedicated `rules/<code>.rs` modules.

### Intentionally Removed

- Legacy SQLFluff `L0xx` support (Phase 2.4) was removed by decision:
  - No legacy-code resolution in `disabled_rules`.
  - No legacy-code resolution in `--noqa`.

### Remaining

- Phase 2 metadata parity:
  - SQLFluff canonical description text is not fully normalized across all rules.
- Phase 3 semantic-depth work remains open for Tier 2 and Tier 3.
  - Tier 1 is functionally complete across planned rules: `AL_001`, `AL_002`, `AL_004`, `AL_005` (with LATERAL/VALUES exceptions), `AL_008`, `CV_003`, `CV_006`, `ST_005`, `ST_010` (broadened), and `ST_011` (broadened).

---

## Phase 1: Rule Code Renumbering

**Why**: This is the single most breaking change and blocks all downstream work
(tests, docs, user configs, `--noqa` comments). Do it first and atomically.

### 1.1 Code Mapping Table

The following FlowScope codes must be renumbered to match SQLFluff:

#### AL (Aliasing) — 5 renames

| Current FS Code | SQLFluff Code | Rule (SQLFluff name) |
|---|---|---|
| `AL_001` | **`AL_003`** | `aliasing.expression` |
| `AL_002` | **`AL_005`** | `aliasing.unused` |
| `AL_003` | **`AL_001`** | `aliasing.table` |
| `AL_004` | **`AL_002`** | `aliasing.column` |
| `AL_005` | **`AL_004`** | `aliasing.unique.table` |

AL_006–AL_009 already match.

#### AM (Ambiguous) — 8 renames

| Current FS Code | SQLFluff Code | Rule (SQLFluff name) |
|---|---|---|
| `AM_001` | **`AM_002`** | `ambiguous.union` |
| `AM_002` | **`AM_009`** | `ambiguous.order_by_limit` |
| `AM_003` | **`AM_001`** | `ambiguous.distinct` |
| `AM_005` | **`AM_003`** | `ambiguous.order_by` |
| `AM_006` | **`AM_005`** | `ambiguous.join` |
| `AM_007` | **`AM_006`** | `ambiguous.column_references` |
| `AM_008` | **`AM_007`** | `ambiguous.set_columns` |
| `AM_009` | **`AM_008`** | `ambiguous.join_condition` |

AM_004 already matches.

#### CV (Convention) — 8 renames

| Current FS Code | SQLFluff Code | Rule (SQLFluff name) |
|---|---|---|
| `CV_001` | **`CV_002`** | `convention.coalesce` |
| `CV_002` | **`CV_004`** | `convention.count_rows` |
| `CV_003` | **`CV_005`** | `convention.is_null` |
| `CV_004` | **`CV_008`** | `convention.left_join` |
| `CV_005` | **`CV_001`** | `convention.not_equal` |
| `CV_006` | **`CV_003`** | `convention.select_trailing_comma` |
| `CV_007` | **`CV_006`** | `convention.terminator` |
| `CV_008` | **`CV_007`** | `convention.statement_brackets` |

CV_009–CV_012 already match.

#### ST (Structure) — 7 renames

| Current FS Code | SQLFluff Code | Rule (SQLFluff name) |
|---|---|---|
| `ST_001` | **`ST_003`** | `structure.unused_cte` |
| `ST_002` | **`ST_001`** | `structure.else_null` |
| `ST_003` | **`ST_004`** | `structure.nested_case` |
| `ST_004` | **`ST_007`** | `structure.using` |
| `ST_005` | **`ST_002`** | `structure.simple_case` |
| `ST_006` | **`ST_005`** | `structure.subquery` |
| `ST_007` | **`ST_006`** | `structure.column_order` |

ST_008–ST_012 already match.

#### Categories with no renames needed

CP (01–05), JJ (01), LT (01–15), RF (01–06), TQ (01–03) — all match.

### 1.2 Implementation Steps

Because many renames are cyclic (e.g., AL_001↔AL_003), a direct rename would
collide. Use a two-pass approach:

1. **Introduce a code alias table** in `issue_codes` that maps old→new codes.
   Temporarily emit both old and new codes during the transition.
2. **Rename constants and source files** in a single atomic commit:
   - `issue_codes::LINT_AL_001` → `LINT_AL_003`, etc.
   - Rename source files: `al_001.rs` → `al_003.rs`, etc.
   - Update `mod.rs` rule registry.
   - Update all test fixtures and snapshot expectations.
   - Update `parity.rs` code references.
   - Update `fix.rs` code references.
   - Update `sqlfluff-gap-matrix.md` and `linter-architecture.md`.
3. **Remove the alias table** once all references are updated.

### 1.3 Files Affected

Core rule files (rename + internal code string changes):
- `crates/flowscope-core/src/linter/rules/al_001.rs` → `al_003.rs`
- `crates/flowscope-core/src/linter/rules/al_002.rs` → `al_005.rs`
- `crates/flowscope-core/src/linter/rules/am_001.rs` → `am_002.rs`
- `crates/flowscope-core/src/linter/rules/am_002.rs` → `am_009.rs`
- `crates/flowscope-core/src/linter/rules/am_003.rs` → `am_001.rs`
- `crates/flowscope-core/src/linter/rules/am_005.rs` → `am_003.rs`
- `crates/flowscope-core/src/linter/rules/am_006.rs` → `am_005.rs`
- `crates/flowscope-core/src/linter/rules/am_007.rs` → `am_006.rs`
- `crates/flowscope-core/src/linter/rules/am_008.rs` → `am_007.rs`
- `crates/flowscope-core/src/linter/rules/am_009.rs` → `am_008.rs`
- `crates/flowscope-core/src/linter/rules/cv_001.rs` → `cv_002.rs`
- `crates/flowscope-core/src/linter/rules/cv_002.rs` → `cv_004.rs`
- `crates/flowscope-core/src/linter/rules/st_001.rs` → `st_003.rs`
- `crates/flowscope-core/src/linter/rules/st_002.rs` → `st_001.rs`
- `crates/flowscope-core/src/linter/rules/st_003.rs` → `st_004.rs`

Supporting files:
- `crates/flowscope-core/src/linter/rules/mod.rs` (registry)
- `crates/flowscope-core/src/linter/rules/parity.rs` (parity code strings)
- `crates/flowscope-core/src/linter/mod.rs` (issue_codes constants)
- `crates/flowscope-cli/src/fix.rs` (fixer code matching)
- `crates/flowscope-core/tests/linter.rs` (test expectations)
- `crates/flowscope-core/tests/snapshots/*.snap` (snapshot files)
- `docs/sqlfluff-gap-matrix.md`
- `docs/linter-architecture.md`
- `docs/error-codes.md`

### 1.4 Risk Mitigation

- This is a breaking change for anyone using rule codes in `disabled_rules` config.
- Document the migration in CHANGELOG with a clear old→new table.
- Consider supporting legacy code aliases in `LintConfig.disabled_rules` for one
  release cycle (resolve both `AM_001` and `AM_002` for the union rule during
  transition).

---

## Phase 2: Rule Name and Description Alignment

**Why**: SQLFluff users reference rules by dotted names (`aliasing.table`,
`ambiguous.union`). FlowScope uses human-readable names. For drop-in
compatibility, we need to support the SQLFluff canonical names.

### 2.1 Add `sqlfluff_name()` to `LintRule` Trait

Add a method to the `LintRule` trait:

```rust
fn sqlfluff_name(&self) -> &'static str;
```

This returns the dotted SQLFluff identifier (e.g., `"aliasing.table"`).

### 2.2 Full Name Mapping

After Phase 1 renumbering, each rule maps as follows:

| New FS Code | `name()` (current) | `sqlfluff_name()` (new) | SQLFluff `description` |
|---|---|---|---|
| AL_001 | "Table alias style" | "aliasing.table" | "Implicit/explicit aliasing of table." |
| AL_002 | "Column alias style" | "aliasing.column" | "Implicit/explicit aliasing of columns." |
| AL_003 | "Implicit alias" | "aliasing.expression" | "Column expression without alias. Use explicit `AS` clause." |
| AL_004 | "Unique table alias" | "aliasing.unique.table" | "Table aliases should be unique within each clause." |
| AL_005 | "Unused table alias" | "aliasing.unused" | "Tables should not be aliased if that alias is not used." |
| AL_006 | "Alias length" | "aliasing.length" | "Enforce table alias lengths." |
| AL_007 | "Forbid unnecessary alias" | "aliasing.forbid" | "Avoid table aliases in from clauses and join conditions." |
| AL_008 | "Unique column alias" | "aliasing.unique.column" | "Column aliases should be unique within each clause." |
| AL_009 | "Self alias column" | "aliasing.self_alias.column" | "Column aliases should not alias to itself." |
| AM_001 | "DISTINCT with GROUP BY" | "ambiguous.distinct" | "Ambiguous use of `DISTINCT` in a `SELECT` statement with `GROUP BY`." |
| AM_002 | "Ambiguous UNION quantifier" | "ambiguous.union" | "`UNION [DISTINCT\|ALL]` is preferred over just `UNION`." |
| AM_003 | "Ambiguous ORDER BY" | "ambiguous.order_by" | "Ambiguous ordering directions for columns in order by clause." |
| AM_004 | "Ambiguous column count" | "ambiguous.column_count" | "Query produces an unknown number of result columns." |
| AM_005 | "Ambiguous join style" | "ambiguous.join" | "Join clauses should be fully qualified." |
| AM_006 | "Ambiguous column references" | "ambiguous.column_references" | "Inconsistent column references in `GROUP BY/ORDER BY` clauses." |
| AM_007 | "Ambiguous set columns" | "ambiguous.set_columns" | "Queries within set query produce different numbers of columns." |
| AM_008 | "Ambiguous join condition" | "ambiguous.join_condition" | "Implicit cross join detected." |
| AM_009 | "LIMIT/OFFSET without ORDER BY" | "ambiguous.order_by_limit" | "Use of LIMIT and OFFSET without ORDER BY may lead to non-deterministic results." |
| CP_001 | "Keyword capitalisation" | "capitalisation.keywords" | "Inconsistent capitalisation of keywords." |
| CP_002 | "Identifier capitalisation" | "capitalisation.identifiers" | "Inconsistent capitalisation of unquoted identifiers." |
| CP_003 | "Function capitalisation" | "capitalisation.functions" | "Inconsistent capitalisation of function names." |
| CP_004 | "Literal capitalisation" | "capitalisation.literals" | "Inconsistent capitalisation of boolean/null literal." |
| CP_005 | "Type capitalisation" | "capitalisation.types" | "Inconsistent capitalisation of datatypes." |
| CV_001 | "Not-equal style" | "convention.not_equal" | "Consistent usage of `!=` or `<>` for not equal to operator." |
| CV_002 | "COALESCE convention" | "convention.coalesce" | "Use `COALESCE` instead of `IFNULL` or `NVL`." |
| CV_003 | "Select trailing comma" | "convention.select_trailing_comma" | "Trailing commas within select clause." |
| CV_004 | "COUNT style" | "convention.count_rows" | "Use consistent syntax to express count number of rows." |
| CV_005 | "Null comparison style" | "convention.is_null" | "Comparisons with NULL should use `IS` or `IS NOT`." |
| CV_006 | "Statement terminator" | "convention.terminator" | "Statements must end with a semi-colon." |
| CV_007 | "Statement brackets" | "convention.statement_brackets" | "Top-level statements should not be wrapped in brackets." |
| CV_008 | "LEFT JOIN convention" | "convention.left_join" | "Use `LEFT JOIN` instead of `RIGHT JOIN`." |
| CV_009 | "Blocked words" | "convention.blocked_words" | "Block a list of configurable words from being used." |
| CV_010 | "Quoted literals style" | "convention.quoted_literals" | "Consistent usage of preferred quotes for quoted literals." |
| CV_011 | "Casting style" | "convention.casting_style" | "Enforce consistent type casting style." |
| CV_012 | "Join condition convention" | "convention.join_condition" | "Use `JOIN ... ON ...` instead of `WHERE ...` for join conditions." |
| JJ_001 | "Jinja padding" | "jinja.padding" | "Jinja tags should have a single whitespace on either side." |
| LT_001 | "Layout spacing" | "layout.spacing" | "Inappropriate spacing." |
| LT_002 | "Layout indent" | "layout.indent" | "Incorrect indentation." |
| LT_003 | "Layout operators" | "layout.operators" | "Operators should follow a standard for being before/after newlines." |
| LT_004 | "Layout commas" | "layout.commas" | "Leading/Trailing comma enforcement." |
| LT_005 | "Layout long lines" | "layout.long_lines" | "Line is too long." |
| LT_006 | "Layout functions" | "layout.functions" | "Function name not immediately followed by parenthesis." |
| LT_007 | "Layout CTE bracket" | "layout.cte_bracket" | "`WITH` clause closing bracket should be on a new line." |
| LT_008 | "Layout CTE newline" | "layout.cte_newline" | "Blank line expected but not found after CTE closing bracket." |
| LT_009 | "Layout select targets" | "layout.select_targets" | "Select targets and nearby whitespace layout." |
| LT_010 | "Layout select modifiers" | "layout.select_modifiers" | "`SELECT` modifiers (e.g. `DISTINCT`) must be on the same line as `SELECT`." |
| LT_011 | "Layout set operators" | "layout.set_operators" | "Set operators should be surrounded by newlines." |
| LT_012 | "Layout end of file" | "layout.end_of_file" | "Files must end with a single trailing newline." |
| LT_013 | "Layout start of file" | "layout.start_of_file" | "Files must not begin with newlines or whitespace." |
| LT_014 | "Layout keyword newline" | "layout.keyword_newline" | "Keyword clauses should follow a standard for being before/after newlines." |
| LT_015 | "Layout newlines" | "layout.newlines" | "Too many consecutive blank lines." |
| RF_001 | "References from" | "references.from" | "References to tables/views must exist in `FROM` clause." |
| RF_002 | "References qualification" | "references.qualification" | "References should be qualified if select has more than one table." |
| RF_003 | "References consistent" | "references.consistent" | "Column references should be qualified consistently." |
| RF_004 | "References keywords" | "references.keywords" | "Keywords should not be used as identifiers." |
| RF_005 | "References special chars" | "references.special_chars" | "Do not use special characters in identifiers." |
| RF_006 | "References quoting" | "references.quoting" | "Unnecessary quoted identifier." |
| ST_001 | "Unnecessary ELSE NULL" | "structure.else_null" | "Do not specify `else null` in a case when statement." |
| ST_002 | "Structure simple case" | "structure.simple_case" | "Prefer simple `CASE` when possible." |
| ST_003 | "Unused CTE" | "structure.unused_cte" | "Query defines a CTE but does not use it." |
| ST_004 | "Flattenable nested CASE" | "structure.nested_case" | "Nested `CASE` statement in `ELSE` clause could be flattened." |
| ST_005 | "Structure subquery" | "structure.subquery" | "Join/From clauses should not contain subqueries. Use CTEs instead." |
| ST_006 | "Structure column order" | "structure.column_order" | "Select wildcards then simple targets before calculations." |
| ST_007 | "Avoid USING in JOIN" | "structure.using" | "Prefer specifying join keys instead of using `USING`." |
| ST_008 | "Structure distinct" | "structure.distinct" | "`DISTINCT` used with parentheses." |
| ST_009 | "Structure join condition order" | "structure.join_condition_order" | "Joins should list the table referenced earlier/later first." |
| ST_010 | "Structure constant expression" | "structure.constant_expression" | "Redundant constant expression." |
| ST_011 | "Structure unused join" | "structure.unused_join" | "Unused join detected." |
| ST_012 | "Structure consecutive semicolons" | "structure.consecutive_semicolons" | "Consecutive semicolons detected." |
| TQ_001 | "TSQL sp_ prefix" | "tsql.sp_prefix" | "`SP_` prefix should not be used for stored procedures." |
| TQ_002 | "TSQL procedure BEGIN/END" | "tsql.procedure_begin_end" | "Procedure bodies with multiple statements should be wrapped in BEGIN/END." |
| TQ_003 | "TSQL empty batch" | "tsql.empty_batch" | "Remove empty batches." |

### 2.3 Implementation Steps

1. Add `sqlfluff_name() -> &'static str` to `LintRule` trait with a default
   implementation that derives from code (e.g., `AL_001` → `"aliasing.table"`
   via a lookup table).
2. Update each core rule struct to return the correct dotted name.
3. Update each parity rule entry to include the dotted name.
4. Expose `sqlfluff_name` in the `Issue` output struct so CLI and API consumers
   can reference it.
5. Support dotted names in `LintConfig.disabled_rules` (resolve `"aliasing.table"`
   to `AL_001`).
6. Update descriptions to match SQLFluff canonical descriptions (see table above).

### 2.4 Legacy Code Aliases

SQLFluff also supports legacy L-codes (e.g., `L011` = `AL01`). Supporting these
is optional but useful for migration. Add a `legacy_codes()` method:

```rust
fn legacy_codes(&self) -> &'static [&'static str] {
    // e.g., AL_001 → &["L011"]
}
```

Support these in `disabled_rules` config resolution.

Full legacy code mapping:

| FS Code | SQLFluff Code | Legacy Code |
|---|---|---|
| AL_001 | AL01 | L011 |
| AL_002 | AL02 | L012 |
| AL_003 | AL03 | L013 |
| AL_004 | AL04 | L020 |
| AL_005 | AL05 | L025 |
| AL_006 | AL06 | L066 |
| AL_007 | AL07 | L031 |
| AM_001 | AM01 | L021 |
| AM_002 | AM02 | L033 |
| AM_003 | AM03 | L037 |
| AM_004 | AM04 | L044 |
| AM_005 | AM05 | L051 |
| AM_006 | AM06 | L054 |
| AM_007 | AM07 | L068 |
| CP_001 | CP01 | L010 |
| CP_002 | CP02 | L014 |
| CP_003 | CP03 | L030 |
| CP_004 | CP04 | L040 |
| CP_005 | CP05 | L063 |
| CV_001 | CV01 | L061 |
| CV_002 | CV02 | L060 |
| CV_003 | CV03 | L038 |
| CV_004 | CV04 | L047 |
| CV_005 | CV05 | L049 |
| CV_006 | CV06 | L052 |
| CV_007 | CV07 | L053 |
| CV_008 | CV08 | L055 |
| CV_009 | CV09 | L062 |
| CV_010 | CV10 | L064 |
| CV_011 | CV11 | L067 |
| RF_001 | RF01 | L026 |
| RF_002 | RF02 | L027 |
| RF_003 | RF03 | L028 |
| RF_004 | RF04 | L029 |
| RF_005 | RF05 | L057 |
| RF_006 | RF06 | L059 |
| ST_001 | ST01 | L035 |
| ST_002 | ST02 | L043 |
| ST_003 | ST03 | L045 |
| ST_004 | ST04 | L058 |
| ST_005 | ST05 | L042 |
| ST_006 | ST06 | L034 |
| ST_007 | ST07 | L032 |
| ST_008 | ST08 | L015 |
| JJ_001 | JJ01 | L046 |
| LT_001 | LT01 | L001 |
| LT_002 | LT02 | L002 |
| LT_003 | LT03 | L007 |
| LT_004 | LT04 | L019 |
| LT_005 | LT05 | L016 |
| LT_006 | LT06 | L017 |
| LT_007 | LT07 | L018 |
| LT_008 | LT08 | L022 |
| LT_009 | LT09 | L036 |
| LT_010 | LT10 | L041 |
| LT_011 | LT11 | L065 |
| LT_012 | LT12 | L009 |
| LT_013 | LT13 | L050 |
| TQ_001 | TQ01 | L056 |

Rules without legacy codes (added after L-code era): AL_008, AL_009, AM_008,
AM_009, CV_012, LT_014, LT_015, ST_009, ST_010, ST_011, ST_012, TQ_002, TQ_003.

---

## Phase 3: Semantic Depth — Close "Partial" Gaps

60 rules are currently "partial". They fall into three tiers:

### Tier 1: Upgrade parity rules to core AST rules (high value)

These parity rules handle semantic concerns that regex cannot do reliably.
Migrate them to proper AST implementations.

| Rule | Current Engine | SQLFluff Behavior Gap |
|---|---|---|
| AL_001 (aliasing.table) | Lexical | Needs to distinguish implicit vs explicit `AS`; handle dialect-specific aliasing |
| AL_002 (aliasing.column) | Lexical | Same as AL_001 for column aliases |
| AL_004 (aliasing.unique.table) | Lexical | Needs AST-level alias tracking across FROM/JOIN |
| AL_005 (aliasing.unused) | Core (partial) | Missing dialect-specific exceptions (VALUES, LATERAL) |
| AL_008 (aliasing.unique.column) | Lexical | Needs AST-level SELECT projection alias tracking |
| CV_003 (convention.select_trailing_comma) | Lexical | Needs token-aware trailing comma detection |
| CV_006 (convention.terminator) | Lexical | Needs statement boundary awareness |
| ST_005 (structure.subquery) | Lexical | Needs AST subquery detection in FROM/JOIN |
| ST_010 (structure.constant_expression) | Core (partial) | Broader detection: 1=1 across more contexts |
| ST_011 (structure.unused_join) | Core (partial) | Broader: check all join types, not just outer |

### Tier 2: Add missing configuration options (medium value)

These rules work but lack SQLFluff config knobs that users depend on:

| Rule | Missing Config | SQLFluff Default |
|---|---|---|
| AL_005 (aliasing.unused) | `alias_case_check` | dialect-dependent |
| AL_007 (aliasing.forbid) | `force_enable` | `False` (disabled by default) |
| AM_005 (ambiguous.join) | `fully_qualify_join_types` | `inner` |
| AM_006 (ambiguous.column_references) | `group_by_and_order_by_style` | `consistent` |
| CP_001 (capitalisation.keywords) | `capitalisation_policy`, `ignore_words` | `consistent` |
| CP_002–005 | `extended_capitalisation_policy` | `consistent` |
| LT_005 (layout.long_lines) | `max_line_length` (configurable) | 80 |
| LT_009 (layout.select_targets) | `wildcard_policy` | `single` |
| RF_001 (references.from) | `force_enable` | `False` |
| RF_003 (references.consistent) | `single_table_references`, `force_enable` | `consistent`, `False` |
| RF_006 (references.quoting) | `prefer_quoted_identifiers`, `case_sensitive` | `False`, `False` |
| ST_005 (structure.subquery) | `forbid_subquery_in` | `both` |
| ST_009 (structure.join_condition_order) | `preferred_first_table_in_join_clause` | `earlier` |

### Tier 3: Improve lexical/document rule accuracy (lower priority)

Remaining parity work in this tier is primarily lexical/style coverage
(`LT` and `JJ`, plus SQLFluff configuration-depth parity for `CP`) where
behavior still relies on lightweight token heuristics.
These should be migrated to token-stream analysis per the linter architecture
plan (Phase 2 of `linter-architecture.md`), but are lower priority because:

- They mostly handle formatting/style (lower impact than semantic rules).
- Lightweight token heuristics produce acceptable results for common patterns.
- Full token-stream engine is a prerequisite (already planned).

---

## Phase 4: Fix Coverage Gaps

10 rules currently lack `--fix` support. Priority order by user impact:

### High Priority (semantic fixers)

| Rule | SQLFluff Fix | Fix Strategy |
|---|---|---|
| AM_001 (ambiguous.distinct) | No (SQLFluff also lacks fix) | Skip — no SQLFluff parity needed |
| AM_009 (ambiguous.order_by_limit) | No (SQLFluff also lacks fix) | Skip — no SQLFluff parity needed |
| AL_005 (aliasing.unused) | Yes | AST rewrite: remove unused alias from table factor |
| RF_001 (references.from) | No (SQLFluff also lacks fix) | Skip |
| RF_002 (references.qualification) | No (SQLFluff also lacks fix) | Skip |

### Medium Priority (style fixers)

| Rule | Fix Strategy |
|---|---|
| CV_008 (convention.left_join) | AST rewrite: swap RIGHT JOIN → LEFT JOIN with table reorder |
| AM_004 (ambiguous.column_count) | No fix (SQLFluff also lacks fix) |
| AM_006 (ambiguous.column_references) | No fix (SQLFluff also lacks fix) |
| AM_007 (ambiguous.set_columns) | No fix (SQLFluff also lacks fix) |
| ST_003 (structure.unused_cte) | No fix (SQLFluff also lacks fix) |

After filtering out rules where SQLFluff itself lacks fix support, the
actual fix gap is:

| Rule | Needs Fix |
|---|---|
| AL_005 (aliasing.unused) | Yes — remove unused alias |
| CV_008 (convention.left_join) | Yes — rewrite to LEFT JOIN |

All other missing fixers match SQLFluff behavior (also unfixable).

---

## Phase 5: `--noqa` Comment Support

SQLFluff supports inline rule suppression:

```sql
SELECT * FROM t  -- noqa: AL01
SELECT * FROM t  -- noqa: AL01, AM02
SELECT * FROM t  -- noqa (disable all)
```

### Implementation

1. Parse `-- noqa` comments from token stream during `LintDocument` construction.
2. Build a suppression map: `line_number → Set<rule_code>` (or `all`).
3. Filter issues against suppression map before final output.
4. Support all code formats: `AL01`, `AL_001`, `aliasing.table`, `L011`.

---

## Execution Order and Dependencies

```
Phase 1 (Code Renumbering)
    ↓
Phase 2 (Names + Descriptions)
    ↓
Phase 3 Tier 1 (AST Upgrades)    Phase 4 (Fix Gaps)    Phase 5 (--noqa)
         ↓                              ↓
Phase 3 Tier 2 (Config Options)
         ↓
Phase 3 Tier 3 (Lexical Migrations — per linter-architecture.md Phase 2)
```

Phase 1 must land first because it changes every code reference.
Phases 3–5 can proceed in parallel after Phase 2.

---

## Appendix A: Complete Code Alignment Summary

### Before (current) → After (target)

**28 renames required:**

```
AL_001 → AL_003    AL_002 → AL_005    AL_003 → AL_001    AL_004 → AL_002    AL_005 → AL_004
AM_001 → AM_002    AM_002 → AM_009    AM_003 → AM_001    AM_005 → AM_003    AM_006 → AM_005
AM_007 → AM_006    AM_008 → AM_007    AM_009 → AM_008
CV_001 → CV_002    CV_002 → CV_004    CV_003 → CV_005    CV_004 → CV_008    CV_005 → CV_001
CV_006 → CV_003    CV_007 → CV_006    CV_008 → CV_007
ST_001 → ST_003    ST_002 → ST_001    ST_003 → ST_004    ST_004 → ST_007    ST_005 → ST_002
ST_006 → ST_005    ST_007 → ST_006
```

**44 codes already correct (no rename):**

```
AL_006  AL_007  AL_008  AL_009
AM_004
CP_001  CP_002  CP_003  CP_004  CP_005
CV_009  CV_010  CV_011  CV_012
JJ_001
LT_001  LT_002  LT_003  LT_004  LT_005  LT_006  LT_007  LT_008
LT_009  LT_010  LT_011  LT_012  LT_013  LT_014  LT_015
RF_001  RF_002  RF_003  RF_004  RF_005  RF_006
ST_008  ST_009  ST_010  ST_011  ST_012
TQ_001  TQ_002  TQ_003
```
