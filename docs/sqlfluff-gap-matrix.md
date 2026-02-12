# SQLFluff vs FlowScope Lint Gap Matrix

_Generated on 2026-02-12 from SQLFluff stable docs: https://docs.sqlfluff.com/en/stable/reference/rules.html_

## Summary

- SQLFluff rules indexed: **72**
- FlowScope mapped rules: **10**
- Not implemented in FlowScope: **62**
- Implemented (close): **3**
- Implemented (partial): **4**
- Implemented (divergent semantics): **3**

Bundle counts (SQLFluff): Aliasing=9, Ambiguous=9, Capitalisation=5, Convention=12, Jinja=1, Layout=15, References=6, Structure=12, TSQL=3

## Matrix

| Bundle | SQLFluff Rule | Code | Core | SQLFluff Fix | FlowScope Status | FlowScope Rule | FlowScope Fix | Notes |
|---|---|---|---|---|---|---|---|---|
| Aliasing | `aliasing.table` | `AL01` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Aliasing | `aliasing.column` | `AL02` | Yes | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Aliasing | `aliasing.expression` | `AL03` | Yes | No | Implemented (partial) | `LINT_AL_001` | No | Covers implicit expression aliasing; no allow_scalar configuration. |
| Aliasing | `aliasing.unique.table` | `AL04` | Yes | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Aliasing | `aliasing.unused` | `AL05` | Yes | Yes | Implemented (partial) | `LINT_AL_002` | No | Checks unused aliases mainly in multi-table/JOIN queries. |
| Aliasing | `aliasing.length` | `AL06` | Yes | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Aliasing | `aliasing.forbid` | `AL07` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Aliasing | `aliasing.unique.column` | `AL08` | Yes | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Aliasing | `aliasing.self_alias.column` | `AL09` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.distinct` | `AM01` | Yes | No | Implemented (close) | `LINT_AM_003` | Yes | Detects DISTINCT + GROUP BY redundancy. |
| Ambiguous | `ambiguous.union` | `AM02` | Yes | Yes | Implemented (partial) | `LINT_AM_001` | No | Warns on bare UNION, recommending UNION ALL rather than explicit DISTINCT/ALL policy. |
| Ambiguous | `ambiguous.order_by` | `AM03` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.column_count` | `AM04` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.join` | `AM05` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.column_references` | `AM06` | Yes | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.set_columns` | `AM07` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.join_condition` | `AM08` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Ambiguous | `ambiguous.order_by_limit` | `AM09` | No | No | Implemented (divergent semantics) | `LINT_AM_002` | No | FlowScope checks ORDER BY without LIMIT in subqueries/CTEs; SQLFluff checks LIMIT/OFFSET without ORDER BY. |
| Capitalisation | `capitalisation.keywords` | `CP01` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Capitalisation | `capitalisation.identifiers` | `CP02` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Capitalisation | `capitalisation.functions` | `CP03` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Capitalisation | `capitalisation.literals` | `CP04` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Capitalisation | `capitalisation.types` | `CP05` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.not_equal` | `CV01` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.coalesce` | `CV02` | No | Yes | Implemented (divergent semantics) | `LINT_CV_001` | Yes | FlowScope rewrites CASE WHEN x IS NULL THEN y ELSE x; SQLFluff CV02 focuses on IFNULL/NVL -> COALESCE. |
| Convention | `convention.select_trailing_comma` | `CV03` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.count_rows` | `CV04` | Yes | Yes | Implemented (partial) | `LINT_CV_002` | Yes | Prefers COUNT(*); unlike SQLFluff, no preference config for COUNT(1)/COUNT(0). |
| Convention | `convention.is_null` | `CV05` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.terminator` | `CV06` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.statement_brackets` | `CV07` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.left_join` | `CV08` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.blocked_words` | `CV09` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.quoted_literals` | `CV10` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.casting_style` | `CV11` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Convention | `convention.join_condition` | `CV12` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Jinja | `jinja.padding` | `JJ01` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.spacing` | `LT01` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.indent` | `LT02` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.operators` | `LT03` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.commas` | `LT04` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.long_lines` | `LT05` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.functions` | `LT06` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.cte_bracket` | `LT07` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.cte_newline` | `LT08` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.select_targets` | `LT09` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.select_modifiers` | `LT10` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.set_operators` | `LT11` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.end_of_file` | `LT12` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.start_of_file` | `LT13` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.keyword_newline` | `LT14` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Layout | `layout.newlines` | `LT15` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| References | `references.from` | `RF01` | Yes | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| References | `references.qualification` | `RF02` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| References | `references.consistent` | `RF03` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| References | `references.keywords` | `RF04` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| References | `references.special_chars` | `RF05` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| References | `references.quoting` | `RF06` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.else_null` | `ST01` | No | Yes | Implemented (close) | `LINT_ST_002` | Yes | Equivalent redundant ELSE NULL detection/removal. |
| Structure | `structure.simple_case` | `ST02` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.unused_cte` | `ST03` | Yes | No | Implemented (close) | `LINT_ST_001` | No | Equivalent unused CTE detection. |
| Structure | `structure.nested_case` | `ST04` | No | Yes | Implemented (divergent semantics) | `LINT_ST_003` | No | FlowScope flags deep CASE nesting depth (>3); SQLFluff ST04 targets flattenable ELSE CASE patterns. |
| Structure | `structure.subquery` | `ST05` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.column_order` | `ST06` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.using` | `ST07` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.distinct` | `ST08` | Yes | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.join_condition_order` | `ST09` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.constant_expression` | `ST10` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.unused_join` | `ST11` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| Structure | `structure.consecutive_semicolons` | `ST12` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| TSQL | `tsql.sp_prefix` | `TQ01` | No | No | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| TSQL | `tsql.procedure_begin_end` | `TQ02` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
| TSQL | `tsql.empty_batch` | `TQ03` | No | Yes | Not implemented | `-` | - | No direct FlowScope lint rule equivalent. |
