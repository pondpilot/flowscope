# Linter Architecture Design

Status: In progress  
Owner: `flowscope-core`  
Last updated: 2026-02-13

## Context

FlowScope currently ships:

- 15 core lint rules implemented as dedicated AST rules.
- 57 SQLFluff parity rules, many implemented in a single parity module with regex or heuristic matching.

This gave strong coverage quickly, but it is not the long-term architecture for an industry-standard linter.

## Goals

- Robust correctness across dialects and real-world SQL.
- Sound architecture with explicit semantics and low false positives.
- Maintainable implementation with clear rule ownership and minimal coupling.
- Scalable rule engine that can grow without a monolith.
- Deterministic outputs and stable spans suitable for editor and CI usage.

## Non-Goals

- One-shot rewrite of all existing rules.
- "AST-only" implementation of purely lexical formatting rules.
- Perfect SQLFluff behavior clone across every dialect from day one.

## Architecture Principles

1. AST-first semantics
- Semantic rules must be driven by parsed AST plus scope/resolution context, not regex.
- Examples: aliasing semantics, reference qualification, join logic, set operation checks.

2. Token-aware style
- Formatting and trivia rules must use token stream data, not AST-only approximations.
- Examples: whitespace, newlines, comments, casing style, quoting style, Jinja padding.

3. Parse once, tokenize once
- Build a single lint document model per SQL input and reuse it across all rules.
- Avoid repeated parsing and repeated ad hoc string scans in each rule.

4. Stable rule contract
- Each rule gets structured input from the engine, not direct access to ad hoc helpers.
- Rule output must include deterministic code, message, severity, statement index, and span.

5. Dialect-explicit behavior
- Rule decisions must be dialect-aware and must not silently assume generic SQL semantics.
- When parser fallback is used, confidence should degrade explicitly.

6. Deterministic and testable
- Same input and config must always produce the same ordered issue set.
- Rules must be independently testable with focused fixtures.

7. Regex is migration glue, not architecture
- Existing regex heuristics can remain temporarily for parity continuity.
- New semantic rules must not be implemented with regex.

## Key Design Decisions

## Decision 1: Introduce a `LintDocument` model

The linter engine should construct a normalized input model once:

- `sql` (full source text)
- `dialect` and parser/fallback metadata
- parsed statements with statement ranges
- token stream with token spans and token kinds
- optional scope/resolution metadata for semantic rules

This becomes the only rule input surface.

## Decision 2: Split rules into 3 engines

1. Semantic engine
- Input: AST + scope/resolution context.
- Handles semantic correctness and structural SQL logic.

2. Lexical engine
- Input: token stream + token spans.
- Handles formatting/casing/quoting/comment-aware style rules.

3. Document engine
- Input: whole file/document metadata.
- Handles file-level checks (EOF newline, leading blank lines, batch separators).

## Decision 3: Replace parity monolith with one-rule-per-file modules

- Move from `parity.rs` monolith to `rules/<code>.rs` modules.
- Keep shared traversal and token utilities in common helpers.
- Preserve existing lint codes for API stability.

## Decision 4: Standardize span generation

- Primary span source: parser or tokenizer spans.
- Secondary span source: scoped fallback search only when necessary.
- No free-form "best guess" spans without explicit fallback path.

## Decision 5: Add rule metadata and confidence

Each issue should carry internal provenance metadata:

- engine type (`semantic`, `lexical`, `document`)
- confidence (`high`, `medium`, `low`)
- fallback source (if parser fallback or heuristic logic was used)

This supports telemetry, triage, and quality gates.

## Decision 6: Define fixability as a rule capability

Rule metadata should include whether a deterministic fix is supported.
- No inferred fix logic from message text.
- Fix support should be explicit and tested per rule.

## Proposed Execution Pipeline

1. Parse SQL into statements with selected dialect.
2. Tokenize full source with token spans.
3. Build `LintDocument` with statement ranges and shared metadata.
4. Optionally build scope/resolution context once for semantic rules.
5. Execute semantic, lexical, and document engines.
6. Normalize, sort, and deduplicate issues.
7. Emit final issues with deterministic ordering and stable spans.

## Migration Plan

## Phase 0: Foundation

- Add `LintDocument` and engine scaffolding.
- Add token stream provider in lint pipeline.
- Keep current rules running unchanged via adapter layer.

## Phase 1: High-risk semantic migrations

Migrate semantic-heavy heuristic rules first:
- references (`RF_001`, `RF_002`, `RF_003`)
- structure join/constant/unused checks (`ST_009`, `ST_010`, `ST_011`)
- ambiguous join/reference rules (`AM_006` to `AM_009`)
- convention join condition (`CV_012`)

## Phase 2: Lexical/style migrations

Move style-oriented checks to lexical engine:
- capitalization (`CP_*`)
- layout (`LT_*`)
- jinja padding (`JJ_001`)
- selected convention style rules (`CV_007`, `CV_010`, `CV_011`)

## Phase 3: Decommission parity monolith

- Remove migrated rules from `parity.rs`.
- Keep only temporary compatibility shims if needed.
- Delete shims after equivalent rule quality gates are met.

## Progress Snapshot (2026-02-12)

- [x] Phase 0 foundation shipped: `LintDocument` model, tokenization pass, and document-level lint execution path are live.
- [x] Engine split is active in linter orchestration: semantic + lexical + document passes run with deterministic sort/dedupe.
- [x] Issue provenance metadata is implemented (`lint_engine`, `lint_confidence`, `lint_fallback_source`).
- [x] Phase 1 AST migrations landed for: `AM_002`, `AM_004`-`AM_009`, `CV_001`, `CV_012`, `RF_001`-`RF_003`, `ST_003`, `ST_009`-`ST_011`.
- [x] `LINT_AM_009` now follows SQLFluff AM09 semantics via AST query-clause analysis, flagging LIMIT/OFFSET usage without ORDER BY across top-level and nested SELECTs.
- [x] `LINT_AM_004` now follows SQLFluff AM04 semantics via AST output-width analysis, flagging queries whose result column count is unknown due to unresolved wildcard expansion (`*`/`alias.*`) across CTE/subquery/set-operation scopes.
- [x] `LINT_AM_002` now follows SQLFluff AM02 core semantics by flagging bare `UNION` (without explicit `ALL`/`DISTINCT`), with CLI fixer behavior inserting explicit `DISTINCT` through AST set-operation quantifier rewrites (text-regex path removed), and dialect-scoped execution aligned to SQLFluff-supported dialects available in FlowScope.
- [x] `LINT_CV_002` now follows SQLFluff CV02 semantics and fixer behavior by flagging IFNULL/NVL function usage and rewriting to COALESCE.
- [x] `LINT_CV_005` now follows SQLFluff CV05 semantics and fixer behavior by flagging `= NULL`/`<> NULL` comparisons and rewriting to `IS [NOT] NULL`.
- [x] `LINT_ST_004` now follows SQLFluff ST04 semantics via AST CASE analysis, flagging flattenable nested CASE expressions in ELSE clauses (instead of depth-based heuristics); fixer parity now flattens eligible nested `ELSE CASE` branches into a single CASE.
- [x] `LINT_ST_007` now includes SQLFluff ST07 fixer parity via AST join-constraint rewrites, converting `JOIN ... USING (...)` to explicit `ON` predicates (including multi-column USING lists).
- [x] `LINT_ST_009` now includes SQLFluff ST09 fixer parity via AST expression rewrites, swapping reversed qualified equality sides in `JOIN ... ON` predicates.
- [x] `LINT_ST_006` now follows SQLFluff ST06 detection semantics via AST SELECT projection analysis (simple targets after leading complex expressions) and includes fixer parity via AST reordering.
- [x] `LINT_ST_002` now follows SQLFluff ST02 detection semantics via AST CASE analysis (repeated equality checks on a common operand) and includes fixer parity via AST CASE rewrites.
- [x] `LINT_ST_008` now follows SQLFluff ST08 detection semantics via AST SELECT analysis for `DISTINCT(<expr>)` and includes fixer parity via AST SELECT rewrite to `SELECT DISTINCT <expr>`.
- [x] `LINT_ST_010` now aligns closer to SQLFluff ST10 by focusing constant-expression detection on `=`/`!=`/`<>` predicate comparisons with operator-side guardrails and SQLFluff-style `1=1`/`1=0` literal allow-list handling, while still traversing SELECT/UPDATE/DELETE/MERGE predicate contexts.
- [x] `LINT_ST_011` now aligns closer to SQLFluff ST11 by scoping candidate checks to explicit OUTER joins, tracking only joined relations (not the base `FROM` source), deferring on unqualified references (RF02-style), accounting for references in other JOIN `ON` clauses, and treating both qualified wildcards (`alias.*`) and unqualified wildcard projections (`*`) as table references.
- [x] `LINT_AL_009` now follows SQLFluff AL09 core detection semantics via AST projection analysis for identifier/qualified-identifier self-alias patterns (`col AS col`), with quote-aware case matching and `alias_case_check` configuration support.
- [x] `LINT_AL_001` now uses AST-driven table-factor alias traversal with token-aware `AS` detection, replacing regex-based matching.
- [x] `LINT_AL_002` now uses AST-driven SELECT projection alias traversal with token-aware `AS` detection, replacing regex-based clause extraction.
- [x] `LINT_AL_004` now also checks implicit table-name aliases (no explicit `AS`) and parent-scope collisions for nested subqueries (excluding the subquery wrapper alias), so duplicate base table names across schemas and nested-scope alias collisions are linted alongside explicit duplicate aliases, and now supports quote-aware `alias_case_check` configuration.
- [x] `LINT_AL_008` now checks duplicate projected output names from both explicit aliases and unaliased column references (e.g., `foo`, `schema.foo`) in SELECT clauses, with quote-aware `alias_case_check` configuration support.
- [x] `lint.ruleConfigs` now supports per-rule configuration objects keyed by canonical/shorthand/dotted rule references; `LINT_AL_001` and `LINT_AL_002` use this for SQLFluff-style `aliasing=explicit|implicit`.
- [x] `LINT_AL_006` now runs as a dedicated AST rule via table-factor alias traversal and supports `min_alias_length` / `max_alias_length` via `lint.ruleConfigs` (default behavior preserves the existing max-length heuristic).
- [x] `LINT_AL_003` now supports `allow_scalar` via `lint.ruleConfigs` (default behavior remains FlowScope-backwards-compatible while enabling SQLFluff-style strictness when configured).
- [x] `LINT_AL_007` now runs as a dedicated AST rule via single-source SELECT analysis for unnecessary base-table aliases (current scope remains intentionally conservative).
- [x] `LINT_RF_004`/`LINT_RF_005`/`LINT_RF_006` are now split out of `parity.rs` into dedicated core modules (`rf_004.rs`-`rf_006.rs`); all three now use AST-driven traversal (`RF04` alias analysis, `RF05`/`RF06` shared quoted-identifier traversal).
- [x] `LINT_ST_012` and `LINT_TQ_001`-`LINT_TQ_003` are now split out of `parity.rs` into dedicated core modules (`st_012.rs`, `tq_001.rs`-`tq_003.rs`); `LINT_TQ_001`/`LINT_TQ_002` are AST-driven (`CreateProcedure` name/body analysis), and `LINT_ST_012`/`LINT_TQ_003` now use token-driven sequencing checks.
- [x] `LINT_CV_001`, `LINT_CV_007`, and `LINT_CV_009`-`LINT_CV_011` are now split out of `parity.rs` into dedicated core modules (`cv_001.rs`, `cv_007.rs`, `cv_009.rs`-`cv_011.rs`); `LINT_CV_007`, `LINT_CV_009`, `LINT_CV_010`, and `LINT_CV_011` are now AST-driven, and `LINT_CV_001` now uses token-aware operator scanning (plus `preferred_not_equal_style` config support) instead of regex.
- [x] `LINT_CV_004` now supports SQLFluff-style COUNT preference knobs (`prefer_count_1` / `prefer_count_0`) via `lint.ruleConfigs` while keeping AST expression traversal for detection.
- [x] `LINT_CV_006` now supports `multiline_newline` / `require_final_semicolon` via `lint.ruleConfigs` while keeping statement-boundary aware terminator checks.
- [x] `LINT_CV_009` now supports configurable `blocked_words` / `blocked_regex` via `lint.ruleConfigs` (AST traversal scope unchanged).
- [x] `LINT_CV_010` now supports `preferred_quoted_literal_style` via `lint.ruleConfigs` (current behavior remains narrower than full SQLFluff literal semantics).
- [x] `LINT_CV_011` now supports `preferred_type_casting_style` via `lint.ruleConfigs` (including `consistent`/`shorthand`/`cast`/`convert` preferences).
- [x] `LINT_LT_005` now supports `max_line_length`, `ignore_comment_lines`, and `ignore_comment_clauses` via `lint.ruleConfigs`.
- [x] `LINT_LT_009` now supports `wildcard_policy` (`single`/`multiple`) via `lint.ruleConfigs`.
- [x] `LINT_LT_015` now supports `maximum_empty_lines_inside_statements` / `maximum_empty_lines_between_statements` via `lint.ruleConfigs`.
- [x] `LINT_LT_003` now supports operator line-placement configuration via `lint.ruleConfigs` (`line_position=leading|trailing`, plus legacy SQLFluff `operator_new_lines=after|before` mapping).
- [x] `LINT_LT_004` now supports comma line-placement configuration via `lint.ruleConfigs` (`line_position=trailing|leading`, plus legacy SQLFluff `comma_style` mapping).
- [x] `LINT_ST_005` now supports `forbid_subquery_in` (`both`/`join`/`from`) via `lint.ruleConfigs`.
- [x] `LINT_ST_009` now supports `preferred_first_table_in_join_clause` (`earlier`/`later`) via `lint.ruleConfigs`.
- [x] `LINT_RF_001` now supports `force_enable` via `lint.ruleConfigs`.
- [x] `LINT_RF_002` now supports `force_enable` via `lint.ruleConfigs`.
- [x] `LINT_RF_003` now supports `single_table_references` (`consistent`/`qualified`/`unqualified`) and `force_enable` via `lint.ruleConfigs`, and treats qualified wildcards (`alias.*`) as qualified references for mixed-style detection.
- [x] `LINT_RF_006` now supports `prefer_quoted_identifiers` / `case_sensitive` via `lint.ruleConfigs`.
- [x] `LINT_AL_007` now supports `force_enable` via `lint.ruleConfigs`.
- [x] `LINT_AL_005` now supports `alias_case_check` (including SQLFluff-style casefolding modes) via `lint.ruleConfigs`.
- [x] `LINT_AM_005` now supports `fully_qualify_join_types` (`inner`/`outer`/`both`) via `lint.ruleConfigs`.
- [x] `LINT_AM_006` now supports `group_by_and_order_by_style` (`consistent`/`explicit`/`implicit`) via `lint.ruleConfigs`.
- [x] `LINT_CP_001` now supports `capitalisation_policy`, `ignore_words`, and `ignore_words_regex` via `lint.ruleConfigs`.
- [x] `LINT_CP_002`-`LINT_CP_005` now support `extended_capitalisation_policy`, `ignore_words`, and `ignore_words_regex` via `lint.ruleConfigs`.
- [x] `LINT_CV_003` now uses token/depth-aware SELECT-clause analysis for trailing-comma detection, replacing regex scanning, and supports SQLFluff-style `select_clause_trailing_comma` (`forbid`/`require`) via `lint.ruleConfigs`.
- [x] `LINT_JJ_001` and `LINT_LT_010`/`LINT_LT_011`/`LINT_LT_012`/`LINT_LT_013`/`LINT_LT_015` are now split out of `parity.rs` into dedicated core modules (`jj_001.rs`, `lt_010.rs`, `lt_011.rs`, `lt_012.rs`, `lt_013.rs`, `lt_015.rs`); `LINT_JJ_001` now uses delimiter scanning, `LINT_LT_010`/`LINT_LT_011` now use tokenizer line-aware checks, `LINT_LT_012` now enforces a single trailing newline at EOF, and `LINT_LT_013`/`LINT_LT_015` now use direct newline-run scanning instead of regex matching.
- [x] `LINT_LT_002`/`LINT_LT_003`/`LINT_LT_004`/`LINT_LT_007` are now split out of `parity.rs` into dedicated core modules (`lt_002.rs`, `lt_003.rs`, `lt_004.rs`, `lt_007.rs`); `LINT_LT_003`/`LINT_LT_004` now use tokenizer-based operator/comma layout checks, and `LINT_LT_007` now uses deterministic CTE sequence scanning instead of regex matching.
- [x] `LINT_LT_001`/`LINT_LT_005`/`LINT_LT_006`/`LINT_LT_008`/`LINT_LT_009`/`LINT_LT_014` are now split out of `parity.rs` into dedicated core modules (`lt_001.rs`, `lt_005.rs`, `lt_006.rs`, `lt_008.rs`, `lt_009.rs`, `lt_014.rs`); `LINT_LT_001` now uses deterministic layout-pattern scanners, `LINT_LT_006` uses token-stream spacing detection for function-like calls, `LINT_LT_009` uses tokenizer-located SELECT-line target counting, and `LINT_LT_014` uses token/line-aware major-clause placement checks instead of regex masking.
- [x] `LINT_CP_001`-`LINT_CP_005` are now split out of `parity.rs` into dedicated core modules (`cp_001.rs`-`cp_005.rs`); `LINT_CP_004` was migrated to tokenizer-driven literal detection and `LINT_CP_001`/`LINT_CP_002`/`LINT_CP_003`/`LINT_CP_005` are now tokenizer-driven (keyword/identifier/function/type token analysis), replacing regex + manual masking paths.
- [x] `LINT_AM_003` now follows SQLFluff AM03 semantics via AST `ORDER BY` analysis, flagging mixed implicit/explicit sort direction (including `NULLS` ordering cases) across nested query scopes; fixer parity now adds explicit `ASC` to implicit items in mixed clauses.
- [x] `LINT_AM_005` now supports SQLFluff AM05 fixer parity for default behavior by rewriting bare `JOIN` operators to explicit `INNER JOIN` via AST join-operator rewrites.
- [x] `LINT_AM_006` now follows SQLFluff AM06 default (`consistent`) semantics via AST traversal of `GROUP BY` / `ORDER BY` clauses, including nested-query precedence and rollup-style references.
- [x] `LINT_AM_008` now follows SQLFluff AM08 semantics via AST join-operator analysis (implicit cross join detection, with `WHERE` deferral to CV12 and UNNEST/CROSS/NATURAL/USING exclusions); fixer parity now rewrites eligible implicit joins to explicit `CROSS JOIN`.
- [x] `LINT_AM_007` now performs AST set-expression branch-width checks with deterministic wildcard resolution for CTE/derived sources, while unresolved wildcard expansions remain non-violating (SQLFluff-aligned behavior).
- [x] Parity monolith decommission is complete: migrated rule registrations and parity tests are removed, and `crates/flowscope-core/src/linter/rules/parity.rs` has been retired.
- [~] SQLFluff fixture adoption is in progress for migrated rules; AM02/AM03/AM04/AM05/AM06/AM07/AM08/AM09, CV02, CV05, ST02, ST04, ST06, ST07, ST08, and ST09 fixture cases were adopted for `LINT_AM_002`/`LINT_AM_003`/`LINT_AM_004`/`LINT_AM_005`/`LINT_AM_006`/`LINT_AM_007`/`LINT_AM_008`/`LINT_AM_009`/`LINT_CV_002`/`LINT_CV_005`/`LINT_ST_002`/`LINT_ST_004`/`LINT_ST_007`/`LINT_ST_006`/`LINT_ST_008`/`LINT_ST_009`, and additional rule-level coverage is still being expanded.
- [ ] Phase 2 lexical/style migrations remain open for semantic-depth parity improvements (token-aware behavior/configuration parity), with remaining major lexical work centered on LT/JJ families and SQLFluff configuration-depth gaps across CP/LT/JJ.

## Quality Gates

Each migrated rule must pass:

- correctness: fixture and regression coverage for trigger/non-trigger cases
- span quality: stable and accurate primary highlight span
- precision guardrails: false positive threshold on curated corpus
- performance: no meaningful regression on representative workloads
- parity continuity: no unintentional code/message regressions unless documented

## Risks and Mitigations

1. Parser limitations and missing AST locations
- Mitigation: token spans become first-class; fallback span logic remains explicit.

2. Dialect edge cases not fully supported upstream
- Mitigation: dialect-specific behavior tables and confidence downgrade on fallback paths.

3. Migration churn and temporary duplicate logic
- Mitigation: phased rule-by-rule migration with adapter compatibility layer.

## Success Criteria

- All semantic rules run through AST/scope engine.
- All style/layout rules run through lexical/document engines.
- `parity.rs` no longer acts as a long-term rule home.
- Rule additions are modular, testable, and engine-scoped by default.
- Lint output quality and determinism improve while preserving stable public rule codes.
