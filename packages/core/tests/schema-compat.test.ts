import { readFileSync } from 'node:fs';
import { describe, it, expect } from 'vitest';
import Ajv from 'ajv';
import type { AnalyzeRequest, AnalyzeResult } from '../src/types';

const schemaPath = new URL('../../../docs/api_schema.json', import.meta.url);

function loadSchemas() {
  const raw = readFileSync(schemaPath, 'utf8');
  const parsed = JSON.parse(raw);
  const ajv = new Ajv({ allErrors: true, strict: false, validateFormats: true });
  // schemars emits "uint" format for unsigned integers; teach Ajv how to handle it
  ajv.addFormat('uint', { type: 'number', validate: (n: number) => Number.isInteger(n) && n >= 0 });
  ajv.addSchema(parsed.AnalyzeRequest, 'AnalyzeRequest');
  ajv.addSchema(parsed.AnalyzeResult, 'AnalyzeResult');
  return ajv;
}

describe('API schema compatibility', () => {
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
          nodes: [],
          edges: [],
        },
      ],
      globalLineage: { nodes: [], edges: [] },
      issues: [],
      summary: {
        statementCount: 1,
        tableCount: 0,
        columnCount: 0,
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
});
