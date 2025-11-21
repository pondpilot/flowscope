# FlowScope Web Demo

Phase 0 demonstration of FlowScope SQL lineage analysis.

## Overview

This is a minimal web application that demonstrates the core FlowScope technology:
- Rust-based SQL parsing
- WASM compilation
- Browser integration
- Table-level lineage extraction

## Running the Demo

### Option 1: Browser

1. Start a local HTTP server:
   ```bash
   yarn dev
   # or
   python3 -m http.server 8080
   ```

2. Open http://localhost:8080 in your browser

3. Enter SQL queries and click "Analyze Lineage"

### Option 2: Node.js Test

```bash
node test.js
```

This runs automated tests that verify:
- Simple SELECT queries
- JOIN queries
- Error handling for invalid SQL

## Sample Queries

Try these in the demo:

```sql
SELECT * FROM users
```

```sql
SELECT * FROM users JOIN orders ON users.id = orders.user_id
```

```sql
SELECT u.name, o.total
FROM public.users u
INNER JOIN public.orders o ON u.id = o.user_id
```

## Architecture

```
┌─────────────────┐
│   HTML/JS UI    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  WASM Module    │  (flowscope_wasm.js + .wasm)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Rust Core      │  (flowscope-core)
│  - sqlparser-rs │
│  - lineage logic│
└─────────────────┘
```

## Files

- `index.html` - Demo UI
- `test.js` - Automated tests
- `public/wasm/` - Built WASM artifacts (generated)

## Building WASM

```bash
cd ../../crates/flowscope-wasm
wasm-pack build --target web --out-dir ../../examples/web-demo/public/wasm
```

## Current Features (Phase 0)

✅ Parse SQL queries
✅ Extract table names
✅ Handle JOINs
✅ Error handling
✅ JSON output

## Coming in Phase 1

- CTE support
- INSERT INTO SELECT
- CREATE TABLE AS SELECT
- Schema metadata
- Cross-statement lineage

## License

Apache 2.0
