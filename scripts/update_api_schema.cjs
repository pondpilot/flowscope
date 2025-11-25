#!/usr/bin/env node
'use strict';

const { spawnSync } = require('node:child_process');
const { writeFileSync } = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..');
const schemaPath = path.join(repoRoot, 'docs', 'api_schema.json');

function runCargoSchemaTest() {
  const args = [
    'test',
    '-p',
    'flowscope-core',
    '--test',
    'schema_guard',
    'regenerate_api_schema_snapshot',
    '--',
    '--ignored',
    '--nocapture',
  ];

  const result = spawnSync('cargo', args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['inherit', 'pipe', 'pipe'],
  });

  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? '');
    process.stderr.write(result.stderr ?? '');
    process.exit(result.status ?? 1);
  }

  return result.stdout ?? '';
}

const output = runCargoSchemaTest();
const jsonStart = output.indexOf('{\n  "AnalyzeRequest"');
const endMarkerIndex = output.indexOf('\ntest ', jsonStart);

if (jsonStart === -1 || endMarkerIndex === -1) {
  console.error('Failed to locate schema JSON in cargo output. Output was:\n', output);
  process.exit(1);
}

const jsonText = output.slice(jsonStart, endMarkerIndex).trim();

try {
  JSON.parse(jsonText);
} catch (error) {
  console.error('Failed to parse schema JSON:', error);
  process.exit(1);
}

writeFileSync(schemaPath, `${jsonText}\n`, 'utf8');
console.log(`Updated ${path.relative(repoRoot, schemaPath)}`);
