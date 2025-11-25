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
├── hooks/              # Custom React hooks
│   ├── useAnalysis.ts      # SQL analysis workflow
│   ├── useFileNavigation.ts # Graph-to-editor navigation
│   ├── useGlobalShortcuts.ts # Keyboard shortcut system
│   ├── useShareImport.ts   # Import/Export logic
│   └── useWasmInit.ts      # WASM initialization
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

## UI Architecture

*   **Layout**: `react-resizable-panels` provides the split-view.
*   **Styling**: Tailwind CSS with `shadcn/ui` (Radix Primitives) pattern.
*   **Icons**: Lucide React.
*   **Editor**: `CodeMirror` (via `@pondpilot/flowscope-react`).

## Configuration

*   **Limits**: 10MB file size, 100 files per project.
*   **Shortcuts**:
    *   `Cmd/Ctrl + Enter`: Run Analysis
    *   `Cmd/Ctrl + P`: Switch Project
    *   `Cmd/Ctrl + O`: Switch File
    *   `Cmd/Ctrl + D`: Switch Dialect

## Future Improvements

*   **Real-time Analysis**: Switch from "Run" button to debit/incremental analysis.
*   **Cloud Storage**: Optional sync backend.
*   **Git Integration**: Direct loading from repositories.