# FlowScope – In-Browser SQL Lineage Engine (Spec Pack)

**Part of the PondPilot project ecosystem**

This repo describes **FlowScope**, a **fully client-side SQL parsing and lineage engine** built on **Rust + WebAssembly + TypeScript**, with optional React UI components.

The core idea:

> Give developers an embeddable, privacy-preserving "SQLFlow-style" lineage engine that runs entirely in the browser (or other JS runtimes), with multi-dialect support and table/column-level lineage.

## Document Map

### Core Specifications

- [`product-scope.md`](./product-scope.md)
  What we're building, who it's for, and what it's explicitly *not*.

- [`architecture-overview.md`](./architecture-overview.md)
  High-level system design, components, data flows, and packaging.

- [`core-engine-spec.md`](./core-engine-spec.md)
  Rust lineage engine: parsing, dialects, lineage computation, internal models.

- [`wasm-and-js-layer.md`](./wasm-and-js-layer.md)
  WASM export, JS/TS wrapper, worker model, public APIs for consumers.

- [`api-types.md`](./api-types.md)
  Exact TypeScript/Rust type definitions for the WASM boundary.

- [`ui-and-integrations.md`](./ui-and-integrations.md)
  React viewer package, example app, and integration surfaces (browser, IDE, external tools).

### Quality & Planning

- [`testing-and-quality.md`](./testing-and-quality.md)
  Testing strategy, sample datasets, regression harness, performance & correctness checks.

- [`roadmap-and-phasing.md`](./roadmap-and-phasing.md)
  Implementation phases and priorities (MVP → v1.0 → later).

### Implementation Guide

- [`implementation-decisions.md`](./implementation-decisions.md)
  All technical decisions made during design: build tools, libraries, strategies, and constraints.

- [`design-gaps-and-questions.md`](./design-gaps-and-questions.md)
  Open questions, resolved gaps, and future design considerations.

## High-Level Summary

- **Core engine:**
  Rust crates (`flowscope-core`, `flowscope-wasm`) using `sqlparser-rs` for multi-dialect SQL parsing and a custom lineage engine that computes **table- and column-level lineage graphs** from SQL.

- **Runtime environment:**
  Compiled to WebAssembly for **pure client-side** execution in browsers and JS runtimes (Node/Deno is a nice-to-have, not a hard requirement).

- **JS/TS wrapper:**
  An NPM package (`@pondpilot/flowscope-core`) providing:
  - WASM initialization and lifecycle.
  - A single high-level `analyzeSql(...)` style entrypoint that returns a **lineage graph** plus **issues** and **summary** metadata.
  - Optional Web Worker helper for UI apps.

- **Input requirements:**
  Host applications must provide **fully rendered SQL text**. dbt/Dagster templating, macros, or other preprocessing happens entirely outside this engine.

- **UI layer (optional but in scope):**
  React-based components (`@pondpilot/flowscope-react`) for:
  - Rendering lineage graphs.
  - Showing column-level lineage and expressions.
  - Highlighting the originating SQL.

- **Cross-statement insight:**
  Results include both per-statement lineage and a **global dependency graph** so UIs can answer impact-analysis questions across entire scripts.

- **Security/privacy stance:**
  No network calls from the engine. Host apps retain full control of SQL/schema data (can keep everything local to the browser).

This spec is written to be directly consumable by a coding agent/team with minimal back-and-forth.
