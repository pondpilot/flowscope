# AGENTS.md

## Scope

This file applies to the entire FlowScope monorepo.

## Repo Overview

FlowScope is a Rust + TypeScript monorepo.
Key areas:
- `crates/` Rust workspace (core engine, wasm, CLI, export).
- `packages/` TypeScript packages (`@pondpilot/flowscope-core`, `@pondpilot/flowscope-react`).
- `app/` demo web app (Vite + React).
- `vscode/` VS Code extension + `vscode/webview-ui`.

## Cursor/Copilot Rules

- No `.cursor/rules/`, `.cursorrules`, or `.github/copilot-instructions.md` found.

## Tooling Defaults

- Use `just` as the task runner (see `justfile`).
- Use `yarn` for Node workspaces.
- Use `cargo` for Rust workspace.
- Node.js 18+ and Rust 1.82+ expected (see `README.md`).

## Build Commands

- `just build` (WASM + TypeScript packages).
- `just build-rust` (Rust workspace debug build).
- `just build-rust-release` (Rust workspace release build).
- `just build-cli` (CLI release build).
- `just build-cli-serve` (CLI with embedded web UI, release build).
- `just build-cli-serve-debug` (CLI with embedded web UI, debug build).
- `just build-wasm` (runs `./scripts/build-rust.sh`).
- `just build-ts` (runs `yarn build:ts`).
- `just run` (build + dev server).

### CLI Serve Mode Build Order

The CLI serve feature embeds the web app using rust-embed at compile time. This requires the app to be built first:

1. `cd app && yarn build` - Build frontend assets to `app/dist/`
2. `cargo build -p flowscope-cli --features serve` - Compile CLI with embedded assets

The `just build-cli-serve` target handles this dependency automatically.

## Dev Commands

- `just dev` (Vite dev server at `http://localhost:5173`).
- `yarn dev` (from `app/` if you want direct Vite usage).
- `just cli -- <args>` (run CLI in debug mode).
- `just cli-release -- <args>` (run CLI in release mode).

## Lint, Format, Typecheck

- `just lint` (Rust + TypeScript lint).
- `just lint-rust` (`cargo clippy --workspace -- -D warnings`).
- `just lint-ts` (`yarn workspaces run lint`).
- `just lint-fix` (`yarn workspaces run lint:fix`).
- `just fmt` (Rust + TS formatting).
- `just fmt-rust` (`cargo fmt --all`).
- `just fmt-check-rust` (`cargo fmt --all -- --check`).
- `just fmt-ts` (runs Prettier across workspaces).
- `just typecheck` (`yarn workspaces run typecheck`).

## Test Commands

- `just test` (Rust + TS tests).
- `just test-rust` (`cargo test --workspace`).
- `just test-rust-release` (`cargo test --workspace --release`).
- `just test-ts` (`yarn workspaces run test`).
- `just test-core` (`cargo test -p flowscope-core`).
- `just test-cli` (`cargo test -p flowscope-cli`).
- `just test-cli-serve` (CLI tests with serve feature, builds app first).
- `just test-lineage` (`cargo test -p flowscope-core --test lineage_engine`).
- `just test-lineage-verbose` (same test with `--nocapture`).
- `just check-schema` (Rust schema guard + TS schema compatibility).
- `just coverage` (generate HTML coverage report in `coverage/`, requires `cargo-llvm-cov`).
- `just coverage-lcov` (generate LCOV file at `lcov.info` for CI/Codecov).
- `just coverage-summary` (print coverage summary to stdout).

## Workspace Utilities

- `just install` (install Node dependencies).
- `just setup` (install deps + tools + hooks + build).
- `just install-rust-tools` (installs `wasm-pack` and `cargo-watch`).
- `just install-hooks` (install `prek` hooks).
- `just clean` (cargo clean + remove node_modules).
- `just update-schema` (regenerate API schema snapshot).
- `just check` (fmt check + lint + typecheck + schema checks).
- `just check-all` (Rust + TS + schema compatibility).
- `just watch` (rebuild Rust workspace on changes).
- `just watch-test` (run Rust tests on changes).
- `just watch-lineage` (run lineage tests on changes).

## Package-Level Scripts

- `packages/core`: `yarn test`, `yarn test:watch`, `yarn lint`, `yarn typecheck`.
- `packages/react`: `yarn test`, `yarn test:watch`, `yarn lint`, `yarn typecheck`.
- `app`: `yarn dev`, `yarn build`, `yarn preview`, `yarn lint`, `yarn typecheck`.
- `vscode`: `npm run build`, `npm run build:webview`, `npm run watch`, `npm run typecheck`.
- `vscode/webview-ui`: `yarn build`, `yarn dev`, `yarn lint`, `yarn typecheck`.

## Single Test Tips

- Rust lineage tests: `just test-lineage-filter PATTERN`.
- Rust lineage tests (manual): `cargo test -p flowscope-core --test lineage_engine PATTERN`.
- Schema compatibility: `yarn workspace @pondpilot/flowscope-core test schema-compat.test.ts --silent`.
- For other Rust crates, use `cargo test -p <crate>` then filter with a test name if needed.

## Code Style (Rust)

- Follow standard Rust conventions (see `CONTRIBUTING.md`).
- Format with `cargo fmt` before committing.
- Lint with `cargo clippy` and fix warnings.
- Workspace uses Rust 2021 edition (see `Cargo.toml`).
- Avoid unused variables; if intentionally unused, prefix with `_`.
- Keep modules small and focused; `flowscope-core` uses a layered analyzer.

## Error Handling (Rust)

- Use `ParseError` for fatal parsing failures returned via `Result<T, ParseError>`.
- Use `Issue` for non-fatal analysis problems (collected and returned alongside results).
- `flowscope-cli` uses `anyhow::Result` with `Context` for CLI errors.
- `flowscope-export` and analyzer input use `thiserror::Error` for structured errors.

## Code Style (TypeScript)

- TypeScript is strict (see `CONTRIBUTING.md`).
- ESM modules are used (`"type": "module"`).
- Use single quotes and trailing commas in multiline structures.
- Prettier config:
  - `printWidth`: 100
  - `tabWidth`: 2
  - `semi`: true
  - `singleQuote`: true
  - `trailingComma`: `es5`
  - `arrowParens`: `always`
- ESLint config highlights:
  - `@typescript-eslint/no-unused-vars`: error, allow unused args with `_` prefix.
  - `@typescript-eslint/explicit-module-boundary-types`: off.

## Testing Expectations

- Add unit tests for new functionality (`CONTRIBUTING.md`).
- Add integration tests for complex features.
- Use fixtures under `crates/flowscope-core/tests/fixtures/` when needed.
- Keep test output clean; avoid noisy logs unless `--nocapture` is intended.

## Docs and Updates

- Update documentation and `CHANGELOG.md` if a change requires it (see `CONTRIBUTING.md`).
- Docs index and specs live in `docs/README.md`.
- Usage guides live in `docs/guides/`.
- The CLI usage details live in `crates/flowscope-cli/README.md`.
- Core engine overview lives in `crates/flowscope-core/README.md`.

## Releases (Single Tag)

Use a single repo tag for each release (`vX.Y.Z`) and align Rust workspace + npm package versions.

1. Update versions:
   - `Cargo.toml` workspace version + workspace dependencies
   - `packages/core/package.json`, `packages/react/package.json`, `packages/core/wasm/package.json`
   - Update peer dependency on `@pondpilot/flowscope-core` in `packages/react`
2. Update `CHANGELOG.md`:
   - Move Unreleased entries to `## [X.Y.Z] - YYYY-MM-DD`
   - Summarize changes per crate/package
3. Validate:
   - `just fmt-rust`
   - `just test-core`
   - `yarn workspace @pondpilot/flowscope-react build`
   - `yarn workspace @pondpilot/flowscope-core build`
4. Publish crates (order matters):
   - `cargo publish -p flowscope-core`
   - `cargo publish -p flowscope-export`
   - `cargo publish -p flowscope-cli`
5. Publish npm packages:
   - `yarn workspace @pondpilot/flowscope-core publish --access public`
   - `yarn workspace @pondpilot/flowscope-react publish --access public`
6. Tag + release:
   - `git tag vX.Y.Z`
   - `git push origin vX.Y.Z`
   - `gh release create vX.Y.Z --title "vX.Y.Z" --notes-file <notes>` (use CHANGELOG notes)

## Notes

- The demo app (`app/`) and VS Code webview (`vscode/webview-ui/`) currently define no tests.
- For full CI parity, `just check` runs formatting checks, lint, typecheck, and schema checks.
