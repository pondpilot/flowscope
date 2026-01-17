# Dialect Semantics Reference

This document explains where FlowScope’s dialect behavior is defined and how it flows into the analyzer. It intentionally avoids duplicating the semantic tables to prevent drift.

## Canonical Data

Dialect semantics are maintained in:

- `crates/flowscope-core/specs/dialect-semantics/`

These files are compiled into Rust in `crates/flowscope-core/src/generated/` by `build.rs`.

## What’s Covered

The semantic specs capture:

- Case sensitivity and identifier normalization
- Alias visibility rules (`GROUP BY`, `HAVING`, `ORDER BY`)
- Function classification (aggregate, window, table-generating)
- Dialect-specific function argument handling
- NULL ordering defaults

## How to Update

- Update the appropriate TOML/JSON file under `dialect-semantics/`.
- Regenerate the Rust outputs by rebuilding the workspace.
- Add tests or fixtures in `crates/flowscope-core/tests/` to validate behavior.

For practical usage, see `dialect_compliance_spec.md` and `dialect-coverage.md`.
