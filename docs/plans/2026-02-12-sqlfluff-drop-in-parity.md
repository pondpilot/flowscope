# SQLFluff Drop-In Parity Plan

Status: In Progress
Created: 2026-02-12
Last Updated: 2026-02-14
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
  - Supports SQLFluff-style `noqa: disable=all` / `noqa: enable=all` range directives, including block-comment forms with SQLFluff-compatible guardrails for valid marker placement.
- API schema snapshot updated to include lint output changes (`sqlfluffName`).
- Phase 4 fix-gap items from this plan are implemented:
  - `AL_005` fixer path is wired to canonical code gating.
  - `CV_008` fixer rewrites simple `RIGHT JOIN` patterns to `LEFT JOIN` with table reorder.
- Phase 1 docs cleanup is implemented:
  - `docs/sqlfluff-gap-matrix.md` mapping rows now use canonical SQLFluff-aligned FlowScope rule codes and updated module references.
- Phase 3 Tier 1 progress:
  - `ST_005` moved from parity regex handling to a dedicated core AST rule implementation.
  - `ST_005` now supports `forbid_subquery_in` (`both`/`join`/`from`) through `lint.ruleConfigs`, with SQLFluff-aligned default behavior set to `join`.
  - `ST_005` now includes SQLFluff correlated-subquery parity for JOIN-derived queries by exempting derived subqueries that reference outer query sources (for example correlated `WHERE ce.name = pd.name` cases).
  - `AL_004` moved from parity into a dedicated core AST rule (`al_004.rs`) and parity registration was removed.
  - `AL_004` now also checks duplicate implicit table-name aliases (e.g., same base table name across schemas without explicit aliases) and parent-scope alias collisions in nested subqueries (excluding the subquery wrapper alias), matching SQLFluff AL04 coverage more closely.
  - `AL_004` now also checks outer-scope alias collisions in expression subqueries (`WHERE`/`IN`/`EXISTS`), including implicit table-name collisions such as `FROM tbl ... (SELECT ... FROM tbl)`.
  - `AL_004` now supports quote-aware `alias_case_check` through `lint.ruleConfigs`.
  - `AL_004` `alias_case_check` now aligns closer to SQLFluff mode semantics for `quoted_cs_naked_upper` and `quoted_cs_naked_lower` (quoted aliases case-sensitive; naked aliases case-folded per configured mode).
  - `AL_001` moved from parity into a dedicated core rule module (`al_001.rs`) and parity registration was removed.
  - `AL_001` was further upgraded to AST-driven table-factor alias traversal with span-bounded source analysis for explicit vs implicit `AS` detection (tokenizer pass removed).
  - `AL_001` now applies alias-style checks to `MERGE` target/source aliases (for example BigQuery `MERGE dataset.inventory t USING ... s`) for SQLFluff AL01 parity.
  - `AL_002` moved from parity into a dedicated core rule module (`al_002.rs`) and parity registration was removed.
  - `AL_002` was further upgraded to AST-driven SELECT projection alias traversal with span-bounded source analysis for explicit vs implicit `AS` detection (tokenizer pass removed).
  - `AL_002` now excludes TSQL assignment-style projection aliases (`SELECT alias = expr`) from AL02 violations, matching SQLFluff behavior.
  - `AL_001`/`AL_002` now support SQLFluff-style `aliasing` mode (`explicit`/`implicit`) via `lint.ruleConfigs`.
  - `AL_008` moved from parity into a dedicated core AST rule (`al_008.rs`) and parity registration was removed.
  - `AL_008` now checks duplicate output names from unaliased column references in SELECT projections (in addition to explicit aliases), improving SQLFluff AL08 parity.
  - `AL_008` now supports quote-aware `alias_case_check` through `lint.ruleConfigs`.
  - `AL_008` `alias_case_check` now aligns closer to SQLFluff mode semantics for `quoted_cs_naked_upper` and `quoted_cs_naked_lower` (quoted aliases case-sensitive; naked aliases case-folded per configured mode).
  - `AL_005` was expanded to cover derived-table aliases while adding SQLFluff-compatible `LATERAL` and `VALUES` exceptions to reduce false positives.
  - `AL_005` now supports `alias_case_check` through `lint.ruleConfigs`.
  - `AL_005` `alias_case_check` now aligns closer to SQLFluff mode semantics for `quoted_cs_naked_upper` and `quoted_cs_naked_lower` (quoted identifiers case-sensitive; naked identifiers case-folded per configured mode).
  - `AL_005` alias-reference traversal now also accounts for usage in `QUALIFY`, named `WINDOW`, `DISTINCT ON`, `PREWHERE`, `CLUSTER BY`/`DISTRIBUTE BY`/`SORT BY`, `LATERAL VIEW`, and `CONNECT BY` clauses.
  - `AL_005` now also checks single-table scopes (not only multi-source `FROM`/`JOIN` clauses), matching SQLFluff AL05 behavior for unused aliases like `FROM users u` when `u` is never referenced.
  - `AL_005` now also counts alias usage from join relation table-factor expressions (for example `LATERAL (SELECT u.id)` and `UNNEST(u.tags)`), reducing false positives where aliases are only referenced by later join sources.
  - `AL_005` now aligns closer to SQLFluff scope handling by ignoring derived-subquery wrapper aliases and value-table-function aliases, while recursively analyzing nested derived-query bodies for inner alias violations (e.g. `SELECT * FROM (SELECT * FROM my_tbl AS foo)` now flags `foo`).
  - `AL_005` now includes dialect-aware function-argument alias handling for BigQuery `TO_JSON_STRING(<table_alias>)`, while keeping ANSI behavior that still flags that alias form as unused.
  - `AL_005` now includes SQLFluff Redshift `QUALIFY` ordering parity: `QUALIFY` alias references are counted only when `QUALIFY` directly follows FROM/JOIN (and are ignored for alias-usage counting when a `WHERE` clause appears before `QUALIFY`).
  - `AL_005` now includes SQLFluff BigQuery/Redshift implicit array-relation parity by counting alias usage from table-factor forms like `FROM t, t.arr` and applying alias exceptions for relation aliases on those array/super-array factors (for example `FROM t, t.super_array AS x`).
  - `AL_005` now includes SQLFluff repeated-self-join alias parity by exempting sibling aliases on the same base relation when one alias for that repeated relation is referenced.
  - `AL_005` now also checks `DELETE ... USING` source scopes, including nested `WITH` subqueries, matching SQLFluff AL05 behavior for unused aliases inside Snowflake delete-derived CTEs.
  - `AL_005` now includes dialect-aware quoted/unquoted identifier normalization in default (`dialect`) mode, aligning SQLFluff AL05 alias usage matching across dialect-specific casefold behavior (for example Postgres/Redshift lower-folding, Snowflake upper-folding, and case-insensitive quoted identifiers for DuckDB/Hive/SQLite).
  - `AL_005` now includes SQLFluff Redshift QUALIFY parity for unqualified alias-prefixed identifiers (for example `ss_sold_date` / `ss_sales_price` counting as `ss` usage when QUALIFY follows FROM/JOIN directly).
  - `CV_003` moved from parity into a dedicated core rule module (`cv_003.rs`) and parity registration was removed.
  - `CV_003` was further upgraded from regex scanning to AST-driven SELECT traversal with span-based trailing-comma detection at clause boundaries.
  - `CV_003` now supports `select_clause_trailing_comma` (`forbid`/`require`) through `lint.ruleConfigs`.
  - `CV_006` moved from parity into a dedicated core rule module (`cv_006.rs`) and parity registration was removed.
  - `CV_006` now aligns with SQLFluff TSQL batch handling by splitting MSSQL statement ranges on `GO` separators before best-effort parsing, preventing parser dropouts from masking final-semicolon checks.
  - `CV_006` terminator detection is now tokenizer/span-driven at statement boundaries (with byte-scan fallback), reducing brittle raw-SQL scanning for semicolon and trailing-comment/newline checks.
  - `CV_006` now treats standalone MSSQL `GO` batch separator lines as non-statement tokens in last-statement detection, so `require_final_semicolon` checks continue to apply to the final SQL statement even when a trailing `GO` line is present.
  - `CV_006` multiline-semicolon style checks now derive multiline status from tokenizer trivia (whitespace/comment tokens) instead of raw newline searching, avoiding false multiline classification from newline characters inside string literals.
  - `ST_010` constant-expression detection scope was broadened beyond SELECT traversal to also check `UPDATE`/`DELETE` predicates and `MERGE ... ON`.
  - `ST_010` now aligns closer with SQLFluff ST10 comparison semantics by detecting equivalent-expression predicate comparisons across `=`/`!=`/`<`/`>`/`<=`/`>=` (e.g. `x = x`, `x < x`, `'A'||'B' = 'A'||'B'`) with operator-side guardrails that defer nested comparison-expression operands while preserving SQLFluff-style literal handling (`1=1`/`1=0` allow-list and non-equality literal-vs-literal deferral).
  - `ST_010` now reports per-occurrence violations for multiple constant predicates within a statement instead of collapsing to a single statement-level issue.
  - `ST_011` now aligns closer to SQLFluff ST11 by scoping checks to explicit OUTER joins, tracking only joined relations (not base `FROM` sources), deferring when unqualified refs exist, accounting for inter-join `ON`-clause references plus wildcard usage (`alias.*` and `*`), counting `DISTINCT ON (...)` references, query-level `ORDER BY` references, `CLUSTER BY`/`DISTRIBUTE BY` references, `LATERAL VIEW`/`CONNECT BY` references, named `WINDOW` clause references, references from later join relation expressions (e.g. `UNNEST(g.nested_array)`), and quoted joined-source normalization across MySQL backticks and MSSQL brackets.
  - `ST_011` now also evaluates multi-root `FROM` clauses (e.g., comma-joins combined with OUTER joins) instead of skipping analysis whenever more than one top-level `FROM` entry exists.
  - `ST_011` now also treats Snowflake qualified wildcard `EXCLUDE` projections (`alias.* EXCLUDE ...`) as joined-source references, matching SQLFluff ST11 fixture behavior.
  - `ST_003` unused-CTE issue spans now derive from AST identifier spans (`cte.alias.name.span`) before fallback SQL-text search, reducing span heuristics for CTE-definition highlighting.
  - CLI lint mode now supports `--rule-configs` JSON for SQLFluff-style per-rule options (for example `{"structure.subquery":{"forbid_subquery_in":"both"}}`), enabling config-aware fixture parity replay outside unit tests.
  - CLI lint mode now honors templating in lint requests (when `--template` is provided), adds a Jinja fallback retry for parse-erroring inputs that contain template markers (`{{`, `{%`, `{#`), and now retries with dbt template mode when Jinja emits template errors (for macros like `ref`/`source`), improving SQLFluff-style templated lint parity.
  - CLI dialect parsing now accepts SQLFluff `sparksql` as an alias to FlowScope `databricks`, reducing fixture replay dialect-skew for SparkSQL-style lint cases.
  - Parser fallback now normalizes escaped-quoted identifier edge cases for BigQuery/ClickHouse and now applies trailing-comma-before-`FROM` fallback normalization across dialects, eliminating SQLFluff fixture parse blockers (including ANSI ST11 trailing-comma cases).
  - Config-aware SQLFluff fixture replay for `AL05`/`ST05`/`ST11` now reports zero mismatches (104/104, 40/40, and 22/22 cases respectively).
  - Config-aware SQLFluff fixture replay for `AL01`/`AL02`/`AL04` now reports zero mismatches (14/14, 9/9, and 10/10 cases respectively).
  - Config-aware SQLFluff fixture replay for `CV06` now reports zero mismatches across supported dialect fixtures (54/54).
  - Tier 1 supported-dialect fixture bundle replay (`AL01`/`AL02`/`AL04`/`AL05`/`AL08`/`CV03`/`CV06`/`ST05`/`ST10`/`ST11`) now reports zero mismatches (286/286 checked; unsupported dialect fixtures skipped).
  - `AM_004`/`AM_007` wildcard-width resolution now also handles declared CTE column lists, table-factor alias column lists (`AS alias(col1, ...)`), and aliased nested-join table factors (including `USING(...)` width deduction plus `NATURAL JOIN` overlap deduction when both sides expose deterministic output column names; unknown wildcard sources remain conservatively unresolved).
- Additional AST-driven migration progress beyond Tier 1:
  - `AL_006` moved from parity regex handling to a dedicated core AST rule (`al_006.rs`).
  - `AL_006` now supports configurable `min_alias_length` / `max_alias_length` through `lint.ruleConfigs`, with SQLFluff-aligned default behavior leaving `max_alias_length` unset unless configured.
  - `AL_003` now supports configurable `allow_scalar` through `lint.ruleConfigs`, with SQLFluff-aligned default `allow_scalar=true`.
  - `AL_007` moved from parity regex handling to a dedicated core AST rule (`al_007.rs`).
  - `AL_007` now supports `force_enable` through `lint.ruleConfigs` and is now disabled by default to align with SQLFluff behavior.
  - `AL_007` now broadens AST scope beyond single-table SELECTs by flagging unnecessary base-table aliases across multi-source `FROM`/`JOIN` clauses while allowing aliases for repeated self-join table references.
  - `AL_009` moved from parity regex handling to a dedicated core AST rule (`al_009.rs`).
  - `AL_009` now supports `alias_case_check` through `lint.ruleConfigs` and applies quote-aware case matching for self-alias detection.
  - `AL_009` `alias_case_check` now aligns closer to SQLFluff mode semantics for `quoted_cs_naked_upper` and `quoted_cs_naked_lower` (quoted aliases case-sensitive; naked aliases case-folded per configured mode).
  - `RF_004` moved from parity handling to a dedicated core rule module (`rf_004.rs`).
  - `RF_004` was further upgraded to AST-driven identifier analysis (expression identifiers, projection aliases, CTE identifiers, and table/join aliases plus table-name parts), eliminating SQL-string false positives from non-SQL string literals.
  - `RF_004` now supports SQLFluff-style `quoted_identifiers_policy` / `unquoted_identifiers_policy` and `ignore_words` / `ignore_words_regex` through `lint.ruleConfigs`.
  - `RF_001` now includes scope-aware AST traversal for `SELECT`/`UPDATE`/`DELETE`/`MERGE` plus PostgreSQL policy statements, supports correlated-subquery source resolution, handles multi-part qualifier matching (`schema.table.column`) with source-qualification guardrails, applies dialect-aware struct-field handling for BigQuery/Hive/Redshift and nested-field prefix handling for BigQuery/DuckDB/Hive/Redshift, recognizes trigger-only `OLD`/`NEW` pseudo references, and supports SQLFluff-style `force_enable` behavior through `lint.ruleConfigs`.
  - `RF_002` now includes recursive AST scope analysis with external-reference semantics for nested subqueries (including scalar/`EXISTS`/`IN` forms and nested derived-subquery cases), supports SQLFluff projection-alias sequencing semantics (`foo AS foo` flagged while later references to earlier aliases are allowed), supports `ignore_words` / `ignore_words_regex` and `subqueries_ignore_external_references` via `lint.ruleConfigs`, handles BigQuery value-table function (`UNNEST`) source-count and alias exemptions, exempts declared BigQuery script variables and `@` variables, and keeps datepart keyword argument false-positive guards (e.g., `timestamp_trunc(..., month)`, `datediff(year, ...)`).
  - Supported-dialect SQLFluff fixture replay for references rules now reports zero mismatches:
    - `RF01`: 49/49 supported-dialect cases matched.
    - `RF02`: 51/51 supported-dialect cases matched.
  - `RF_003` now supports `single_table_references` (`consistent`/`qualified`/`unqualified`) and `force_enable` through `lint.ruleConfigs`, and now treats qualified wildcards (`alias.*`) as qualified references in consistency checks.
  - `RF_005` moved from parity handling to a dedicated core rule module (`rf_005.rs`).
  - `RF_005` was further upgraded to AST-driven quoted-identifier traversal, replacing raw quote-regex scanning.
  - `RF_005` now supports `quoted_identifiers_policy` / `unquoted_identifiers_policy`, `additional_allowed_characters`, and `ignore_words` / `ignore_words_regex` through `lint.ruleConfigs`.
  - `RF_006` moved from parity handling to a dedicated core rule module (`rf_006.rs`).
  - `RF_006` was further upgraded to AST-driven quoted-identifier traversal, replacing raw quote-regex scanning.
  - `RF_006` now supports `prefer_quoted_identifiers`, `prefer_quoted_keywords`, `quoted_identifiers_policy` / `unquoted_identifiers_policy`, `ignore_words` / `ignore_words_regex`, and `case_sensitive` through `lint.ruleConfigs`.
  - `ST_002` moved from parity regex handling to a dedicated core AST rule (`st_002.rs`).
  - `ST_006` moved from parity regex handling to a dedicated core AST rule (`st_006.rs`).
  - `ST_008` moved from parity regex handling to a dedicated core AST rule (`st_008.rs`).
  - `ST_009` now supports `preferred_first_table_in_join_clause` (`earlier`/`later`) through `lint.ruleConfigs`.
  - `ST_012` moved from parity handling to a dedicated core rule module (`st_012.rs`).
  - `ST_012` was further upgraded from regex scanning to tokenizer-level semicolon sequencing, eliminating string-literal/comment false positives for consecutive-semicolon detection.
  - `ST_012` semicolon sequencing is now dialect-aware and span-based (active-dialect tokenization), including dialect-specific comment trivia handling (for example MySQL `#` comments).
  - `TQ_001` moved from parity handling to a dedicated core rule module (`tq_001.rs`).
  - `TQ_001` was further upgraded to AST-driven procedure-name analysis via `Statement::CreateProcedure`, replacing SQL text regex scanning.
  - `TQ_002` moved from parity handling to a dedicated core rule module (`tq_002.rs`).
  - `TQ_002` was further upgraded to AST-driven procedure-body analysis via `CreateProcedure` `ConditionalStatements::BeginEnd` detection, replacing SQL text regex scanning.
  - `TQ_003` moved from parity handling to a dedicated core rule module (`tq_003.rs`).
  - `TQ_003` was further upgraded from regex scanning to active-dialect token/line-aware repeated-`GO` separator detection, reducing string-literal false positives.
  - `TQ_003` now consumes the shared document token stream from lint context (`parse once, tokenize once`) before fallback tokenization.
  - `TQ_003` empty-batch detection now derives GO-separator and in-between-line emptiness from token-span line summaries (including comment-line handling) instead of `sql.lines()` text trimming.
  - `CV_001` moved from parity handling to a dedicated core rule module (`cv_001.rs`).
  - `CV_001` was further upgraded from regex checks to AST expression traversal for not-equal comparisons, reducing lexical false positives.
  - `CV_001` not-equal style detection is now AST span-driven (operator classification from source slices between `Expr` operand spans), replacing tokenizer-wide `Token::Neq` scanning.
  - `CV_001` not-equal operator-style classification now uses shared token-stream slices between AST operand spans (with fallback), reducing raw between-operand byte scanning.
  - `CV_001` not-equal operator-style classification is now token-stream-only (shared document tokens with statement-token fallback), with raw between-operand byte-scan fallback removed.
  - `CV_001` now supports `preferred_not_equal_style` (`consistent`/`c_style`/`ansi`) through `lint.ruleConfigs`.
  - `CV_003` trailing-comma boundary detection now uses token-stream classification (shared document tokens first, then fallback tokenization) for first-significant-token checks, replacing raw clause-suffix byte scanning.
  - `CV_003` trailing-comma first-significant-token checks are now token-stream-only (shared document tokens with statement-token fallback), with raw clause-suffix byte-scan fallback removed.
  - `CV_004` now supports `prefer_count_1` / `prefer_count_0` through `lint.ruleConfigs`.
  - `CV_004` fixer now rewrites both `COUNT(1)` and `COUNT(0)` to `COUNT(*)` under default preference, aligning fix behavior with current violation detection.
  - `CV_008` fixer now rewrites both simple and chained/nested `RIGHT JOIN` patterns into `LEFT JOIN` form via AST join-tree rewrites (operand swap plus join-operator normalization).
  - `CV_006` now supports `multiline_newline` / `require_final_semicolon` through `lint.ruleConfigs`.
  - `CV_006` semicolon-style analysis is now tokenizer/span-only (byte-scan fallback removed), including tokenized last-statement detection with MSSQL `GO` batch-separator awareness and tokenized trailing-comment checks for multiline newline-style terminators.
  - `CV_006` MSSQL standalone-`GO` detection now derives from token-line content (not raw line slicing), preserving SQLFluff-style “standalone separator only” behavior.
  - Lint execution now threads the document-level tokenizer stream through rule context (`parse once, tokenize once` path), and `CV_006` consumes that shared token stream before fallback tokenization.
  - `CV_006` multiline fallback classification now counts CRLF/CR/LF line breaks instead of raw `contains('\n')`, keeping fallback behavior aligned with tokenizer line-break semantics.
  - `CV_007` moved from parity handling to a dedicated core rule module (`cv_007.rs`).
  - `CV_007` was further upgraded to AST-driven statement-shape detection (`Statement::Query` + wrapper `SetExpr::Query`), replacing SQL text `starts_with('(')/ends_with(')')` heuristics.
  - `CV_009` moved from parity handling to a dedicated core rule module (`cv_009.rs`).
  - `CV_009` was further upgraded to AST-driven traversal (table names, aliases, expression identifiers), replacing raw regex scanning and reducing string/comment false positives.
  - `CV_009` now supports configurable `blocked_words` / `blocked_regex` through `lint.ruleConfigs`.
  - `CV_010` moved from parity handling to a dedicated core rule module (`cv_010.rs`).
  - `CV_010` was further upgraded to AST-driven double-quoted identifier traversal, replacing raw quote-regex scanning.
  - `CV_010` now supports `preferred_quoted_literal_style` through `lint.ruleConfigs`.
  - `CV_010` `consistent` mode now triggers only when mixed single-quoted and double-quoted literal-like styles coexist in the same statement.
  - `CV_011` moved from parity handling to a dedicated core rule module (`cv_011.rs`).
  - `CV_011` was further upgraded to AST-driven cast-kind traversal (`CastKind::{Cast, TryCast, SafeCast, DoubleColon}`), replacing raw SQL `::`/`CAST(` regex scanning and reducing string-literal false positives.
  - `CV_011` now supports `preferred_type_casting_style` through `lint.ruleConfigs`.
  - `CV_012` now broadens AST join-operator handling to include `INNER JOIN` forms represented as `JoinOperator::Inner` without `ON/USING`, and now aligns closer to SQLFluff CV12 chain semantics by flagging only when all naked joins in a join chain are represented via WHERE join predicates.
  - `JJ_001` moved from parity handling to a dedicated core rule module (`jj_001.rs`).
  - `JJ_001` was further upgraded from regex matching to tokenizer/span-aware delimiter checks for Jinja padding.
  - `JJ_001` now also checks statement/comment closing tags (`%}`/`#}`) and supports trim-marker-safe padding detection for tags like `{{- ... -}}`.
  - `JJ_001` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `AM_002` bare-`UNION` issue spans now use active-dialect tokenized `UNION` keyword spans aligned to AST set-operation traversal order, replacing SQL-text keyword searching.
  - `AM_002` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `LT_010` moved from parity handling to a dedicated core rule module (`lt_010.rs`).
  - `LT_010` was further upgraded from regex scanning to active-dialect tokenizer line-aware SELECT modifier checks.
  - `LT_010` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `LT_011` moved from parity handling to a dedicated core rule module (`lt_011.rs`).
  - `LT_011` was further upgraded from regex scanning to active-dialect tokenizer line-aware set-operator placement checks.
  - `LT_011` now supports `line_position` (`alone:strict`/`leading`/`trailing`) through `lint.ruleConfigs`.
  - `LT_011` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `LT_012` moved from parity handling to a dedicated core rule module (`lt_012.rs`).
  - `LT_012` now enforces SQLFluff-style single trailing newline at EOF (flags both missing final newline and multiple trailing blank lines), and now derives trailing-content boundaries from tokenizer spans without raw-text fallback.
  - `LT_012` now consumes the shared document token stream for document-level tokenization before fallback tokenization.
  - `LT_012` document multiline gating now derives from token span line metadata (with CRLF-aware fallback) instead of raw `sql.contains('\n')`.
  - `LT_013` moved from parity handling to a dedicated core rule module (`lt_013.rs`).
  - `LT_013` was further upgraded from regex matching to direct leading-blank-line scanning.
  - `LT_013` now uses tokenizer-first start-of-file trivia detection without raw-text fallback for leading blank-line parity.
  - `LT_013` now consumes the shared document token stream for document-level tokenization before fallback tokenization.
  - `LT_015` moved from parity handling to a dedicated core rule module (`lt_015.rs`).
  - `LT_015` was further upgraded from line-splitting-only blank-line run detection to tokenizer-derived line occupancy without raw fallback, including single-line comment end-line handling for blank-line run counting.
  - `LT_015` now supports `maximum_empty_lines_inside_statements` / `maximum_empty_lines_between_statements` through `lint.ruleConfigs`.
  - `LT_015` now consumes the shared document token stream for statement/gap tokenization before fallback tokenization.
  - `LT_015` statement-range trimming and line-count bounds now derive from token spans first (with CRLF-aware fallback), reducing raw byte/`sql.lines()` dependence in blank-line run checks.
  - `LT_002` moved from parity handling to a dedicated core rule module (`lt_002.rs`).
  - `LT_002` now supports SQLFluff-style indentation config shapes across both `layout.indent` and top-level `indentation` sections (`indent_unit` / `tab_space_size`), enforces tab-vs-space indentation style, detects first-line indentation from full-statement source context, and now builds line-indentation snapshots from tokenizer spans (including comment lines) instead of line-splitting alone, consuming the shared document token stream before fallback tokenization.
  - `LT_003` moved from parity handling to a dedicated core rule module (`lt_003.rs`).
  - `LT_003` was further upgraded from regex scanning to active-dialect tokenizer line-aware trailing-operator checks.
  - `LT_003` now supports operator line-position configuration through `lint.ruleConfigs` (`line_position=leading|trailing`) and legacy SQLFluff `operator_new_lines` compatibility.
  - `LT_003` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `LT_004` moved from parity handling to a dedicated core rule module (`lt_004.rs`).
  - `LT_004` was further upgraded from regex scanning to active-dialect tokenizer-driven comma-spacing checks.
  - `LT_004` now supports comma line-position configuration through `lint.ruleConfigs` (`line_position=trailing|leading`) and legacy SQLFluff `comma_style` compatibility.
  - `LT_004` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `LT_001` moved from parity handling to a dedicated core rule module (`lt_001.rs`).
  - `LT_001` was further upgraded from deterministic raw-text scanning to tokenizer-with-span layout detection (JSON arrows, compact `text[` forms, numeric precision commas, and line-start `EXISTS (` patterns), reducing literal/comment false positives and consuming the shared document token stream before fallback tokenization.
  - `LT_005` moved from parity handling to a dedicated core rule module (`lt_005.rs`).
  - `LT_005` now supports configurable `max_line_length`, `ignore_comment_lines`, and `ignore_comment_clauses` through `lint.ruleConfigs`, including SQLFluff-style disabled checks when `max_line_length <= 0`, comma-prefixed and Jinja comment-line handling, and SQL `COMMENT` clause handling for ignore-comment-clause semantics.
  - `LT_005` long-line overflow detection now uses tokenizer/span-derived line analysis only, including Jinja-comment-safe sanitization for tokenization plus Jinja-aware line/comment-clause handling (raw fallback removed).
  - `LT_005` now consumes the shared document token stream for document-level tokenization before fallback tokenization (with Jinja-comment-aware fallback preserved).
  - Analyzer linting now runs `LT_005` for statementless/comment-only SQL inputs via document-level fallback, closing SQLFluff LT05 coverage gaps for comment-only files.
  - `LT_006` moved from parity handling to a dedicated core rule module (`lt_006.rs`).
  - `LT_006` was further upgraded from regex masking to active-dialect token-stream function-call spacing checks with context guards.
  - `LT_006` spacing checks now derive candidate function names from AST expression traversal and apply token/span adjacency checks only to those function-call identifiers (plus cast-style function keywords), reducing non-function false positives from pure token heuristics.
  - `LT_006` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `LT_007` moved from parity handling to a dedicated core rule module (`lt_007.rs`).
  - `LT_007` now includes source-aware templating parity: when templating is enabled, lint evaluation uses untemplated source slices for CTE close-bracket checks so SQLFluff whitespace-consuming Jinja forms (`{{- ... -}}`, `{#- ... -#}`, `{%- ... -%}`) no longer produce false positives.
  - `LT_007` closing-bracket checks are now AST-first (`Query.with.cte_tables` closing-paren metadata) with tokenizer-span matching for multiline close placement, and now use tokenizer fallback scanning (raw byte fallback removed) when AST/token span mapping is unavailable.
  - `LT_007` now consumes the shared document token stream for statement tokenization before fallback tokenization (templated-source fallback preserved).
  - `LT_007` multiline CTE-body detection now derives from token spans between CTE parens (CRLF/CR/LF-aware) instead of direct body-slice newline checks.
  - `LT_007` close-bracket own-line checks now derive from token-line content before the closing `)` (instead of raw line-prefix trimming).
  - `LT_007` document-token usage is now gated by span/source alignment plus whitespace-gap validation, replacing raw template-marker string checks while preserving templated-source fallback semantics.
  - `LT_008` moved from parity handling to a dedicated core rule module (`lt_008.rs`).
  - `LT_008` was further upgraded from raw byte/state scanning to AST/token-aware CTE suffix analysis using `Query.with.cte_tables` closing-paren tokens plus tokenizer span traversal for blank-line detection, consuming the shared document token stream before fallback tokenization.
  - `LT_009` moved from parity handling to a dedicated core rule module (`lt_009.rs`).
  - `LT_009` was further upgraded from regex masking to AST-backed SELECT target analysis with active-dialect token-aware clause layout checks (single-target newline semantics, multi-target line separation, and `FROM`-line checks).
  - `LT_009` now supports `wildcard_policy` (`single`/`multiple`) through `lint.ruleConfigs`.
  - `LT_009` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - Supported-dialect SQLFluff layout fixture replay now reports zero mismatches for upgraded layout rules:
    - `LT05`: 55/55 cases matched (with replay mapping from SQLFluff `core.max_line_length` to `layout.long_lines.max_line_length`).
    - `LT07`: 13/13 SQLFluff standard cases matched (including whitespace-consuming Jinja fixtures).
    - `LT09`: 37/37 cases matched.
  - `LT_014` moved from parity handling to a dedicated core rule module (`lt_014.rs`).
  - `LT_014` was further upgraded from regex masking to active-dialect token/line-aware major-clause placement checks.
  - `LT_014` now consumes the shared document token stream for statement tokenization before fallback tokenization.
  - `CP_001` moved from parity handling to a dedicated core rule module (`cp_001.rs`).
  - `CP_002` moved from parity handling to a dedicated core rule module (`cp_002.rs`).
  - `CP_003` moved from parity handling to a dedicated core rule module (`cp_003.rs`).
  - `CP_004` moved from parity handling to a dedicated core rule module (`cp_004.rs`).
  - `CP_004` was further upgraded from regex masking to active-dialect tokenizer-driven literal-token collection (`NULL`/`TRUE`/`FALSE`), reducing string/comment false positives.
  - `CP_005` moved from parity handling to a dedicated core rule module (`cp_005.rs`).
  - `CP_001` was further upgraded from regex masking to active-dialect tokenizer-driven tracked-keyword collection.
  - `CP_002` was further upgraded from regex masking to AST identifier-candidate traversal.
  - `CP_003` was further upgraded from regex scanning to AST expression traversal for function-name detection.
  - `CP_003` function-name detection is now AST-expression-driven (`Expr::Function` traversal), including bare function keyword forms (for example `CURRENT_TIMESTAMP`) without tokenizer fallback.
  - `CP_005` was further upgraded from regex masking to active-dialect tokenizer-driven type-keyword collection.
  - `CP_001` now supports `capitalisation_policy` / `ignore_words` / `ignore_words_regex` through `lint.ruleConfigs`.
  - `CP_002`-`CP_005` now support `extended_capitalisation_policy` / `ignore_words` / `ignore_words_regex` through `lint.ruleConfigs`; `CP_002` additionally supports SQLFluff-style `unquoted_identifiers_policy`.
  - `CP_002` consistent-policy handling now aligns closer to SQLFluff ambiguity behavior by evaluating shared style compatibility (`upper`/`lower`/`capitalise`) across identifier tokens, allowing single-letter ambiguous forms like `A` to align with capitalised tokens.
  - `CP_002` Pascal policy handling now aligns with SQLFluff fixture expectations for all-caps identifiers (for example `PASCALCASE` under `extended_capitalisation_policy=pascal`).
  - `CP_002` now includes Databricks/SparkSQL `SHOW TBLPROPERTIES ... (<property.path>)` identifier candidate extraction via AST `ShowVariable` handling, covering mixed-case property path violations (for example `created.BY.user`, `Created.By.User`).
  - `CP_001`/`CP_003`/`CP_004`/`CP_005` now run once per document with full-SQL lexical context and now also run in statementless parser-fallback mode, improving SQLFluff parity on parser-erroring fixture inputs while avoiding per-statement case-policy fragmentation.
  - `CP_001`/`CP_004`/`CP_005` now consume the shared document token stream for statement tokenization before fallback tokenization.
  - `CP_001`/`CP_004`/`CP_005` document-token usage is now gated by span/source word-alignment checks, replacing raw template-marker string checks for fallback routing.
  - SQLFluff-style `core.ignore_templated_areas` is now supported through `lint.ruleConfigs.core.ignore_templated_areas` for lexical CP document-scope checks, masking Jinja tag regions from case-policy evaluation when enabled.
  - `CP_001` consistent-policy handling now treats single tracked mixed-case tokens (for example `SeLeCt`) as violations, matching SQLFluff fixture expectations.
  - `CP_003` now includes bare-function keyword forms (`CURRENT_TIMESTAMP`, `CURRENT_DATE`, `CURRENT_USER`, etc.) in AST-based function case checks.
  - `CP_005` now includes broader SQL dialect type keyword coverage (`STRING`, `INT64`, `FLOAT64`, `BYTES`, `TIME`, `INTERVAL`, `STRUCT`, `ARRAY`, `MAP`, `ENUM`, `WITH`, `ZONE`) and now tracks user-defined type names introduced by `CREATE TYPE` / `ALTER TYPE` for downstream type-case checks.
  - Supported-dialect SQLFluff fixture replay for CP keyword/function/literal/type rules now reports zero mismatches on supported cases:
    - `CP01`: 27/27 checked, 0 mismatches.
    - `CP03`: 17/17 checked, 0 mismatches.
    - `CP04`: 5/5 checked, 0 mismatches.
    - `CP05`: 22/22 checked, 0 mismatches.
  - Shared AST traversal helpers (`visit_expressions`, `visit_selects_in_statement`) now include `UPDATE`/`DELETE`/`MERGE` statement expression paths plus richer function-argument traversal (`Named`, `ExprNamed`, subquery args, argument clauses), reducing non-SELECT blind spots for AST-driven lint rules.
  - Supported-dialect SQLFluff fixture replay for `CP02` now reports zero mismatches (44/44), and `CP02_LT01` now reports zero mismatches for CP02 expectations across supported templaters (4/4; one placeholder-templater case skipped).
  - `AM_005` now supports `fully_qualify_join_types` (`inner`/`outer`/`both`) through `lint.ruleConfigs`.
  - `AM_005` outer-mode qualification now uses AST join-operator variants for `LEFT`/`RIGHT` detection and keeps token-level fallback only for `FULL JOIN` vs `FULL OUTER JOIN` disambiguation.
  - `AM_005` explicit `FULL OUTER` lexical checks now consume the shared document token stream for statement tokenization before fallback tokenization.
  - `AM_005` fixer now honors `fully_qualify_join_types` modes: `inner` rewrites bare `JOIN` to `INNER JOIN`; `outer`/`both` also qualify `LEFT`/`RIGHT` joins and rewrite bare `FULL JOIN` keywords to `FULL OUTER JOIN` outside string literals.
  - `AM_006` now supports `group_by_and_order_by_style` (`consistent`/`explicit`/`implicit`) through `lint.ruleConfigs`.
  - Legacy `parity.rs` monolith was retired; rule registration now points only to dedicated `rules/<code>.rs` modules.

### Intentionally Removed

- Legacy SQLFluff `L0xx` support (Phase 2.4) was removed by decision:
  - No legacy-code resolution in `disabled_rules`.
  - No legacy-code resolution in `--noqa`.

### Remaining

- Phase 2 metadata parity:
  - SQLFluff canonical description text is not fully normalized across all rules.
- Phase 3 semantic-depth work remains open for Tier 2 and Tier 3.
  - Tier 1 AST rule migration is complete, and config-aware SQLFluff fixture replay is currently 100% on supported-dialect cases for `AL_001`, `AL_002`, `AL_004`, `AL_005`, `AL_008`, `CV_003`, `CV_006`, `ST_005`, `ST_010`, and `ST_011`.

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
| AL_005 (aliasing.unused) | Core (partial) | Remaining advanced dialect/scope edges beyond current `LATERAL`/`VALUES`, BigQuery `TO_JSON_STRING(<table_alias>)`, `DELETE ... USING`, and Redshift QUALIFY alias-prefix parity |
| AL_008 (aliasing.unique.column) | Lexical | Needs AST-level SELECT projection alias tracking |
| CV_003 (convention.select_trailing_comma) | Lexical | Needs token-aware trailing comma detection |
| CV_006 (convention.terminator) | Lexical | Needs statement boundary awareness |
| ST_005 (structure.subquery) | Lexical | Needs AST subquery detection in FROM/JOIN |
| ST_010 (structure.constant_expression) | Core (partial) | Broader detection: 1=1 across more contexts |
| ST_011 (structure.unused_join) | Core (partial) | Track SQLFluff-style OUTER-join scope, inter-join reference semantics, and wildcard/reference resolution edge cases (including dialect-specific wildcard variants) |

### Tier 2: Add missing configuration options (medium value)

Tier 2 configuration parity work is now complete for the rules tracked in this plan.

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

After filtering out rules where SQLFluff itself lacks fix support, there are
currently no remaining fix gaps for SQLFluff-fixable rules in this plan.

All remaining no-fix rules match SQLFluff behavior (also unfixable).

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
