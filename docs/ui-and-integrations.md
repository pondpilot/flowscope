# UI & Integrations Spec

This document covers the **React viewer components**, the **example web app**, and the types of integrations we anticipate. It focuses on responsibilities and data flow rather than JSX details.

## 1. React Viewer Package (`@pondpilot/flowscope-react`)

### 1.1 Purpose

Provide an optional React-based UI layer that:

- Visualizes the lineage graph produced by `@pondpilot/flowscope-core`.
- Displays:
  - Table/CTE lineage.
  - Column-level lineage and expressions.
  - SQL with highlighted segments.
- Is **presentation-only**:
  - Does **not** compute lineage itself.
  - Accepts typed data as input.

### 1.2 Inputs

Core data input:

- `AnalyzeResult` (from `@pondpilot/flowscope-core`).
- Original SQL string (for highlighting).
- Optional UI configuration (colors, layouts, etc.).

### 1.3 Key Components (Conceptual)

1. **Graph View**
   - Displays:
     - Table-level graph (default).
     - Optionally a more detailed graph including column nodes (when zoomed in).
   - Features:
     - Zoom/pan.
     - Node selection.
     - Edge highlighting.
   - Inputs:
     - Lineage graph for a specific statement.
     - Optional filters (only tables, only selected table and its columns, etc.).

2. **Column Lineage Panel**
   - When a column node is selected:
     - Show upstream and downstream paths.
     - List contributing columns and expressions.
   - Helps answer:
     - "Where does this column come from?"
     - "What does this column feed into?"

3. **SQL Highlight View**
   - Original SQL rendered with:
     - Syntax highlighting (can reuse existing code editor components, or keep minimal).
     - Inline highlights corresponding to:
       - Selected tables and columns.
       - Problematic spans indicated by issues.
   - Mouse interactions:
     - Click on highlighted segments to select the corresponding node in graph view.

4. **Issues Panel**
   - List of issues:
     - Severity (info, warning, error).
     - Message.
     - Optional link to highlight corresponding SQL span.

5. **Composite Explorer**
   - "All-in-one" component that ties:
     - Graph view.
     - SQL view.
     - Column lineage panel.
     - Issues panel.
   - Optional statement selector if input contains multiple statements.

### 1.4 Graph Rendering Library

The spec does not mandate a specific graph library, but requires that:

- Graph layout supports:
  - Directed edges.
  - Reasonable layout for DAGs.
- Nodes can display:
  - Labels (table/CTE/column names).
  - Icons or styling based on node type.

Libraries like ElkJS, Dagre, or Cytoscape.js are acceptable; the actual choice can be made during implementation.

### 1.5 Theming & Customization

- Provide reasonable default styling.
- Allow host applications to:
  - Override color palette and fonts.
  - Provide custom node renderers (for advanced integration scenarios) if practical.

## 2. Example Web App (`examples/web-demo`)

### 2.1 Purpose

A simple SPA that:

- Demonstrates the full flow:
  - User enters SQL and selects dialect.
  - Optional schema JSON is pasted/uploaded.
  - User clicks "Analyze".
  - The app shows:
    - Statement selector (if needed).
    - Graph.
    - Column details.
    - SQL highlighting.
    - Issues.

- Acts as a manual QA tool for developers.
- Serves as documentation for how to integrate `@pondpilot/flowscope-core` and `@pondpilot/flowscope-react`.

### 2.2 Features

- Text area or code editor for SQL input.
- Dialect selection dropdown.
- Schema metadata input:
  - Text area for JSON, or file upload.
- "Analyze" button:
  - Triggers call to `@pondpilot/flowscope-core`.
  - While running:
    - Show a progress indicator.
- Result area:
  - Use `@pondpilot/flowscope-react` composite components.
  - Show raw JSON results in a collapsible section for debugging.

### 2.3 Constraints

- The demo app should be light and focused:
  - No backend logic.
  - No authentication.
  - All state client-side.

## 3. Integrations (Future-Facing)

Even if not implemented in the MVP, the architecture should make these scenarios natural:

### 3.1 Browser Extension

- Content script:
  - Detect SQL input areas in target tools (Snowflake UI, BigQuery console, dbt Cloud, etc.).
  - Extract SQL from the page.
- Extension popup or side panel:
  - Use bundled `@pondpilot/flowscope-core` and `@pondpilot/flowscope-react`.
  - Run analysis in an extension worker/background page.
  - Display lineage without sending any data off the machine.

### 3.2 VS Code / Web IDE

- Extension:
  - Add a "Show Lineage" command:
    - Take the current file contents.
    - Pass SQL to `@pondpilot/flowscope-core`.
  - Render the results in a webview panel using the same React components.

### 3.3 Embedding into Third-Party Apps

- Third-party apps with SQL editors can:
  - Call `@pondpilot/flowscope-core.analyzeSql` when user clicks "Lineage".
  - Render the result using:
    - Either `@pondpilot/flowscope-react` components, or
    - Their own UI built atop the graph model.

No additional integration contracts beyond the **TS types and NPM APIs** are required for these scenarios.
