# FlowScope Studio

The official web interface for FlowScope. This application demonstrates the full capabilities of the client-side SQL lineage engine.

## Features

- **Interactive Lineage Graph:** Visualize table and column dependencies.
- **Multi-File Workspace:** Manage multiple SQL files and analyze cross-file dependencies (simulated).
- **Schema Editor:** Define table schemas to enable advanced analysis features like wildcard expansion and column validation.
- **Privacy-First:** All processing happens locally in the browser via WebAssembly.

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

The application loads the WASM module generated from `@crates/flowscope-wasm`.
- **State Management:** Zustand
- **UI Components:** React Flow (graph), CodeMirror (editor), Tailwind CSS

## License

Released under the O'Saasy License. See `LICENSE` for details.
