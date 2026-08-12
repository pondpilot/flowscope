# FlowScope Studio

The official web interface for FlowScope. This application demonstrates the full capabilities of the client-side SQL lineage engine.

## Features

- **Interactive Lineage Graph:** Visualize table and column dependencies.
- **Multi-File Workspace:** Manage multiple SQL files and analyze cross-file dependencies.
- **Schema Editor:** Define table schemas to enable advanced analysis features like wildcard expansion and column validation.
- **SQL Linting:** Real-time lint diagnostics in the Issues panel. 72 rules check aliasing, layout, conventions, and structure.
- **Local Analysis:** SQL lineage analysis runs locally in the browser via WebAssembly.
- **Opt-in Librarian:** After you configure an AI provider and submit a question, Librarian sends the active SQL snippet, formatted lineage, relevant PDF text excerpts and citations, recent chat history, and the question to that provider.

## Development

This project uses Vite and React.

### Prerequisites

- Node.js 18+
- Yarn

### Setup

```bash
yarn install
```

### Running Locally

```bash
yarn dev
```

This will start the development server at `http://localhost:5173`.

### Architecture

The browser WASM artifact has one canonical home: `packages/core/wasm/`. The
`@pondpilot/flowscope-core` loader dynamically imports the generated glue, and
Vite emits the referenced `.wasm` file as a single hashed asset for both the dev
server and production build. Do not copy WASM into `app/public`; public assets
are copied verbatim and would duplicate the package-owned bytes. CLI serve mode
embeds the same `app/dist` output, so it follows this loading path without a
second WASM copy. The VS Code extension intentionally uses its separate
Node-target build under `vscode/wasm-node/`.

The default editor, Dagre graph, and analysis worker are startup features.
Non-default analysis tabs, ELK layout, Librarian, share dialogs, PNG export,
PDF parsing, and local embeddings load on first use. Keep optional heavyweight
features behind an interaction-driven dynamic import rather than adding them to
the startup graph.

Production builds run `yarn check:bundle` automatically. The checker reads the
Vite manifest so only the entry and its static imports count toward startup;
dynamic feature chunks remain subject to the per-chunk and total budgets.
Thresholds live in `bundle-budgets.json`:

| Budget                     |                        Threshold |
| -------------------------- | -------------------------------: |
| Emitted WASM               |    exactly 1 file, at most 9 MiB |
| Entry/startup JavaScript   |  at most 3 MiB raw, 600 KiB gzip |
| Startup CSS                | at most 128 KiB raw, 24 KiB gzip |
| Any async JavaScript chunk |                 at most 2.25 MiB |
| All JavaScript             |                    at most 8 MiB |
| Entire `dist`              |                   at most 18 MiB |

Run the checker and its focused tests with:

```bash
yarn check:bundle
yarn test:bundle-budget
```

- **State Management:** Zustand
- **UI Components:** React Flow (graph), CodeMirror (editor), Tailwind CSS

## License

Released under the O'Saasy License. See `LICENSE` for details.
