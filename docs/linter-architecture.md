# Linter Architecture Design

Status: In progress  
Owner: `flowscope-core`  
Last updated: 2026-02-12

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
- [x] `LINT_AM_002` now follows SQLFluff AM09 semantics via AST query-clause analysis, flagging LIMIT/OFFSET usage without ORDER BY across top-level and nested SELECTs.
- [x] `LINT_AM_004` now follows SQLFluff AM04 semantics via AST output-width analysis, flagging queries whose result column count is unknown due to unresolved wildcard expansion (`*`/`alias.*`) across CTE/subquery/set-operation scopes.
- [x] `LINT_AM_001` now follows SQLFluff AM02 core semantics by flagging bare `UNION` (without explicit `ALL`/`DISTINCT`), with CLI fixer behavior inserting explicit `DISTINCT` and dialect-scoped execution aligned to SQLFluff-supported dialects available in FlowScope.
- [x] `LINT_CV_001` now follows SQLFluff CV02 semantics and fixer behavior by flagging IFNULL/NVL function usage and rewriting to COALESCE.
- [x] `LINT_CV_003` now follows SQLFluff CV05 semantics and fixer behavior by flagging `= NULL`/`<> NULL` comparisons and rewriting to `IS [NOT] NULL`.
- [x] `LINT_ST_003` now follows SQLFluff ST04 semantics via AST CASE analysis, flagging flattenable nested CASE expressions in ELSE clauses (instead of depth-based heuristics); fixer parity now flattens eligible nested `ELSE CASE` branches into a single CASE.
- [x] `LINT_AM_005` now follows SQLFluff AM03 semantics via AST `ORDER BY` analysis, flagging mixed implicit/explicit sort direction (including `NULLS` ordering cases) across nested query scopes; fixer parity now adds explicit `ASC` to implicit items in mixed clauses.
- [x] `LINT_AM_006` now supports SQLFluff AM05 fixer parity for default behavior by rewriting bare `JOIN` operators to explicit `INNER JOIN` via AST join-operator rewrites.
- [x] `LINT_AM_007` now follows SQLFluff AM06 default (`consistent`) semantics via AST traversal of `GROUP BY` / `ORDER BY` clauses, including nested-query precedence and rollup-style references.
- [x] `LINT_AM_009` now follows SQLFluff AM08 semantics via AST join-operator analysis (implicit cross join detection, with `WHERE` deferral to CV12 and UNNEST/CROSS/NATURAL/USING exclusions); fixer parity now rewrites eligible implicit joins to explicit `CROSS JOIN`.
- [x] `LINT_AM_008` now performs AST set-expression branch-width checks with deterministic wildcard resolution for CTE/derived sources, while unresolved wildcard expansions remain non-violating (SQLFluff-aligned behavior).
- [~] Parity monolith decommission is in progress: migrated rule registrations and parity tests are removed from `parity.rs`; helper cleanup is still ongoing.
- [~] SQLFluff fixture adoption is in progress for migrated rules; AM02/AM03/AM04/AM05/AM06/AM07/AM08/AM09, CV02, CV05, and ST04 fixture cases were adopted for `LINT_AM_001`/`LINT_AM_005`/`LINT_AM_004`/`LINT_AM_006`/`LINT_AM_007`/`LINT_AM_008`/`LINT_AM_009`/`LINT_AM_002`/`LINT_CV_001`/`LINT_CV_003`/`LINT_ST_003`, and additional rule-level coverage is still being expanded.
- [ ] Phase 2 lexical/style migrations are pending (`CP_*`, `LT_*`, `JJ_001`, remaining `CV_*` style rules).

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
