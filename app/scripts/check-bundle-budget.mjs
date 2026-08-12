import { readFileSync, readdirSync, statSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_DIR = path.dirname(SCRIPT_DIR);

function walkFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const absolutePath = path.join(directory, entry.name);
    return entry.isDirectory() ? walkFiles(absolutePath) : [absolutePath];
  });
}

function sumFileSizes(files) {
  return files.reduce((total, file) => total + statSync(file).size, 0);
}

function sumGzipSizes(files) {
  return files.reduce(
    (total, file) => total + gzipSync(readFileSync(file), { level: 9 }).length,
    0
  );
}

function collectStartupManifestKeys(manifest, key, keys = new Set()) {
  if (keys.has(key)) return keys;
  const entry = manifest[key];
  if (!entry) {
    throw new Error(`Bundle manifest references missing entry: ${key}`);
  }

  keys.add(key);
  for (const importedKey of entry.imports ?? []) {
    collectStartupManifestKeys(manifest, importedKey, keys);
  }
  return keys;
}

export function inspectBundle(distDirectory) {
  const manifestPath = path.join(distDirectory, '.vite', 'manifest.json');
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const entryKeys = Object.keys(manifest).filter((key) => manifest[key].isEntry);
  if (entryKeys.length !== 1) {
    throw new Error(
      `Expected exactly one application entry in the Vite manifest, found ${entryKeys.length}`
    );
  }

  const entry = manifest[entryKeys[0]];
  const startupKeys = collectStartupManifestKeys(manifest, entryKeys[0]);
  const startupEntries = [...startupKeys].map((key) => manifest[key]);
  const startupJsRelative = new Set(
    startupEntries.map((item) => item.file).filter((file) => /\.(?:js|mjs)$/.test(file))
  );
  const startupCssRelative = new Set(startupEntries.flatMap((item) => item.css ?? []));
  const absolute = (relativePath) => path.join(distDirectory, relativePath);

  const allFiles = walkFiles(distDirectory);
  const jsFiles = allFiles.filter((file) => /\.(?:js|mjs)$/.test(file));
  const wasmFiles = allFiles.filter((file) => file.endsWith('.wasm'));
  const startupJsFiles = [...startupJsRelative].map(absolute);
  const startupCssFiles = [...startupCssRelative].map(absolute);
  const entryFile = absolute(entry.file);
  const startupJsAbsolute = new Set(startupJsFiles);
  const asyncJsFiles = jsFiles.filter((file) => !startupJsAbsolute.has(file));
  const largestAsyncJsFile =
    asyncJsFiles.length > 0
      ? asyncJsFiles.reduce((largest, file) =>
          statSync(file).size > statSync(largest).size ? file : largest
        )
      : null;

  return {
    entryJsBytes: statSync(entryFile).size,
    startupJsBytes: sumFileSizes(startupJsFiles),
    startupJsGzipBytes: sumGzipSizes(startupJsFiles),
    startupCssBytes: sumFileSizes(startupCssFiles),
    startupCssGzipBytes: sumGzipSizes(startupCssFiles),
    wasmFileCount: wasmFiles.length,
    largestWasmBytes:
      wasmFiles.length > 0 ? Math.max(...wasmFiles.map((file) => statSync(file).size)) : 0,
    largestAsyncJsChunkBytes: largestAsyncJsFile ? statSync(largestAsyncJsFile).size : 0,
    largestAsyncJsChunk: largestAsyncJsFile
      ? path.relative(distDirectory, largestAsyncJsFile)
      : '(none)',
    totalJsBytes: sumFileSizes(jsFiles),
    totalDistBytes: sumFileSizes(allFiles),
  };
}

const CHECKS = [
  ['wasmFileCount', 'wasmFileCount', 'exactly'],
  ['largestWasmBytes', 'maxWasmBytes', 'at most'],
  ['entryJsBytes', 'maxEntryJsBytes', 'at most'],
  ['startupJsBytes', 'maxStartupJsBytes', 'at most'],
  ['startupJsGzipBytes', 'maxStartupJsGzipBytes', 'at most'],
  ['startupCssBytes', 'maxStartupCssBytes', 'at most'],
  ['startupCssGzipBytes', 'maxStartupCssGzipBytes', 'at most'],
  ['largestAsyncJsChunkBytes', 'maxAsyncJsChunkBytes', 'at most'],
  ['totalJsBytes', 'maxTotalJsBytes', 'at most'],
  ['totalDistBytes', 'maxTotalDistBytes', 'at most'],
];

export function validateBudgets(budgets) {
  for (const [, budgetName, comparison] of CHECKS) {
    const budget = budgets[budgetName];
    const isValidNumber = typeof budget === 'number' && Number.isFinite(budget) && budget >= 0;
    const isValidExactCount = comparison !== 'exactly' || Number.isInteger(budget);
    if (!isValidNumber || !isValidExactCount) {
      const expected = comparison === 'exactly' ? 'a nonnegative integer' : 'a nonnegative number';
      throw new Error(`Invalid bundle budget "${budgetName}": expected ${expected}`);
    }
  }
}

export function findBudgetFailures(metrics, budgets) {
  validateBudgets(budgets);
  return CHECKS.flatMap(([metricName, budgetName, comparison]) => {
    const actual = metrics[metricName];
    const budget = budgets[budgetName];
    const failed = comparison === 'exactly' ? actual !== budget : actual > budget;
    return failed ? [{ metricName, budgetName, actual, budget, comparison }] : [];
  });
}

export function formatBytes(bytes) {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}

function printMetrics(metrics) {
  console.log('Bundle budget summary');
  console.log(`  WASM files: ${metrics.wasmFileCount}`);
  console.log(`  WASM size: ${formatBytes(metrics.largestWasmBytes)}`);
  console.log(`  Entry JS: ${formatBytes(metrics.entryJsBytes)}`);
  console.log(
    `  Startup JS: ${formatBytes(metrics.startupJsBytes)} raw / ${formatBytes(metrics.startupJsGzipBytes)} gzip`
  );
  console.log(
    `  Startup CSS: ${formatBytes(metrics.startupCssBytes)} raw / ${formatBytes(metrics.startupCssGzipBytes)} gzip`
  );
  console.log(
    `  Largest async JS chunk: ${formatBytes(metrics.largestAsyncJsChunkBytes)} (${metrics.largestAsyncJsChunk})`
  );
  console.log(`  Total JS: ${formatBytes(metrics.totalJsBytes)}`);
  console.log(`  Total dist: ${formatBytes(metrics.totalDistBytes)}`);
}

function run() {
  const distDirectory = path.resolve(process.argv[2] ?? path.join(APP_DIR, 'dist'));
  const budgetPath = path.resolve(process.argv[3] ?? path.join(APP_DIR, 'bundle-budgets.json'));
  const budgets = JSON.parse(readFileSync(budgetPath, 'utf8'));
  const metrics = inspectBundle(distDirectory);
  const failures = findBudgetFailures(metrics, budgets);

  printMetrics(metrics);
  if (failures.length === 0) {
    console.log('Bundle budgets passed.');
    return;
  }

  console.error('\nBundle budgets failed:');
  for (const failure of failures) {
    const actual =
      failure.metricName === 'wasmFileCount' ? failure.actual : formatBytes(failure.actual);
    const budget =
      failure.metricName === 'wasmFileCount' ? failure.budget : formatBytes(failure.budget);
    console.error(
      `  ${failure.metricName}: ${actual}; expected ${failure.comparison} ${budget} (${failure.budgetName})`
    );
  }
  process.exitCode = 1;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  run();
}
