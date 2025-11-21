# flowscope-wasm

WebAssembly bindings for flowscope-core.

## Overview

`flowscope-wasm` provides WebAssembly bindings for the FlowScope core engine, enabling SQL lineage analysis to run in browsers and Node.js.

## Building

```bash
wasm-pack build --target web --out-dir ../../examples/web-demo/public/wasm
```

This will generate:
- `flowscope_wasm_bg.wasm` - The WASM binary (~1.68 MB)
- `flowscope_wasm.js` - JavaScript glue code
- `flowscope_wasm.d.ts` - TypeScript definitions

## API

### `analyze_sql(sql: string): string`

Analyzes SQL and returns JSON string with lineage results.

**Parameters:**
- `sql` - SQL query string

**Returns:**
- JSON string with format: `{ "tables": ["table1", "table2", ...] }`

**Throws:**
- Error if SQL is invalid

## Usage in Browser

```javascript
import init, { analyze_sql } from './wasm/flowscope_wasm.js';

await init();

const result = analyze_sql('SELECT * FROM users');
const parsed = JSON.parse(result);
console.log(parsed.tables); // ["users"]
```

## Usage in Node.js

```javascript
import { readFile } from 'fs/promises';
import init, { analyze_sql } from './wasm/flowscope_wasm.js';

const wasmBuffer = await readFile('./wasm/flowscope_wasm_bg.wasm');
await init(wasmBuffer);

const result = analyze_sql('SELECT * FROM users');
console.log(JSON.parse(result));
```

## Testing

```bash
cd examples/web-demo
node test.js
```

## License

Apache 2.0
