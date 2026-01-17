import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import type { AnalyzeResult } from '../src/types';

const sampleSql = `-- FlowScope Export
CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
INSERT INTO _meta (key, value) VALUES ('version', '0.1.0');`;

const baseResult: AnalyzeResult = {
  statements: [{
    statementIndex: 0,
    statementType: 'SELECT',
    nodes: [],
    edges: [],
    joinCount: 0,
    complexityScore: 1,
  }],
  globalLineage: { nodes: [], edges: [] },
  issues: [],
  summary: {
    statementCount: 1,
    tableCount: 1,
    columnCount: 0,
    issueCount: { errors: 0, warnings: 0, infos: 0 },
    hasErrors: false,
  },
};

const wasmModuleMock = vi.hoisted(() => ({
  default: vi.fn(async () => undefined),
  analyze_sql_json: vi.fn(() => JSON.stringify(baseResult)),
  export_to_duckdb_sql: vi.fn(() => sampleSql),
  completion_items_json: vi.fn(() => JSON.stringify({ clause: 'unknown', items: [] })),
  split_statements_json: vi.fn(() => JSON.stringify({ statements: [] })),
  set_panic_hook: vi.fn(() => undefined),
}));

vi.mock('../src/wasm/flowscope_wasm', () => wasmModuleMock);

async function loadAnalyzer() {
  return import('../src/analyzer');
}

describe('exportToDuckDbSql', () => {
  beforeEach(() => {
    wasmModuleMock.default.mockClear();
    wasmModuleMock.default.mockImplementation(async () => undefined);
    wasmModuleMock.export_to_duckdb_sql.mockClear();
    wasmModuleMock.export_to_duckdb_sql.mockImplementation(() => sampleSql);
    wasmModuleMock.set_panic_hook.mockClear();
    wasmModuleMock.set_panic_hook.mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  it('exports analysis result to SQL', async () => {
    const { exportToDuckDbSql } = await loadAnalyzer();

    const sql = await exportToDuckDbSql(baseResult);

    expect(sql).toContain('CREATE TABLE _meta');
    expect(sql).toContain('INSERT INTO _meta');
    expect(wasmModuleMock.export_to_duckdb_sql).toHaveBeenCalledTimes(1);
  });

  it('passes serialized result to WASM function', async () => {
    const { exportToDuckDbSql } = await loadAnalyzer();

    await exportToDuckDbSql(baseResult, 'my_schema');

    const payload = JSON.parse(wasmModuleMock.export_to_duckdb_sql.mock.calls[0][0]);
    // Payload structure is { result: AnalyzeResult, schema?: string }
    expect(payload.result.statements).toHaveLength(1);
    expect(payload.result.summary.statementCount).toBe(1);
    expect(payload.schema).toBe('my_schema');
  });

  it('handles empty result gracefully', async () => {
    const emptyResult: AnalyzeResult = {
      statements: [],
      globalLineage: { nodes: [], edges: [] },
      issues: [],
      summary: {
        statementCount: 0,
        tableCount: 0,
        columnCount: 0,
        issueCount: { errors: 0, warnings: 0, infos: 0 },
        hasErrors: false,
      },
    };

    const { exportToDuckDbSql } = await loadAnalyzer();
    const sql = await exportToDuckDbSql(emptyResult);

    expect(sql).toBeTruthy();
    expect(wasmModuleMock.export_to_duckdb_sql).toHaveBeenCalledTimes(1);
  });

  it('validates schema name before export', async () => {
    const { exportToDuckDbSql } = await loadAnalyzer();

    // Valid schema names should work
    await expect(exportToDuckDbSql(baseResult, 'valid_schema')).resolves.toBeTruthy();
    await expect(exportToDuckDbSql(baseResult, '_private')).resolves.toBeTruthy();
    await expect(exportToDuckDbSql(baseResult, 'Schema123')).resolves.toBeTruthy();

    // Invalid schema names should throw
    await expect(exportToDuckDbSql(baseResult, '123invalid')).rejects.toThrow(
      'Invalid schema name: must start with a letter or underscore'
    );
    await expect(exportToDuckDbSql(baseResult, 'invalid-schema')).rejects.toThrow(
      'Invalid schema name: can only contain letters, numbers, and underscores'
    );
    await expect(exportToDuckDbSql(baseResult, 'a'.repeat(64))).rejects.toThrow(
      'Invalid schema name: must be 63 characters or fewer'
    );
  });
});
