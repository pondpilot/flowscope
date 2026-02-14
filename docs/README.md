# FlowScope Documentation

This folder contains the canonical documentation for FlowScope. The public API and behavior are defined by the Rust/TypeScript code, and these docs are kept in sync with the current implementation.

## Document Map

## License

FlowScope code is released under Apache-2.0 unless stated otherwise. The `app/` directory uses the O'Saasy License; see `app/LICENSE`.

### Core References

- `architecture-overview.md`
  System-level overview of Rust, WASM, and TS layers.
- `workspace-structure.md`
  Monorepo layout, package relationships, and build entry points.
- `core-engine-spec.md`
  Behavior and responsibilities of the Rust analyzer.
- `api-types.md`
  API surface for the TS wrapper (mirrors `packages/core/src/types.ts`).
- `schema-handling-design.md`
  Schema metadata rules, implied schema capture, and resolution behavior.
- `column_lineage.md`
  Column lineage semantics and edge types.
- `dialect-coverage.md`
  Supported dialect list and high-level statement coverage.
- `sqlfluff-gap-matrix.md`
  Rule-by-rule SQLFluff vs FlowScope lint parity matrix.
- `linter-architecture.md`
  Principles and key design decisions for AST-first, token-aware lint architecture.
- `dialect_compliance_spec.md`
  Dialect normalization and scoping rules used by the analyzer.
- `comprehensive_dialect_rules.md`
  Source of truth for dialect semantics in `crates/flowscope-core/specs/`.
- `error-codes.md`
  Issue code reference for `AnalyzeResult.issues`.

### Guides

- `guides/quickstart.md`
  TypeScript quickstart and usage patterns.
- `guides/schema-metadata.md`
  How to pass schema metadata for better lineage.
- `guides/error-handling.md`
  Interpreting issues and handling partial results.

### Generated Artifacts

- `api_schema.json`
  JSON schema snapshot generated from Rust types.
- `crates/flowscope-core/src/generated/`
  Rust code generated from `crates/flowscope-core/specs/dialect-semantics/` via `build.rs`.

### Release Docs

- `publishing.md`
  NPM publishing flow for `@pondpilot/flowscope-core`.
