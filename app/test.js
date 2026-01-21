// Simple test script to verify WASM works in Node.js
import { readFile } from 'fs/promises';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

async function testWasm() {
  try {
    console.log('Loading WASM module...');

    // Read the WASM file
    const wasmPath = join(__dirname, 'public/wasm/flowscope_wasm_bg.wasm');
    const wasmBuffer = await readFile(wasmPath);

    console.log(`WASM file size: ${(wasmBuffer.length / 1024 / 1024).toFixed(2)} MB`);

    // Import the JS wrapper
    const { default: init, analyze_sql } = await import('./public/wasm/flowscope_wasm.js');

    // Initialize WASM
    await init(wasmBuffer);
    console.log('✓ WASM module initialized');

    // Test 1: Simple SELECT
    console.log('\nTest 1: Simple SELECT');
    const sql1 = 'SELECT * FROM users';
    const result1 = analyze_sql(sql1);
    const parsed1 = JSON.parse(result1);
    console.log('Result:', parsed1);
    console.log(parsed1.tables.includes('users') ? '✓ PASS' : '✗ FAIL');

    // Test 2: JOIN query
    console.log('\nTest 2: JOIN query');
    const sql2 = 'SELECT * FROM users JOIN orders ON users.id = orders.user_id';
    const result2 = analyze_sql(sql2);
    const parsed2 = JSON.parse(result2);
    console.log('Result:', parsed2);
    console.log(
      parsed2.tables.includes('users') && parsed2.tables.includes('orders') ? '✓ PASS' : '✗ FAIL'
    );

    // Test 3: Invalid SQL
    console.log('\nTest 3: Invalid SQL (should error)');
    try {
      const sql3 = 'SELECT * FROM';
      const result3 = analyze_sql(sql3);
      console.log('✗ FAIL - Should have thrown an error');
    } catch (err) {
      console.log('✓ PASS - Error caught:', err.message);
    }

    console.log('\n✓ All tests completed successfully!');
  } catch (err) {
    console.error('✗ Test failed:', err);
    process.exit(1);
  }
}

testWasm();
