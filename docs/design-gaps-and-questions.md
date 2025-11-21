# Design Focus Areas

This document tracks the **active** design decisions we still need to make. Resolved items are folded into the other specs so this list stays short and actionable.

## Immediate Decisions (Phase 1 readiness)

1. **Logging & Observability**
   - Decide how the Rust engine surfaces debug logs in browser contexts (e.g., ring buffer returned with `AnalyzeResult`, dedicated log events, or console forwarding).
   - Define the configuration surface (`initWasm` option, per-request flag) and how logs interact with privacy expectations.

2. **Error-Recovery Semantics**
   - Document exact behavior for unresolved tables, missing columns, unsupported joins, etc.
   - For each issue code, specify whether the engine emits partial lineage, placeholder nodes, or aborts the statement.
   - Capture these rules in `core-engine-spec.md` plus golden tests.

3. **Issue Catalog & Telemetry**
   - Finalize the canonical list of issue codes/severities.
   - Decide how the TS layer reports aggregated issue stats (e.g., `summary.issueBreakdown`) and whether we emit usage telemetry hooks in dev builds.

## Phase 2+ Opportunities

1. **Graph Layout Hints**
   - Determine if the engine should emit layout groups, flow directions, or importance scores to improve ReactFlow rendering.

2. **Incremental Analysis**
   - Explore caching AST fragments / lineage subgraphs so IDE integrations can re-run analysis on edit without recomputing everything.

3. **Warning Configuration**
   - Allow hosts to suppress or downgrade specific warning codes (e.g., `APPROXIMATE_LINEAGE`) per request or globally.

4. **Export Formats**
   - Decide on official exports (OpenLineage JSON, DOT, Mermaid) and whether they live in `@pondpilot/flowscope-core` or a companion package.

5. **Lineage Diff Mode**
   - Define the contract for comparing two analysis runs and highlighting added/removed nodes or edges.

## Documentation & Developer Experience

1. **Integration Guides**
   - Author guides for React apps, browser extensions, and IDE/webview hosts covering initialization, worker usage, and schema best practices.

2. **API References**
   - Generate Rust and TypeScript API docs plus an error-code catalog; wire them into the docs site.

3. **Contributor Guide**
   - Add build instructions, test workflows, and review expectations so new contributors can ramp quickly.

## Next Steps

1. Lock down the logging/debugging plan and error-recovery matrix before moving into Phase 1 implementation.
2. Spin up an issue-code catalog doc and add regression fixtures that cover the agreed semantics.
3. Schedule documentation work (integration/API/contributor guides) alongside Phase 3 when the React viewer stabilizes.

Last Updated: 2025-11-21
