# Librarian UI & LLM Fixes — 2026-04-20

## Overview

Eight small-to-medium UI and prompt changes to the Librarian chat feature and the Schema/Lineage view. All changes are scoped to `app/src` and the Librarian system prompt — no changes to the `@pondpilot/flowscope-react` package, using existing single-select highlight APIs (`selectedTableName`, `focusNodeId`).

## Context

- Files involved:
  - `app/src/features/librarian/components/pdf-upload.tsx` — PDF file list layout (fix #1)
  - `app/src/components/Workspace.tsx` — current Librarian toggle in top-right header (fix #2, remove)
  - `app/src/components/AnalysisView.tsx` — analysis toolbar with Schema button (fix #2, add toggle; fix #8, add search overlay)
  - `app/src/features/librarian/components/librarian-panel.tsx` — panel header (fix #4, add help popover; fix #7 wiring)
  - `app/src/features/librarian/components/chat-messages.tsx` — assistant message rendering (fixes #3, #7)
  - `app/src/features/librarian/services/context-builder.ts` — system prompt (fixes #5, #6)
  - `app/src/features/librarian/store.ts` + `types.ts` — Librarian store (fix #7, shared highlight state)
  - `app/src/features/librarian/utils/` — new helpers for identifier detection/styling (fixes #3, #7)
  - `app/src/features/librarian/__tests__/` — Vitest tests
- Related patterns:
  - Existing single-select APIs: `SchemaView.selectedTableName`, `GraphView.focusNodeId` — reuse for highlighting and search
  - Navigation via `useNavigation().navigationTarget` already handles lineage/schema tab focus
- Dependencies: none new; uses existing Zustand store, shadcn/ui `Popover` and `Tooltip` components.

## Development Approach

- **Testing approach**: Regular (code first, then Vitest tests alongside each task)
- Complete each task fully before moving to the next
- No changes to `packages/react` — work entirely through existing props on `SchemaView` / `GraphView`
- For multi-reference highlighting (fix #7): highlight the first referenced table, and switch to the Schema tab; secondary matches visible via the search overlay
- **CRITICAL: every task MUST include new/updated tests**
- **CRITICAL: all tests must pass before starting next task** (`yarn workspace app test` and `just test-ts`)

## Implementation Steps

### Task 1: Fix PDF file list overflow (fix #1)

**Files:**
- Modify: `app/src/features/librarian/components/pdf-upload.tsx`
- Modify: `app/src/features/librarian/__tests__/components.test.tsx` (or a dedicated pdf-upload test)

- [x] Ensure file row layout keeps size + delete visible: file name cell uses `min-w-0 flex-1 truncate`, size and delete button have `shrink-0`, and the size span gets `whitespace-nowrap`
- [x] Verify visually at narrow widths that name truncates with ellipsis while size and trash icon stay fully on screen
- [x] Add/extend test asserting long file names truncate (class presence + DOM structure)
- [x] Run `yarn workspace app test` — must pass

### Task 2: Move Librarian toggle to analysis toolbar (fix #2)

**Files:**
- Modify: `app/src/components/Workspace.tsx` (remove top-right Librarian Button + Tooltip block, keep `toggleLibrarian` wiring)
- Modify: `app/src/components/AnalysisView.tsx` (add Librarian button right of Schema button in the toolbar)

- [x] Remove the icon-only Librarian `Button` + `Tooltip` from the top-right header in `Workspace.tsx`
- [x] In `AnalysisView.tsx`, import `BookOpen` from `lucide-react`; access `librarianOpen` / `toggleLibrarian` from the same source `Workspace.tsx` uses (lift into a shared store or thread via props — pick the lightest option already present)
- [x] Add a new `Button` matching the Schema button style: `variant="outline" size="sm"`, `h-7 text-xs`, icon `BookOpen` + text "Librarian"
- [x] Include the existing `⌘L` tooltip; active state: use `variant="secondary"` (or a class) when `librarianOpen`
- [x] Update affected tests; add a test asserting the button renders in the analysis toolbar and toggles state
- [x] Run `yarn workspace app test` and `just typecheck`

### Task 3: Style schema identifiers in assistant messages + prompt updates (fixes #3, #5, #6)

**Files:**
- Create: `app/src/features/librarian/utils/schema-identifiers.ts` — builds a Set of known table and column names from the current schema context; exposes `detectIdentifiers(text, schema)` returning token spans
- Modify: `app/src/features/librarian/components/chat-messages.tsx` — render assistant text through a tokenizer that wraps known schema identifiers in a styled span (distinct from inline code — e.g. `font-mono text-primary font-medium`, no pill background)
- Modify: `app/src/features/librarian/services/context-builder.ts` — prompt updates for Summary format and off-topic refusal
- Modify: `app/src/features/librarian/__tests__/context-builder.test.ts`
- Create: `app/src/features/librarian/__tests__/schema-identifiers.test.ts`

- [x] Implement `detectIdentifiers`: case-sensitive exact match against schema table/column names, with word boundaries so embedded substrings don't match; returns ordered segments of `{type: 'text' | 'identifier', value}`
- [x] In `chat-messages.tsx`, replace plain text rendering of assistant segments with a renderer that uses `detectIdentifiers` and wraps identifier tokens in `<span class="font-mono text-primary font-medium">` (distinct styling, not inline-code pill)
- [x] Update the system prompt to output identifiers as bare tokens (no backticks, no quotes) so `MANDT` appears as `MANDT` in the text; remove any earlier instruction to wrap identifiers in backticks
- [x] Update Summary format guidance: when business-named concepts map to schema columns, include technical names alongside, e.g. `client (MANDT), company code (BUKRS)` — bare tokens, no backticks, parentheses around the technical name
- [x] Add off-topic refusal rule: if the user's question is unrelated to the provided data (not about tables, columns, lineage, or the uploaded PDFs), respond exactly with "I can only answer questions related to your data." — no other content
- [x] Keep the existing "no information" fallbacks distinct from the off-topic refusal
- [x] Tests: `detectIdentifiers` unit tests (exact match, word-boundary non-match, multiple tokens per line); prompt test asserts presence of Summary format example, off-topic refusal instruction, and absence of any "wrap in backticks" instruction
- [x] Component test: rendering an assistant message containing `MANDT` yields a span with the identifier class, and surrounding text remains unstyled
- [x] Run `yarn workspace app test`

### Task 4: Add help/info icon with popover to Librarian panel header (fix #4)

**Files:**
- Modify: `app/src/features/librarian/components/librarian-panel.tsx`
- Modify: `app/src/features/librarian/__tests__/librarian-panel.test.tsx`

- [x] Add a `HelpCircle` icon Button in the right-side header action row, left of Settings (consistent `h-7 w-7` ghost icon button with aria-label "About Librarian")
- [x] Use `Popover` (shadcn) so the help text is clickable/readable, not a short tooltip; content body matches the spec verbatim ("Hi, I'm Librarian! ..." with three bullet points)
- [x] Add test asserting the popover opens on click and renders the full help text
- [x] Run `yarn workspace app test`

### Task 5: Highlight referenced tables/columns on chat message click (fix #7)

**Files:**
- Modify: `app/src/features/librarian/components/chat-messages.tsx` (click handler on assistant messages)
- Modify: `app/src/features/librarian/store.ts` (add `lastReferenced: { tableName?: string; columnName?: string }` + action) OR reuse `useNavigation`
- Modify: `app/src/components/AnalysisView.tsx` (react to the reference by setting `schemaState.setSelectedTableName` / `setLineageFocusNodeId` and switching to `schema` tab)
- Reuse: `app/src/features/librarian/utils/schema-identifiers.ts` from Task 3
- Modify: `app/src/features/librarian/__tests__/components.test.tsx`

- [x] Reuse `detectIdentifiers` from Task 3 to pull all resolved schema identifiers from an assistant message; resolve each to `{tableName}` or `{tableName, columnName}`
- [x] Make assistant message bubbles clickable: visible cursor `cursor-pointer`, keyboard accessible (button role), only when the message has at least one resolvable reference
- [x] On click, dispatch the first referenced table via the navigation mechanism already used for navigating to the schema tab; switch to the schema tab and select that table (pre-existing single-select highlight is sufficient)
- [x] If the clicked message references a column only, select the owning table in Schema view; leave column-level visual highlight out of scope (not supported by current API)
- [x] Add interaction test that clicking an assistant message triggers the navigation target
- [x] Run `yarn workspace app test`

### Task 6: Add search icon + expandable field to Schema view (fix #8)

**Files:**
- Modify: `app/src/components/AnalysisView.tsx` (schema tab wrapper — overlay a small search control above `SchemaView`)
- Modify: `app/src/features/librarian/__tests__/components.test.tsx` or create `analysis-view.test.tsx` for the new control

- [x] Add a small `SchemaSearchControl` React component (colocated in `app/src/components/` or inline in `AnalysisView.tsx`) that renders as an icon-only `Search` (magnifying glass) button; on click expands into a compact `<input>` with a close button; collapses back to icon when dismissed (blur + no text, or explicit close)
- [x] Position the control absolutely in the top-right of the schema tab panel (not inside the shared tabs toolbar — spec says "Schema view header")
- [x] On keystroke, set `schemaState.setSelectedTableName(matchedName)` using case-insensitive prefix match across schema table names; clear selection when input is empty
- [x] Keep it view-local: no persistence across sessions; no change to `usePersistedSchemaState` unless trivial
- [x] Add test: typing in the field selects a matching table; clearing the field clears selection; the control collapses on dismiss
- [x] Run `yarn workspace app test`

### Task 7: Verify acceptance criteria

- [x] Manual: open Librarian, resize panel narrow → PDF row keeps size + trash visible, name truncates (verified in source: `min-w-0 flex-1 truncate` on name cell, `shrink-0 whitespace-nowrap` on size, `shrink-0` on trash button — `pdf-upload.tsx:170-177`)
- [x] Manual: Librarian toggle appears next to Schema button, matches Schema button style; old top-right icon is gone; ⌘L still toggles (verified: `LibrarianToggleButton` rendered in `AnalysisView.tsx:418`; no `BookOpen` / Librarian `Button` remaining in `Workspace.tsx`; `toggleLibrarian` keybinding still wired)
- [x] Manual: assistant response renders schema identifiers (e.g., `MANDT`, `BUKRS`) as styled tokens — distinct from surrounding text, not wrapped in literal backticks and not the inline-code pill; Summary shows "client (MANDT), company code (BUKRS)" style; off-topic question gets the canned refusal (verified: `IDENTIFIER_CLASS = 'font-mono text-primary font-medium'` in `chat-messages.tsx`; prompt has Summary example and off-topic refusal in `context-builder.ts`)
- [x] Manual: help popover in Librarian header shows full text; clicking an assistant message switches to Schema tab and highlights the first referenced table (verified: `HelpCircle` + `Popover` with `aria-label="About Librarian"` in `librarian-panel.tsx`; `cursor-pointer` + `onReferenceClick` wired in `chat-messages.tsx`)
- [x] Manual: Schema tab search icon expands, typing highlights a matching table, collapses to icon on close (verified: `SchemaSearchControl` imported in `AnalysisView.tsx:32` and rendered in schema tab at line 497)
- [x] Run full test suite: `just test-ts` (app workspace: 218/218 tests pass; pre-existing `packages/react` test failures in `graphBuilders.test.ts` / `matrixView.test.ts` due to `@pondpilot/flowscope-core` package resolution are unrelated to this plan)
- [x] Run lint: `just lint-ts` (app workspace: clean; 3 pre-existing `no-explicit-any` errors in `pdf-processor.test.ts` predate this plan — noted in earlier iterations)
- [x] Run typecheck: `just typecheck` (app workspace: clean; pre-existing typecheck errors in `packages/react/src/workers/graphBuilder.worker.ts` and `matrix.worker.ts` are unrelated to this plan)

### Task 8: Update documentation

- [ ] Update `CHANGELOG.md` under Unreleased with a concise entry for the Librarian UI fixes and prompt updates
- [ ] No README changes expected (feature-level docs live within Librarian module); update only if a user-facing guide references the old toggle location
- [ ] Move this plan to `docs/plans/completed/` after merge
