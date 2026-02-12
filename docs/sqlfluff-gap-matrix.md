# SQLFluff vs FlowScope Lint Gap Matrix

_Generated on 2026-02-12 from a local SQLFluff source snapshot (2026-01-20)._

## Summary

- SQLFluff lint rules indexed (excluding docs anchor `rule-index`): **72**
- FlowScope mapped rules: **72**
- Not implemented in FlowScope: **0**
- Implemented (close): **6**
- Implemented (partial): **63**
- Implemented (divergent semantics): **3**
- FlowScope fix coverage: **52 / 72**
- FlowScope rules without fix support: **20**

Bundle counts (SQLFluff): Aliasing=9, Ambiguous=9, Capitalisation=5, Convention=12, Jinja=1, Layout=15, References=6, Structure=12, TSQL=3

Status definitions:
- **Implemented (close)**: behavior is materially equivalent for common SQL patterns.
- **Implemented (partial)**: overlaps one SQLFluff rule but with narrower scope or missing configuration options.
- **Implemented (divergent semantics)**: similar category, but detects/enforces a different pattern.

FlowScope source-of-truth used for mapping:
- Rule registry: `crates/flowscope-core/src/linter/rules/mod.rs`
- Rule implementations: `crates/flowscope-core/src/linter/rules/*.rs`
- SQLFluff parity heuristics: `crates/flowscope-core/src/linter/rules/parity.rs`
- Auto-fix support (`flowscope --lint --fix`): `crates/flowscope-cli/src/fix.rs`

## Matrix

| Bundle | SQLFluff Rule | Code | Core | SQLFluff Fix | FlowScope Status | FlowScope Rule | FlowScope Fix | Notes |
|---|---|---|---|---|---|---|---|---|
| Aliasing | `aliasing.table` | `AL01` | No | Yes | Implemented (partial) | `LINT_AL_003` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Aliasing | `aliasing.column` | `AL02` | Yes | No | Implemented (partial) | `LINT_AL_004` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Aliasing | `aliasing.expression` | `AL03` | Yes | No | Implemented (partial) | `LINT_AL_001` | No | Covers implicit expression aliasing; no allow_scalar configuration. |
| Aliasing | `aliasing.unique.table` | `AL04` | Yes | No | Implemented (partial) | `LINT_AL_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Aliasing | `aliasing.unused` | `AL05` | Yes | Yes | Implemented (partial) | `LINT_AL_002` | No | Checks unused aliases mainly in multi-table/JOIN queries. |
| Aliasing | `aliasing.length` | `AL06` | Yes | No | Implemented (partial) | `LINT_AL_006` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Aliasing | `aliasing.forbid` | `AL07` | No | Yes | Implemented (partial) | `LINT_AL_007` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Aliasing | `aliasing.unique.column` | `AL08` | Yes | No | Implemented (partial) | `LINT_AL_008` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Aliasing | `aliasing.self_alias.column` | `AL09` | Yes | Yes | Implemented (partial) | `LINT_AL_009` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Ambiguous | `ambiguous.distinct` | `AM01` | Yes | No | Implemented (close) | `LINT_AM_003` | Yes | Detects DISTINCT + GROUP BY redundancy. |
| Ambiguous | `ambiguous.union` | `AM02` | Yes | Yes | Implemented (partial) | `LINT_AM_001` | No | Warns on bare UNION, recommending UNION ALL rather than explicit DISTINCT/ALL policy. |
| Ambiguous | `ambiguous.order_by` | `AM03` | No | Yes | Implemented (partial) | `LINT_AM_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Ambiguous | `ambiguous.column_count` | `AM04` | No | No | Implemented (partial) | `LINT_AM_004` | No | Detects set-operation column-count mismatches when both branch projection widths are statically known (wildcards skip check). |
| Ambiguous | `ambiguous.join` | `AM05` | No | Yes | Implemented (partial) | `LINT_AM_006` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Ambiguous | `ambiguous.column_references` | `AM06` | Yes | No | Implemented (partial) | `LINT_AM_007` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Ambiguous | `ambiguous.set_columns` | `AM07` | No | No | Implemented (partial) | `LINT_AM_008` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Ambiguous | `ambiguous.join_condition` | `AM08` | No | Yes | Implemented (partial) | `LINT_AM_009` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Ambiguous | `ambiguous.order_by_limit` | `AM09` | No | No | Implemented (divergent semantics) | `LINT_AM_002` | No | FlowScope checks ORDER BY without LIMIT in subqueries/CTEs; SQLFluff checks LIMIT/OFFSET without ORDER BY. |
| Capitalisation | `capitalisation.keywords` | `CP01` | Yes | Yes | Implemented (partial) | `LINT_CP_001` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Capitalisation | `capitalisation.identifiers` | `CP02` | Yes | Yes | Implemented (partial) | `LINT_CP_002` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Capitalisation | `capitalisation.functions` | `CP03` | Yes | Yes | Implemented (partial) | `LINT_CP_003` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Capitalisation | `capitalisation.literals` | `CP04` | Yes | Yes | Implemented (partial) | `LINT_CP_004` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Capitalisation | `capitalisation.types` | `CP05` | Yes | Yes | Implemented (partial) | `LINT_CP_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.not_equal` | `CV01` | No | Yes | Implemented (partial) | `LINT_CV_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.coalesce` | `CV02` | No | Yes | Implemented (divergent semantics) | `LINT_CV_001` | Yes | FlowScope rewrites CASE WHEN x IS NULL THEN y ELSE x; SQLFluff CV02 focuses on IFNULL/NVL -> COALESCE. |
| Convention | `convention.select_trailing_comma` | `CV03` | Yes | Yes | Implemented (partial) | `LINT_CV_006` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.count_rows` | `CV04` | Yes | Yes | Implemented (partial) | `LINT_CV_002` | Yes | Prefers COUNT(*); unlike SQLFluff, no preference config for COUNT(1)/COUNT(0). |
| Convention | `convention.is_null` | `CV05` | Yes | Yes | Implemented (close) | `LINT_CV_003` | No | Flags `= NULL` / `<> NULL` comparisons and recommends `IS [NOT] NULL`. |
| Convention | `convention.terminator` | `CV06` | No | Yes | Implemented (partial) | `LINT_CV_007` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.statement_brackets` | `CV07` | No | Yes | Implemented (partial) | `LINT_CV_008` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.left_join` | `CV08` | No | No | Implemented (close) | `LINT_CV_004` | No | Flags RIGHT JOIN variants and recommends LEFT JOIN style. |
| Convention | `convention.blocked_words` | `CV09` | No | No | Implemented (partial) | `LINT_CV_009` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.quoted_literals` | `CV10` | No | Yes | Implemented (partial) | `LINT_CV_010` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.casting_style` | `CV11` | No | Yes | Implemented (partial) | `LINT_CV_011` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Convention | `convention.join_condition` | `CV12` | No | Yes | Implemented (partial) | `LINT_CV_012` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Jinja | `jinja.padding` | `JJ01` | Yes | Yes | Implemented (partial) | `LINT_JJ_001` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.spacing` | `LT01` | Yes | Yes | Implemented (partial) | `LINT_LT_001` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.indent` | `LT02` | Yes | Yes | Implemented (partial) | `LINT_LT_002` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.operators` | `LT03` | No | Yes | Implemented (partial) | `LINT_LT_003` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.commas` | `LT04` | No | Yes | Implemented (partial) | `LINT_LT_004` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.long_lines` | `LT05` | Yes | Yes | Implemented (partial) | `LINT_LT_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.functions` | `LT06` | Yes | Yes | Implemented (partial) | `LINT_LT_006` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.cte_bracket` | `LT07` | Yes | Yes | Implemented (partial) | `LINT_LT_007` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.cte_newline` | `LT08` | Yes | Yes | Implemented (partial) | `LINT_LT_008` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.select_targets` | `LT09` | No | Yes | Implemented (partial) | `LINT_LT_009` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.select_modifiers` | `LT10` | Yes | Yes | Implemented (partial) | `LINT_LT_010` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.set_operators` | `LT11` | Yes | Yes | Implemented (partial) | `LINT_LT_011` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.end_of_file` | `LT12` | Yes | Yes | Implemented (partial) | `LINT_LT_012` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.start_of_file` | `LT13` | No | Yes | Implemented (partial) | `LINT_LT_013` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.keyword_newline` | `LT14` | No | Yes | Implemented (partial) | `LINT_LT_014` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Layout | `layout.newlines` | `LT15` | No | Yes | Implemented (partial) | `LINT_LT_015` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| References | `references.from` | `RF01` | Yes | No | Implemented (partial) | `LINT_RF_001` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| References | `references.qualification` | `RF02` | No | No | Implemented (partial) | `LINT_RF_002` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| References | `references.consistent` | `RF03` | No | Yes | Implemented (partial) | `LINT_RF_003` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| References | `references.keywords` | `RF04` | No | No | Implemented (partial) | `LINT_RF_004` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| References | `references.special_chars` | `RF05` | No | No | Implemented (partial) | `LINT_RF_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| References | `references.quoting` | `RF06` | No | Yes | Implemented (partial) | `LINT_RF_006` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.else_null` | `ST01` | No | Yes | Implemented (close) | `LINT_ST_002` | Yes | Equivalent redundant ELSE NULL detection/removal. |
| Structure | `structure.simple_case` | `ST02` | No | Yes | Implemented (partial) | `LINT_ST_005` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.unused_cte` | `ST03` | Yes | No | Implemented (close) | `LINT_ST_001` | No | Equivalent unused CTE detection. |
| Structure | `structure.nested_case` | `ST04` | No | Yes | Implemented (divergent semantics) | `LINT_ST_003` | No | FlowScope flags deep CASE nesting depth (>3); SQLFluff ST04 targets flattenable ELSE CASE patterns. |
| Structure | `structure.subquery` | `ST05` | No | Yes | Implemented (partial) | `LINT_ST_006` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.column_order` | `ST06` | No | Yes | Implemented (partial) | `LINT_ST_007` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.using` | `ST07` | No | Yes | Implemented (close) | `LINT_ST_004` | No | Flags `JOIN ... USING (...)` and recommends explicit `ON` conditions. |
| Structure | `structure.distinct` | `ST08` | Yes | Yes | Implemented (partial) | `LINT_ST_008` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.join_condition_order` | `ST09` | No | Yes | Implemented (partial) | `LINT_ST_009` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.constant_expression` | `ST10` | No | No | Implemented (partial) | `LINT_ST_010` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.unused_join` | `ST11` | No | No | Implemented (partial) | `LINT_ST_011` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| Structure | `structure.consecutive_semicolons` | `ST12` | No | Yes | Implemented (partial) | `LINT_ST_012` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| TSQL | `tsql.sp_prefix` | `TQ01` | No | No | Implemented (partial) | `LINT_TQ_001` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| TSQL | `tsql.procedure_begin_end` | `TQ02` | No | Yes | Implemented (partial) | `LINT_TQ_002` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |
| TSQL | `tsql.empty_batch` | `TQ03` | No | Yes | Implemented (partial) | `LINT_TQ_003` | No | Heuristic parity rule implemented in `crates/flowscope-core/src/linter/rules/parity.rs`; semantics are narrower than SQLFluff. |

Notes:
- `FlowScope Fix` indicates deterministic support in CLI `--fix` mode, not generic auto-fix in core linting APIs.
- `FlowScope Fix = No` marks a tracked fix gap for that SQLFluff rule mapping.
