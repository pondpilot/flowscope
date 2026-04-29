# Isolate Librarian state per project (RAM-only)

## Overview

Refactor the Librarian Zustand store from a single flat shape to per-project buckets keyed by FlowScope's `activeProjectId`. Chat messages, PDF files, and PDF chunks become isolated per project so switching projects no longer leaks one project's chat/docs into another's LLM context. Persistence stays RAM-only — F5 still wipes everything (current behavior preserved).

## Context

- Files involved:
  - `app/src/features/librarian/store.ts` — flat store; needs per-project bucket shape and selectors
  - `app/src/features/librarian/components/librarian-panel.tsx` — consumes `messages`, `addPdfFile`, `addPdfChunks`, `setPdfStatus`
  - `app/src/features/librarian/components/pdf-upload.tsx` — consumes `pdfFiles`, `removePdf`, `hasPdfFile`
  - `app/src/features/librarian/components/chat-input.tsx` — needs disabled state when no active project
  - `app/src/features/librarian/hooks/use-librarian-chat.ts` — reads `messages` and `pdfChunks` via `getState()`
  - `app/src/lib/project-store.tsx` — source of `activeProjectId` (already handles backend `__backend__` id)
  - `app/src/components/Workspace.tsx` — mounts `LibrarianPanel`; already uses `useProject()` so natural place for the sync hook
  - `app/src/features/librarian/__tests__/store.test.ts` — existing flat-shape tests need rewriting
  - `app/src/features/librarian/__tests__/use-librarian-chat.test.ts` — mocks `useProject`; needs `activeProjectId` in mock
  - `app/src/features/librarian/__tests__/librarian-panel.test.tsx`, `pdf-upload`-related tests in `components.test.tsx` — may need active project setup
- Related patterns:
  - Plain Zustand `create()` (no `createSelectors`) — match existing store style
  - Selector hooks colocated in `store.ts` (the spec allows a separate `selectors.ts`; keeping in `store.ts` is simpler and avoids new files)
  - Radix UI + Tailwind for the disabled-input UX
- Dependencies: none new

## Development Approach

- Testing approach: Regular (code first, then tests) — matches the existing store's test pattern
- Complete each task fully before moving to the next
- CRITICAL: every task MUST include new/updated tests
- CRITICAL: `yarn typecheck` and `npx vitest run src/features/librarian/` must pass before starting next task
- Match existing zustand/Radix/Tailwind patterns; do not introduce new abstractions
- No localStorage persistence, no migration code, no UI redesign — strictly the isolation behavior

## Implementation Steps

### Task 1: Reshape the Librarian store to per-project buckets

**Files:**
- Modify: `app/src/features/librarian/store.ts`

- [x] Add `ProjectLibrarianState` interface `{ messages, pdfFiles, pdfChunks }`
- [x] Replace flat fields with `byProject: Record<string, ProjectLibrarianState>` and `activeProjectId: string | null`; keep `isLoading` and `hasConfig` global
- [x] Add `setActiveProjectId(id)` setter
- [x] Rewrite `addMessage`, `clearMessages`, `addPdfFile`, `addPdfChunks`, `removePdf`, `setPdfStatus` to operate on `byProject[activeProjectId]`; lazily initialize the bucket on first write; no-op if `activeProjectId` is null
- [x] Rewrite `hasPdfFile` to read from the current project bucket only
- [x] Add selector hooks `useLibrarianMessages()`, `useLibrarianPdfFiles()`, `useLibrarianPdfChunks()` returning empty arrays when no active project or bucket missing
- [x] Update `app/src/features/librarian/__tests__/store.test.ts`: reset `byProject` + set `activeProjectId` in `beforeEach`; add cases — write/read isolation across two project ids, switching back preserves data, `addMessage` no-ops when active id is null, lazy bucket init, `hasPdfFile` scoped to active project
- [x] `cd flowscope_fork/app && yarn typecheck && npx vitest run src/features/librarian/__tests__/store.test.ts` — must pass

### Task 2: Sync `activeProjectId` from project store into librarian store

**Files:**
- Create: `app/src/features/librarian/hooks/use-sync-active-project.ts`
- Modify: `app/src/components/Workspace.tsx` (call the hook where `useProject()` is already in scope)

- [x] Implement `useSyncActiveProject()` that reads `activeProjectId` from `useProject()` and pushes it into the store via `setActiveProjectId` inside a `useEffect`
- [x] Invoke the hook once in `Workspace.tsx` (it already calls `useProject()`)
- [x] Add a test `app/src/features/librarian/__tests__/use-sync-active-project.test.tsx` that mounts the hook with a mocked `useProject`, asserts the store's `activeProjectId` updates when the project changes, and that switching back re-points to the original bucket
- [x] `yarn typecheck && npx vitest run src/features/librarian/` — must pass

### Task 3: Migrate consumer components to selector hooks

**Files:**
- Modify: `app/src/features/librarian/components/librarian-panel.tsx`
- Modify: `app/src/features/librarian/components/pdf-upload.tsx`
- Modify: `app/src/features/librarian/components/chat-input.tsx`

- [ ] In `librarian-panel.tsx`, replace `useLibrarianStore((s) => s.messages)` with `useLibrarianMessages()`; keep setter calls (`addPdfFile`, `addPdfChunks`, `setPdfStatus`) unchanged — they now route through active project automatically
- [ ] In `pdf-upload.tsx`, replace `useLibrarianStore((s) => s.pdfFiles)` with `useLibrarianPdfFiles()`; keep `removePdf` / `hasPdfFile` calls
- [ ] Add a "no active project" guard to `LibrarianPanel`: read `activeProjectId` from the store; when null, disable the chat input (pass an additional disabled signal) and surface an empty-state hint "Open or create a project to use Librarian" — wire via `ChatInput`'s existing `disabled` prop or a thin sibling flag (no new component)
- [ ] Update `app/src/features/librarian/__tests__/librarian-panel.test.tsx` (and `components.test.tsx` if it covers `pdf-upload`) to seed `byProject` + `activeProjectId` in `beforeEach`; add a case for the "no active project" disabled UI
- [ ] `yarn typecheck && npx vitest run src/features/librarian/` — must pass

### Task 4: Update `use-librarian-chat` to read from the active project bucket

**Files:**
- Modify: `app/src/features/librarian/hooks/use-librarian-chat.ts`
- Modify: `app/src/features/librarian/__tests__/use-librarian-chat.test.ts`

- [ ] Replace `useLibrarianStore.getState().pdfChunks` and `.messages` reads with bucket-scoped reads (`state.byProject[state.activeProjectId]?.pdfChunks ?? []`, same for messages)
- [ ] Bail out early with a friendly assistant message ("Open or create a project to use Librarian") if `activeProjectId` is null — keeps prompt builder from running on a phantom bucket
- [ ] Update the existing chat hook test mock for `useProject` to include `activeProjectId: 'proj-1'`; update the `beforeEach` `setState` to seed `byProject['proj-1']` and `activeProjectId`; add a new test: PDF chunks uploaded to project A are not searched while project B is active
- [ ] `yarn typecheck && npx vitest run src/features/librarian/__tests__/use-librarian-chat.test.ts` — must pass

### Task 5: Verify acceptance criteria

- [ ] Manual test (matches spec §7):
  1. Open project A, send a chat message, upload a PDF — both visible
  2. Switch to project B — chat empty, PDF list empty
  3. Send a different message in B, upload a different PDF
  4. Switch back to A — original chat and PDF restored
  5. Ask a question in A — only A's PDF and chat history land in the prompt (verify via network/devtools)
  6. F5 — both projects start fresh (RAM-only confirmed)
- [ ] No active project: deleting the last project (or starting with `activeProjectId === null`) shows the disabled state and does not crash
- [ ] `cd flowscope_fork/app && yarn typecheck` — 0 errors
- [ ] `cd flowscope_fork/app && npx vitest run src/features/librarian/` — all green
- [ ] `cd flowscope_fork/app && yarn lint` — clean
- [ ] Verify Librarian tests still cover ≥ 80% of the feature directory (existing baseline maintained)

### Task 6: Update documentation and finalize

- [ ] Update `flowscope_fork/CHANGELOG.md` with a "Per-project Librarian state isolation (RAM-only)" entry
- [ ] Update `flowscope_fork/docs/librarian.md` if it describes chat/PDF persistence (add a note that state is per project, RAM-only)
- [ ] Update `PondPilot Librarian/CLAUDE.md` "Current Status" working list to mention per-project isolation
- [ ] Move this plan to `flowscope_fork/docs/plans/completed/`
