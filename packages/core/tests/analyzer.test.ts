import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import type { AnalyzeResult } from '../src/types';

const baseResult: AnalyzeResult = {
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

const wasmModuleMock = vi.hoisted(() => ({
  default: vi.fn(async () => undefined),
  analyze_sql_json: vi.fn(() => JSON.stringify(baseResult)),
  set_panic_hook: vi.fn(() => undefined),
}));

vi.mock('../src/wasm/flowscope_wasm', () => wasmModuleMock);

async function loadAnalyzer() {
  return import('../src/analyzer');
}

describe('analyzer', () => {
  beforeEach(() => {
    wasmModuleMock.default.mockClear();
    wasmModuleMock.default.mockImplementation(async () => undefined);
    wasmModuleMock.analyze_sql_json.mockClear();
    wasmModuleMock.analyze_sql_json.mockImplementation(() => JSON.stringify(baseResult));
    wasmModuleMock.set_panic_hook.mockClear();
    wasmModuleMock.set_panic_hook.mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  it('calls into wasm and returns typed results', async () => {
    const { analyzeSql } = await loadAnalyzer();

    const result = await analyzeSql({ sql: 'SELECT 1', dialect: 'generic' });

    expect(result.summary.hasErrors).toBe(false);
    expect(wasmModuleMock.analyze_sql_json).toHaveBeenCalledTimes(1);
    const payload = JSON.parse(wasmModuleMock.analyze_sql_json.mock.calls[0][0]);
    expect(payload.sql).toBe('SELECT 1');
    expect(payload.dialect).toBe('generic');
  });

  it('validates input SQL and throws for empty strings', async () => {
    const { analyzeSql } = await loadAnalyzer();
    await expect(analyzeSql({ sql: '', dialect: 'generic' })).rejects.toThrow(
      /sql must be a non-empty string/
    );
  });

  it('validates dialect values', async () => {
    const { analyzeSql } = await loadAnalyzer();
    await expect(
      analyzeSql({ sql: 'SELECT 1', dialect: 'oracle' as never })
    ).rejects.toThrow(/Invalid dialect/);
  });

  it('throws when wasm returns malformed JSON', async () => {
    wasmModuleMock.analyze_sql_json.mockImplementation(() => 'not-json');
    const { analyzeSql } = await loadAnalyzer();
    await expect(analyzeSql({ sql: 'SELECT 1', dialect: 'generic' })).rejects.toThrow(
      /Failed to parse analysis result/
    );
  });

  it('exposes analyzeSimple helper with default dialect', async () => {
    const { analyzeSimple } = await loadAnalyzer();

    await analyzeSimple('SELECT 1');
    const payload = JSON.parse(wasmModuleMock.analyze_sql_json.mock.calls[0][0]);
    expect(payload.dialect).toBe('generic');
  });
});
