import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'node:test';

import { findBudgetFailures, inspectBundle } from './check-bundle-budget.mjs';
import { removeLegacyWasm } from './remove-legacy-wasm.mjs';

const temporaryDirectories = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function createBundleFixture({ duplicateWasm = false } = {}) {
  const directory = mkdtempSync(path.join(tmpdir(), 'flowscope-bundle-budget-'));
  temporaryDirectories.push(directory);
  mkdirSync(path.join(directory, '.vite'), { recursive: true });
  mkdirSync(path.join(directory, 'assets'), { recursive: true });

  const manifest = {
    'index.html': {
      file: 'assets/entry.js',
      isEntry: true,
      imports: ['_vendor.js'],
      dynamicImports: ['src/lazy.ts'],
      css: ['assets/app.css'],
    },
    '_vendor.js': { file: 'assets/vendor.js' },
    'src/lazy.ts': { file: 'assets/lazy.js', isDynamicEntry: true },
  };
  writeFileSync(path.join(directory, '.vite', 'manifest.json'), JSON.stringify(manifest));
  writeFileSync(path.join(directory, 'assets', 'entry.js'), Buffer.alloc(10, 1));
  writeFileSync(path.join(directory, 'assets', 'vendor.js'), Buffer.alloc(20, 2));
  writeFileSync(path.join(directory, 'assets', 'lazy.js'), Buffer.alloc(100, 3));
  writeFileSync(path.join(directory, 'assets', 'app.css'), Buffer.alloc(5, 4));
  writeFileSync(path.join(directory, 'assets', 'engine.wasm'), Buffer.alloc(40, 5));
  if (duplicateWasm) {
    writeFileSync(path.join(directory, 'engine-copy.wasm'), Buffer.alloc(40, 5));
  }
  return directory;
}

const unlimitedBudgets = {
  wasmFileCount: 1,
  maxWasmBytes: Number.MAX_SAFE_INTEGER,
  maxEntryJsBytes: Number.MAX_SAFE_INTEGER,
  maxStartupJsBytes: Number.MAX_SAFE_INTEGER,
  maxStartupJsGzipBytes: Number.MAX_SAFE_INTEGER,
  maxStartupCssBytes: Number.MAX_SAFE_INTEGER,
  maxStartupCssGzipBytes: Number.MAX_SAFE_INTEGER,
  maxAsyncJsChunkBytes: Number.MAX_SAFE_INTEGER,
  maxTotalJsBytes: Number.MAX_SAFE_INTEGER,
  maxTotalDistBytes: Number.MAX_SAFE_INTEGER,
};

test('counts only static entry imports in the startup budget', () => {
  const metrics = inspectBundle(createBundleFixture());

  assert.equal(metrics.entryJsBytes, 10);
  assert.equal(metrics.startupJsBytes, 30);
  assert.equal(metrics.startupCssBytes, 5);
  assert.equal(metrics.totalJsBytes, 130);
  assert.equal(metrics.wasmFileCount, 1);
});

test('reports duplicate WASM assets', () => {
  const metrics = inspectBundle(createBundleFixture({ duplicateWasm: true }));
  const failures = findBudgetFailures(metrics, unlimitedBudgets);

  assert.deepEqual(
    failures.map((failure) => failure.metricName),
    ['wasmFileCount']
  );
});

test('reports startup JavaScript regressions', () => {
  const metrics = inspectBundle(createBundleFixture());
  const failures = findBudgetFailures(metrics, {
    wasmFileCount: 1,
    maxWasmBytes: 100,
    maxEntryJsBytes: 100,
    maxStartupJsBytes: 29,
    maxStartupJsGzipBytes: Number.MAX_SAFE_INTEGER,
    maxStartupCssBytes: 100,
    maxStartupCssGzipBytes: Number.MAX_SAFE_INTEGER,
    maxAsyncJsChunkBytes: 100,
    maxTotalJsBytes: 200,
    maxTotalDistBytes: 1000,
  });

  assert.deepEqual(
    failures.map((failure) => failure.metricName),
    ['startupJsBytes']
  );
});

test('rejects missing or malformed budget values', () => {
  const metrics = inspectBundle(createBundleFixture());

  assert.throws(
    () => findBudgetFailures(metrics, { ...unlimitedBudgets, maxTotalDistBytes: undefined }),
    /Invalid bundle budget "maxTotalDistBytes"/
  );
  assert.throws(
    () => findBudgetFailures(metrics, { ...unlimitedBudgets, wasmFileCount: 1.5 }),
    /Invalid bundle budget "wasmFileCount"/
  );
});

test('removes only the legacy public WASM directory', () => {
  const directory = mkdtempSync(path.join(tmpdir(), 'flowscope-legacy-wasm-'));
  temporaryDirectories.push(directory);
  mkdirSync(path.join(directory, 'public', 'wasm'), { recursive: true });
  writeFileSync(path.join(directory, 'public', 'wasm', 'flowscope_wasm_bg.wasm'), 'legacy');
  writeFileSync(path.join(directory, 'public', 'favicon.svg'), 'keep');

  removeLegacyWasm(directory);

  assert.equal(existsSync(path.join(directory, 'public', 'wasm')), false);
  assert.equal(existsSync(path.join(directory, 'public', 'favicon.svg')), true);
});
