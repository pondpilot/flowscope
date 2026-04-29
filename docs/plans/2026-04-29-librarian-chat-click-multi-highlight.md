# Highlight all referenced tables and columns in Data Lineage on chat click

## Overview

When the user clicks an assistant message in the Librarian chat, switch to the Data Lineage tab and highlight every table and column referenced in that message (not just the first), auto-expanding parent tables of matched columns and recentering the viewport. Currently the click navigates to the Schema tab and highlights only the first referenced table.

## Context

- Files involved (from the spec):
  - `app/src/features/librarian/utils/schema-identifiers.ts` — extend parser to return all references
  - `app/src/features/librarian/components/chat-messages.tsx` — pass full reference list, rename callback
  - `app/src/features/librarian/components/librarian-panel.tsx` — propagate new prop type
  - `app/src/components/Workspace.tsx` — `LibrarianPanelWithNavigation` routes to lineage with `highlightNodeIds`
  - `app/src/components/AnalysisView.tsx` — consume `highlightNodeIds`, expand parent tables, recenter
  - `app/src/lib/navigation-context.tsx` — extend `NavigationTarget` type with `highlightNodeIds` and `tablesToExpand`
  - `app/src/features/librarian/__tests__/schema-identifiers.test.ts` — cover new resolver
  - `app/src/features/librarian/__tests__/components.test.tsx` — update click-navigation assertions
- Related patterns to follow:
  - `detectIdentifiers()` tokenization already in `schema-identifiers.ts`
  - `useNavigation()` target consumption pattern in `AnalysisView` (existing useEffect with `clearNavigationTarget()`)
  - `useLineage()` actions: `selectNode`, `setSearchTerm`, `toggleTableExpansion` are public API; `expandedTableIds` is in state
- Constraint: Do NOT modify `packages/react` or `packages/core`. Use existing public store actions only.
- Multi-highlight feasibility (per spec): there is no public action to set an arbitrary highlight set. `setSearchTerm` uses substring `includes()` and cannot precisely express an arbitrary node-id set, especially for qualified columns or unrelated tables. The spec authorizes a documented fallback: select first reference + Prev/Next cycling through the rest. Plan implements the parsing/resolution layer fully (so all references are known), and uses the cleanest mechanism available at the wiring step.

## Development Approach

- **Testing approach**: regular (code first, then tests) for resolver/wiring; update existing tests in lockstep
- Complete each task fully before moving to the next
- **CRITICAL: every task MUST include new/updated tests**
- **CRITICAL: all tests must pass before starting next task**
- Run `yarn typecheck` and the librarian + AnalysisView vitest suites between tasks

## Implementation Steps

### Task 1: Extend reference parser to return all references

**Files:**
- Modify: `app/src/features/librarian/utils/schema-identifiers.ts`
- Modify: `app/src/features/librarian/__tests__/schema-identifiers.test.ts`

- [x] Add `ChatReference` interface `{ tableName?: string; columnName?: string; bareColumn?: boolean }`
- [x] Add `resolveAllReferences(text, schema): ChatReference[]` — tokenizes via `detectIdentifiers`, classifies each identifier as table / qualified column (when preceded by `<table>.` or `<table> `) / bare column; deduplicates by `(tableName, columnName)` preserving first-occurrence order
- [x] Keep `resolveFirstTableReference` exported for any non-Librarian callers (verify none remain; remove if unused)
- [x] Add tests: `BKPF.MANDT` → one qualified ref; `BKPF` → one table ref; `MANDT` (alone) → one bareColumn ref; mixed message → all refs in order, deduplicated; unknown identifier → skipped silently
- [x] Run `npx vitest run src/features/librarian/__tests__/schema-identifiers.test.ts` — must pass

### Task 2: Add lineage node-id resolver

**Files:**
- Create: `app/src/lib/lineage-node-resolver.ts`
- Create: `app/src/lib/__tests__/lineage-node-resolver.test.ts`

- [x] Implement `resolveLineageNodeIds(result: AnalyzeResult | null, refs: ChatReference[]): { nodeIds: string[]; tablesToExpand: string[] }`
- [x] Walk `result.globalLineage.nodes`; case-insensitive match against `label` and `qualifiedName`
- [x] `{ tableName }` → table-like node (`type: 'table' | 'view' | 'cte'`) matching `label` or `qualifiedName`
- [x] `{ tableName, columnName }` → column node where `qualifiedName` equals `${tableName}.${columnName}` (case-insensitive); fallback match by (parent table label, column label)
- [x] `{ columnName, bareColumn }` → every column node whose `label` equals `columnName`; collect parent-table IDs into `tablesToExpand`
- [x] Skip refs with zero matches; deduplicate node IDs and `tablesToExpand`
- [x] Add tests covering: bare column with multiple matching tables; qualified column → exactly one match; unknown identifier → empty result; case-insensitive matching; deduplication
- [x] Run vitest on the new file — must pass

### Task 3: Extend NavigationTarget type

**Files:**
- Modify: `app/src/lib/navigation-context.tsx`

- [x] Add `highlightNodeIds?: string[]` and `tablesToExpand?: string[]` to `NavigationTarget` interface
- [x] Leave existing `tableId` / `tableName` fields intact (back-compat for `HierarchyView` and other callers)
- [x] No new tests required (pure type/state plumbing); existing context tests must still pass — run them

### Task 4: Wire chat click → lineage tab with highlight set

**Files:**
- Modify: `app/src/components/Workspace.tsx`
- Modify: `app/src/features/librarian/components/librarian-panel.tsx`
- Modify: `app/src/features/librarian/components/chat-messages.tsx`
- Modify: `app/src/features/librarian/__tests__/components.test.tsx`

- [x] Rename `onNavigateToTable` to `onNavigateToReferences(refs: ChatReference[])` through `chat-messages` → `librarian-panel` → `Workspace`
- [x] In `chat-messages.tsx`, replace `resolveFirstTableReference` with `resolveAllReferences`; bubble is clickable when `refs.length > 0`; on click, pass full refs array (preserve existing text-selection-skip logic and aria-label/data attributes — adapt label to first table or "highlighted nodes")
- [x] In `Workspace.tsx` `LibrarianPanelWithNavigation`: receive refs, call `resolveLineageNodeIds` against current `AnalyzeResult` (read via `useLineage` state), then `navigateTo('lineage', { highlightNodeIds, tablesToExpand })`
- [x] If `resolveLineageNodeIds` returns zero matches, do nothing (no toast, no error) — matches spec
- [x] Update `components.test.tsx`: clicking an assistant message must trigger `onNavigateToReferences` with the parsed refs (replace the old `expect(onNavigateToTable).toHaveBeenCalledWith('MARA')` assertions)
- [x] Run `npx vitest run src/features/librarian/` — must pass

### Task 5: Consume highlightNodeIds in AnalysisView

**Files:**
- Modify: `app/src/components/AnalysisView.tsx`
- Modify: `app/src/components/__tests__/AnalysisView.test.tsx` (or create if absent)

- [ ] In the lineage useEffect that consumes `navigationTarget`: if `highlightNodeIds` is present and non-empty, (a) for each id in `tablesToExpand`, expand only if not already expanded (read `state.expandedTableIds`, then call `actions.toggleTableExpansion(id)` if missing); (b) trigger highlight + fit-to-view via the chosen mechanism (see below); (c) `clearNavigationTarget()`
- [ ] Highlight + fit mechanism: investigate at implementation time. Preferred: any direct public action in `@pondpilot/flowscope-react` that accepts a node-id set. If none, fall back per spec: `actions.selectNode(highlightNodeIds[0])` and `setLineageFocusNodeId(highlightNodeIds[0])`; document the limitation as a TODO comment and in the PR description. Do not modify `packages/react`.
- [ ] Remove the schema-tab branch in the second navigation useEffect that reacted to `navigationTarget.tableName` originating from Librarian. Keep `schemaState.setSelectedTableName` usage elsewhere (SchemaSearchControl) intact — it does not flow through `navigationTarget`. Verify `HierarchyView` still routes to schema tab via `tableName` (it does, per Grep — leave that working).
- [ ] When the user clicks empty graph background, the existing GraphView pane-click already calls `selectNode(null)`; verify highlight clears in this flow (no code change expected, but add a manual check).
- [ ] Tests: assert that consuming a `navigationTarget` with `highlightNodeIds` expands missing parent tables (mock `toggleTableExpansion`) and selects the first node (mock `selectNode`); assert that schema-tab branch no longer fires for librarian-originated navigation
- [ ] Run `npx vitest run src/components/` — must pass

### Task 6: Verify acceptance criteria

- [ ] Manual: load SAP test SQL from `app/src/features/librarian/TEST_CASES.md`; ask "How are BKPF and BSEG linked?"; click answer → Lineage tab opens, BKPF + BSEG expand, all referenced columns highlighted, viewport fits the highlight set (or first node if multi-highlight fell back)
- [ ] Manual: ask "Where is MANDT stored?"; click answer → every table containing MANDT expanded; every MANDT column highlighted (or, if fallback, first MANDT highlighted with a clear indicator)
- [ ] Manual: click another answer → highlight set replaced
- [ ] Manual: click empty graph background → highlights cleared
- [ ] Manual: open Schema tab → no Librarian-driven highlight present; schema search bar still works independently
- [ ] Run `yarn typecheck` — 0 errors
- [ ] Run `yarn test` — all suites pass
- [ ] Run `yarn lint` — 0 errors
- [ ] Verify librarian feature test coverage stays at or above existing baseline

### Task 7: Update documentation

- [ ] Update `docs/librarian.md` to reflect: clicking a chat answer opens Lineage (not Schema) and highlights all referenced identifiers
- [ ] Update `CHANGELOG.md` with a one-line entry under unreleased
- [ ] If multi-highlight fell back to single-select + cycling, note the limitation in `pr-description.md` (parent dir) and in the librarian section of `CLAUDE.md` "Known Limitations"
- [ ] Move `docs/plans/2026-04-29-librarian-chat-click-multi-highlight.md` to `docs/plans/completed/`
