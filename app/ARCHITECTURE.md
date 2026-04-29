# FlowScope App Architecture

## Overview

The FlowScope app is a production-ready web application for SQL lineage visualization. It follows professional software engineering practices with clean separation of concerns, proper state management using React Context and Hooks, and a component-driven architecture.

## Directory Structure

```
app/src/
├── components/          # React components
│   ├── ui/             # Reusable UI primitives (Radix + Tailwind)
│   ├── debug/          # Debugging tools
│   ├── AnalysisView.tsx    # Lineage visualization & Schema tabs
│   ├── EditorArea.tsx      # SQL editor container
│   ├── EditorToolbar.tsx   # Editor controls (Run, Dialect)
│   ├── FileSelector.tsx    # File dropdown/management
│   ├── ProjectSelector.tsx # Project management dropdown
│   ├── SchemaEditor.tsx    # DDL schema editor
│   ├── ShareDialog.tsx     # Project export/sharing
│   └── Workspace.tsx       # Main two-panel layout
├── features/           # Feature modules (self-contained)
│   └── librarian/         # AI chat panel (Q&A over lineage + PDFs)
├── hooks/              # Custom React hooks
│   ├── useAnalysis.ts      # SQL analysis workflow
│   ├── useFileNavigation.ts # Graph-to-editor navigation
│   ├── useGlobalShortcuts.ts # Keyboard shortcut system
│   ├── useShareImport.ts   # Import/Export logic
│   └── useAnalysisWorkerInit.ts  # Analysis worker initialization
├── lib/                # Core utilities and state
│   ├── constants.ts        # App-wide configuration
│   ├── project-store.tsx   # Global Project State (Context)
│   ├── schema-parser.ts    # Client-side DDL parsing
│   └── share.ts            # Sharing format types
├── types/              # TypeScript type definitions
│   └── index.ts
├── App.tsx             # Root provider composition
└── main.tsx            # Entry point
```

## Key Design Patterns

### 1. Separation of Concerns

*   **Workspace**: Handles the high-level layout (Header + Split Panes) and global dialogs (Share).
*   **EditorArea**: Manages the "Input" side (SQL text, File selection, Dialect).
*   **AnalysisView**: Manages the "Output" side (Lineage Graph, Schema Visualization, Issues).
*   **Store**: `ProjectStore` manages the persistent state (Projects, Files, Settings).

### 2. State Management

#### Project State (`lib/project-store.tsx`)
Uses React Context to manage:
*   Project CRUD (Create/Read/Update/Delete)
*   File System (Virtual file management)
*   Active File/Project selection
*   Schema SQL definitions
*   Persistence via `localStorage`

#### Lineage State (`@pondpilot/flowscope-react`)
The lineage visualization library manages its own transient state (graph layout, selection, zoom level) via an internal store (Zustand), exposed via `useLineage`.

### 3. Custom Hooks

*   `useAnalysis`: Orchestrates the analysis flow. It bridges the `ProjectStore` (source data) and the WASM Engine (processor) to produce results.
*   `useFileNavigation`: Handles the interaction where clicking a table in the graph navigates the editor to the defining SQL file.
*   `useGlobalShortcuts`: Centralized keyboard shortcut registry.

### 4. Schema Awareness

The app now supports "Schema-Aware" analysis.
*   **SchemaEditor**: Allows users to define a schema using standard `CREATE TABLE` DDL.
*   **schema-parser.ts**: Parses this DDL client-side to generate metadata.
*   **Integration**: This metadata is passed to the WASM engine, enabling:
    *   Wildcard expansion (`SELECT *`)
    *   Column validation
    *   Precise column lineage

### 5. Feature Modules

Self-contained feature folders under `app/src/features/` own all their code (components, hooks, services, workers, tests) and expose a public API via `index.ts`.

#### Librarian (`features/librarian/`)

AI-powered chat panel for SQL lineage Q&A.

- `components/` — panel, chat messages, input, PDF upload, AI settings dialog
- `services/` — AI client (OpenAI / Anthropic / custom), context builder, lineage formatter, PDF processor, vector search, embedding service
- `workers/` — embedding Web Worker (local Xenova/transformers model)
- `hooks/use-librarian-chat.ts` — chat orchestrator
- `hooks/use-sync-active-project.ts` — mirrors `activeProjectId` from `useProject()` into the Librarian store and prunes buckets for deleted projects
- `store.ts` — Zustand store. Per-project buckets (`byProject` keyed by `activeProjectId`) hold messages, PDF files, and embedded chunks; `isLoading` and `hasConfig` are global. Selector hooks `useLibrarianMessages` / `useLibrarianPdfFiles` / `useLibrarianPdfChunks` return the active project's slice. `addMessageToProject(projectId, ...)` writes to an explicit bucket so an in-flight LLM response is routed back to the originating project even if the user switches mid-flight.

State is Zustand (not React Context), UI is Radix + Tailwind. All AI calls hit the user's configured provider directly from the browser. See `docs/librarian.md` for the user guide.

## Data Flow

### Analysis Loop

1.  **Trigger**: User clicks "Run" or presses `Cmd+Enter`.
2.  **Collection**: `useAnalysis` gathers the SQL from the active file (or all files, depending on mode) and the defined Schema DDL.
3.  **Validation**: Basic limits (size/count) are checked.
4.  **Processing**:
    *   Schema DDL is parsed into metadata.
    *   SQL and Metadata are sent to `analyze_sql_json` (WASM).
5.  **Result**: The JSON result is dispatched to the Lineage Store.
6.  **Rendering**: `AnalysisView` updates the Graph and Issues panel.

### Librarian Chat Flow

1.  User types a question in the Librarian panel.
2.  `use-librarian-chat.ts` gathers: current lineage (from `useLineageState`), active SQL file content, last 10 chat messages, and vector-search results over uploaded PDF chunks.
3.  `context-builder.ts` assembles a structured prompt with labeled data sources (Data Lineage / SQL Code / Documentation / Conversation History).
4.  `ai-service.ts` sends the prompt via `fetch()` to the configured provider (OpenAI / Anthropic / custom endpoint).
5.  Response is stored in the chat and rendered with markdown + identifier highlighting.

PDF processing runs asynchronously: text extraction (pdfjs-dist) → 500-char chunking → embeddings (local `multilingual-e5-small` model in a Web Worker) → stored in the librarian store.

## UI Architecture

*   **Layout**: `react-resizable-panels` provides the split-view.
*   **Styling**: Tailwind CSS with `shadcn/ui` (Radix Primitives) pattern.
*   **Icons**: Lucide React.
*   **Editor**: `CodeMirror` (via `@pondpilot/flowscope-react`).
*   **Librarian icon**: Custom SVG (`/public/polly-icon.svg`) for the toolbar, chat avatar, and empty state.

## Configuration

*   **Limits**: 10MB file size, 100 files per project.
*   **Shortcuts**:
    *   `Cmd/Ctrl + Enter`: Run Analysis
    *   `Cmd/Ctrl + P`: Switch Project
    *   `Cmd/Ctrl + O`: Switch File
    *   `Cmd/Ctrl + D`: Switch Dialect
    *   `Cmd/Ctrl + L`: Toggle Librarian panel

## Future Improvements

*   **Real-time Analysis**: Switch from "Run" button to debit/incremental analysis.
*   **Cloud Storage**: Optional sync backend.
*   **Git Integration**: Direct loading from repositories.