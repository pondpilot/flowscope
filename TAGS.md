# Classification & Tag Propagation Plan

This document captures the proposed implementation work for adding user/robot-assigned tags (e.g., PII, PCI, PHI) and visualizing their downstream impact across FlowScope.

## Objectives
- Capture table/column-level classifications from both manual edits (UI) and external automation (“robots”).
- Propagate tags through the lineage engine so downstream tables, views, and columns inherit the correct sensitivity labels.
- Surface tag presence, counts, and warnings in every analysis surface (Graph, Matrix, Hierarchy, Schema, Issues, exports).
- Preserve and share tagging context across projects, exports, and automation hooks.

## Milestones

### 1. Data Contract & Persistence
- Extend `SchemaTable` / `ColumnSchema` (packages/core/src/types.ts) with `classifications?: ColumnTag[]` and define the `ColumnTag` interface (`name`, `confidence`, `source`, optional `notes`, `lastUpdated`).
- Add optional `tagHints?: TagHint[]` to `AnalyzeRequest` so automation can submit classifications alongside SQL.
- Include `tagCounts` and `tagFlow` helpers in `AnalyzeResult.summary` to provide aggregate statistics and quick UI lookups.
- Persist user overrides in the project store (`app/src/lib/project-store.tsx`) via a `classificationOverrides` map keyed by canonical column IDs.

### 2. Analyzer Propagation
- Seed initial tags from imported schema, overrides, and `tagHints` within the Rust analyzer (`crates/flowscope-core/src/analyzer`).
- During column lineage traversal, propagate tags along `data_flow` and `derivation` edges, recording the provenance/path on each inherited tag.
- Define masking/transform rules (e.g., hashing removes `raw` but adds `derived`) and emit warnings when sensitive tags cross statement boundaries without masking.
- Attach propagated tags to `Node.metadata`, `Edge.metadata`, and `resolvedSchema.tables[].columns` so downstream layers can render badges without recomputation.
- Expand the analyzer’s unit and snapshot tests to cover tag inheritance, override precedence, and issue generation.

### 3. FlowScope React Enhancements
- Augment the lineage store (`packages/react/src/store.ts`) with classification filter state (selected tags, severity levels).
- Update `graphBuilders`, `GraphView`, and node components to:
  - Color nodes/columns by highest-risk tag.
  - Display tag pills/hover details.
  - Filter graphs by selected tags and highlight propagation paths.
- Introduce a Tag Legend/Filter control near the existing panel buttons plus a Tag Inspector side panel (replaces deprecated ColumnPanel) that lists upstream/downstream tag provenance and allows quick overrides.
- Enhance `MatrixView`, `HierarchyView`, and search suggestions to understand tag filters (heatmaps by tag presence, subtree collapse to sensitive nodes, `pii:` keyword search).
- Add Tag metadata to exports (PNG/SVG) and tooltip legends so users can capture classifications in shared artifacts.

### 4. Studio App UX
- Build a “Tag Manager” dialog accessible from the Schema tab or header that lets users import/export classifications (CSV/JSON), run bulk actions, and edit column tags without editing raw DDL.
- Show tag badges within SchemaView rows and highlight conflicts between imported schema tags and overrides.
- Integrate robot inputs by accepting uploaded classification files or syncing with the new `tagHints` API.
- Trigger re-analysis when tags change (mirrors existing schema change auto-run) so propagation stays up to date.
- Update `SchemaAwareIssuesPanel` with new issue codes (e.g., `TAG_PROPAGATION_WARNING`, `TAG_CONFLICT`) and surface tag stats in the Analysis header.

### 5. Sharing, Persistence, QA
- Ensure project export/share payloads include classification overrides and schemas containing tags.
- Document the tagging workflow in README/docs (how to supply tags, view propagation, automate via `tagHints`).
- Add end-to-end fixtures demonstrating tag propagation and Cypress smoke tests covering tag filters, inspector interactions, and warnings.
- Stress-test large graphs to gauge performance impact; add a failsafe toggle (“disable tag visualization for huge graphs”) if necessary.

## Open Questions
- How granular should transform rules be (static map vs. user-defined policy)?
- Should we allow per-tag severity levels that impact coloring/alerts?
- What is the storage format for robot-imported tag catalogs (JSON schema, CSV template)?
- Do we need versioning/audit logs for tag changes in collaborative scenarios?

Tracking these decisions early will keep the implementation aligned with compliance expectations while remaining intuitive for analysts and developers.
