# Column Lineage Semantics (FlowScope Core)

This note captures the intended column-level lineage rules so changes are easy to validate and reason about.

- **Ownership edges**: Every physical table/CTE node owns its columns. These edges link table/CTE nodes to column nodes.
- **Data flow edges**: A data-flow edge connects a source column to a target column when the target is a direct projection (`SELECT src_col AS target`). No transformation is implied.
- **Derivation edges**: A derivation edge connects source columns to a computed target (`SELECT f(a, b) AS c`, window functions, CASE, aggregates). The `expression` is preserved on the target node and edge.
- **Wildcard expansion**: With schema metadata, `*`/`table.*` expands to concrete columns; without schema, expansion is approximate and emits `APPROXIMATE_LINEAGE`.
- **Set operations**: UNION/INTERSECT/EXCEPT emit derivation edges from each branch’s output columns to the combined output columns.
- **Aggregation**: Grouping columns flow directly; aggregated expressions are derivations. HAVING and window clauses do not create new columns, only derivations.
- **Write targets**: INSERT/CTAS/CREATE VIEW attach ownership edges for target columns and data-flow/derivation edges from source columns based on projection semantics.
- **Cross-statement**: Produced tables connect to downstream consumers via `cross_statement` edges in the global graph.

Tests should assert these behaviors with stable summaries rather than hash-based node IDs.***
