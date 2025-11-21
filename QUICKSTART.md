# FlowScope Quickstart Guide

Welcome to FlowScope! This guide will get you up and running in 5 minutes.

## What is FlowScope?

FlowScope is a SQL lineage analysis engine that runs in the browser using WebAssembly. It parses SQL queries and traces data flow through tables, columns, and transformations.

**Phase 0 Status:** ✅ Complete - Core tech stack validated

## Quick Demo

### 1. Run the Web Demo

```bash
cd app
python3 -m http.server 8080
```

Then open http://localhost:8080 in your browser and try these queries:

```sql
SELECT * FROM users
```

```sql
SELECT * FROM users JOIN orders ON users.id = orders.user_id
```

### 2. Run Automated Tests

```bash
# Rust tests
cargo test --workspace

# Web demo tests
cd app && node test.js
```

## What's Working (Phase 0)

✅ SQL parsing (powered by sqlparser-rs)
✅ Table extraction from SELECT queries
✅ JOIN support
✅ WASM compilation (1.68 MB bundle)
✅ Browser integration
✅ Error handling

## Project Structure

```
flowscope/
├── crates/                    # Rust code
│   ├── flowscope-core/        # Core lineage engine
│   └── flowscope-wasm/        # WASM bindings
├── packages/                  # NPM packages (Phase 1+)
│   ├── core/                  # TypeScript wrapper
│   └── react/                 # React components
├── examples/
│   └── web-demo/              # Working demo
├── docs/                      # Specifications
└── TODO.md                    # Implementation roadmap
```

## Build from Source

### Prerequisites

- Rust (latest stable)
- Node.js >= 18
- wasm-pack: `cargo install wasm-pack`

### Build Steps

```bash
# 1. Build Rust core
cargo build

# 2. Run tests
cargo test

# 3. Build WASM
cd crates/flowscope-wasm
wasm-pack build --target web --out-dir ../../app/public/wasm

# 4. Test the demo
cd ../../app
node test.js
```

## Next Steps

- 📖 Read [PHASE_0_SPIKE_RESULTS.md](./PHASE_0_SPIKE_RESULTS.md) for technical details
- 📋 See [TODO.md](./TODO.md) for the full roadmap
- 🏗️ Check [docs/](./docs/) for architecture and design docs
- 🚀 Phase 1 is ready to start!

## Development Workflow

```bash
# Watch Rust changes
cargo watch -x test

# Rebuild WASM when needed
cd crates/flowscope-wasm && wasm-pack build --target web --out-dir ../../app/public/wasm

# Run demo
cd app && python3 -m http.server 8080
```

## Common Commands

```bash
# Build everything
cargo build --workspace

# Test everything
cargo test --workspace

# Format code
cargo fmt

# Lint code
cargo clippy

# Clean build artifacts
cargo clean
```

## Getting Help

- 📚 Documentation: [docs/README.md](./docs/README.md)
- 🐛 Issues: https://github.com/pondpilot/flowscope/issues
- 💬 Discussions: https://github.com/pondpilot/flowscope/discussions

## License

Apache 2.0 - See [LICENSE](./LICENSE)

---

**Status:** Phase 0 Complete ✅ | Ready for Phase 1 🚀
