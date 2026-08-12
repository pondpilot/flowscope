import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, it, expect } from 'vitest';
import Ajv from 'ajv';
import { VALID_DIALECTS, type AnalyzeRequest, type AnalyzeResult } from '../src/types';

const schemaPath = new URL('../../../docs/api_schema.json', import.meta.url);
const generatedTypesPath = new URL('../src/generated/api-types.ts', import.meta.url);
const generatorPath = new URL('../../../scripts/generate_ts_api_types.mjs', import.meta.url);
const repoRoot = fileURLToPath(new URL('../../../', import.meta.url));

interface ApiSchemaSnapshot {
  AnalyzeRequest: {
    definitions: Record<string, unknown> & { Dialect: { enum: string[] } };
  };
  AnalyzeResult: { definitions: Record<string, unknown> };
  [name: string]: { definitions: Record<string, unknown> };
}

function loadSnapshot(): ApiSchemaSnapshot {
  return JSON.parse(readFileSync(schemaPath, 'utf8')) as ApiSchemaSnapshot;
}

function loadSchemas() {
  const raw = readFileSync(schemaPath, 'utf8');
  const parsed = JSON.parse(raw);
  const ajv = new Ajv({ allErrors: true, strict: false, validateFormats: true });
  // schemars emits "uint" format for unsigned integers; teach Ajv how to handle it
  ajv.addFormat('uint', { type: 'number', validate: (n: number) => Number.isInteger(n) && n >= 0 });
  ajv.addFormat('uint8', {
    type: 'number',
    validate: (n: number) => Number.isInteger(n) && n >= 0 && n <= 255,
  });
  ajv.addSchema(parsed.AnalyzeRequest, 'AnalyzeRequest');
  ajv.addSchema(parsed.AnalyzeResult, 'AnalyzeResult');
  return ajv;
}

describe('API schema compatibility', () => {
  it('has a current generated declaration for every Rust schema type', () => {
    const result = spawnSync(process.execPath, [fileURLToPath(generatorPath), '--check'], {
      cwd: repoRoot,
      encoding: 'utf8',
    });
    expect(result.status, result.stderr || result.stdout).toBe(0);

    const snapshot = loadSnapshot();
    const expectedNames = new Set(
      Object.entries(snapshot).flatMap(([rootName, schema]) => [
        rootName,
        ...Object.keys(schema.definitions),
      ])
    );
    const generatedSource = readFileSync(generatedTypesPath, 'utf8');
    const generatedNames = new Set(
      [...generatedSource.matchAll(/^export (?:interface|type) (\w+)/gm)].map((match) => match[1])
    );

    expect([...generatedNames].sort()).toEqual([...expectedNames].sort());
  });

  it('safely generates Rust descriptions containing a JSDoc terminator', () => {
    const result = spawnSync(process.execPath, [fileURLToPath(generatorPath), '--self-test'], {
      cwd: repoRoot,
      encoding: 'utf8',
    });
    expect(result.status, result.stderr || result.stdout).toBe(0);
  });

  it('keeps the runtime dialect list exhaustive with the Rust enum', () => {
    const rustDialects = loadSnapshot().AnalyzeRequest.definitions.Dialect.enum;
    expect([...VALID_DIALECTS]).toEqual(rustDialects);
  });

  it('compiles both complete Rust root schemas with all local definitions', () => {
    const ajv = loadSchemas();
    expect(ajv.getSchema('AnalyzeRequest')).toBeDefined();
    expect(ajv.getSchema('AnalyzeResult')).toBeDefined();
  });

  it('validates a typed AnalyzeRequest against Rust schema', () => {
    const ajv = loadSchemas();
    const validate = ajv.getSchema<AnalyzeRequest>('AnalyzeRequest');
    expect(validate).toBeDefined();

    const request: AnalyzeRequest = {
      sql: 'SELECT id, name FROM users',
      dialect: 'postgres',
      files: undefined,
      options: { enableColumnLineage: true },
      schema: {
        defaultSchema: 'public',
        tables: [{ name: 'users', columns: [{ name: 'id' }, { name: 'name' }] }],
      },
    };

    const valid = validate?.(request);
    expect(valid).toBe(true);
    if (!valid) {
      expect(validate?.errors).toBeUndefined();
    }
  });

  it('validates a typed AnalyzeResult against Rust schema', () => {
    const ajv = loadSchemas();
    const validate = ajv.getSchema<AnalyzeResult>('AnalyzeResult');
    expect(validate).toBeDefined();

    const result: AnalyzeResult = {
      statements: [
        {
          statementIndex: 0,
          statementType: 'SELECT',
          sourceName: 'inline.sql',
          joinCount: 0,
          complexityScore: 0,
        },
      ],
      nodes: [],
      edges: [],
      issues: [],
      summary: {
        statementCount: 1,
        tableCount: 0,
        columnCount: 0,
        joinCount: 0,
        complexityScore: 0,
        issueCount: { errors: 0, warnings: 0, infos: 0 },
        hasErrors: false,
      },
    };

    const valid = validate?.(result);
    expect(valid).toBe(true);
    if (!valid) {
      expect(validate?.errors).toBeUndefined();
    }
  });

  it('validates an AnalyzeResult with autofix metadata against Rust schema', () => {
    const ajv = loadSchemas();
    const validate = ajv.getSchema<AnalyzeResult>('AnalyzeResult');
    expect(validate).toBeDefined();

    const result: AnalyzeResult = {
      statements: [
        {
          statementIndex: 0,
          statementType: 'SELECT',
          sourceName: 'inline.sql',
          joinCount: 0,
          complexityScore: 1,
        },
      ],
      nodes: [],
      edges: [],
      issues: [
        {
          severity: 'warning',
          code: 'LINT_CP_001',
          message: 'Keywords should be upper-case.',
          sqlfluffName: 'capitalisation.keywords',
          span: { start: 0, end: 6 },
          statementIndex: 0,
          lintEngine: 'lexical',
          lintConfidence: 'high',
          autofix: {
            applicability: 'safe',
            edits: [{ span: { start: 0, end: 6 }, replacement: 'SELECT' }],
          },
        },
        {
          severity: 'warning',
          code: 'LINT_CP_002',
          message: 'Identifiers should be lower-case.',
          span: { start: 7, end: 9 },
          statementIndex: 0,
          lintEngine: 'semantic',
          lintConfidence: 'medium',
          lintFallbackSource: 'parser_fallback',
          autofix: {
            applicability: 'displayOnly',
            edits: [{ span: { start: 7, end: 9 }, replacement: 'id' }],
          },
        },
        {
          severity: 'info',
          code: 'LINT_DOC_001',
          message: 'Document-level check.',
          lintEngine: 'document',
          lintConfidence: 'low',
          lintFallbackSource: 'heuristic_rule',
          autofix: {
            applicability: 'unsafe',
            edits: [],
          },
        },
      ],
      summary: {
        statementCount: 1,
        tableCount: 1,
        columnCount: 1,
        joinCount: 0,
        complexityScore: 1,
        issueCount: { errors: 0, warnings: 2, infos: 1 },
        hasErrors: false,
      },
    };

    const valid = validate?.(result);
    expect(valid).toBe(true);
    if (!valid) {
      expect(validate?.errors).toBeUndefined();
    }
  });
});
