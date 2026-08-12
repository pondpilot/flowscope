import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import type { AnalyzeResult } from '../src/types';

const baseResult: AnalyzeResult = {
  statements: [],
  nodes: [],
  edges: [],
  issues: [],
  summary: {
    statementCount: 0,
    tableCount: 0,
    columnCount: 0,
    joinCount: 0,
    complexityScore: 0,
    issueCount: { errors: 0, warnings: 0, infos: 0 },
    hasErrors: false,
  },
};

const wasmModuleMock = vi.hoisted(() => ({
  default: vi.fn(async () => undefined),
  analyze_sql_json: vi.fn<(request: string) => string>(() => JSON.stringify(baseResult)),
  export_to_duckdb_sql: vi.fn(() => '-- DuckDB SQL export'),
  export_json: vi.fn(() => JSON.stringify(baseResult)),
  export_mermaid: vi.fn(() => 'graph TD'),
  export_html: vi.fn(() => '<html></html>'),
  export_csv_bundle: vi.fn(() => new Uint8Array()),
  export_xlsx: vi.fn(() => new Uint8Array()),
  export_filename: vi.fn(() => 'flowscope_export'),
  completion_items_json: vi.fn(() => JSON.stringify({ clause: 'unknown', items: [] })),
  split_statements_json: vi.fn(() => JSON.stringify({ statements: [] })),
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
    const payloadJson = wasmModuleMock.analyze_sql_json.mock.calls[0]?.[0];
    expect(payloadJson).toBeDefined();
    const payload = JSON.parse(payloadJson as string);
    expect(payload.sql).toBe('SELECT 1');
    expect(payload.dialect).toBe('generic');
  });

  it('retries initialization for analysis, completion, and export after a transient failure', async () => {
    wasmModuleMock.default.mockRejectedValueOnce(new Error('transient load failure'));
    const { analyzeSql, completionItems, exportJson } = await loadAnalyzer();

    await expect(analyzeSql({ sql: 'SELECT 1', dialect: 'generic' })).rejects.toThrow(
      /transient load failure/
    );

    const [analysis, completions, exported] = await Promise.all([
      analyzeSql({ sql: 'SELECT 1', dialect: 'generic' }),
      completionItems({ sql: 'SELECT ', dialect: 'generic', cursorOffset: 7 }),
      exportJson(baseResult),
    ]);

    expect(analysis.summary.hasErrors).toBe(false);
    expect(completions.items).toEqual([]);
    expect(JSON.parse(exported)).toEqual(baseResult);
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(2);
  });

  it('uses current wasm exports after reset instead of cached function references', async () => {
    const originalAnalyze = wasmModuleMock.analyze_sql_json;
    const originalExportJson = wasmModuleMock.export_json;
    const replacementAnalyze = vi.fn(() =>
      JSON.stringify({ ...baseResult, summary: { ...baseResult.summary, tableCount: 2 } })
    );
    const replacementExportJson = vi.fn(() => '{"lifecycle":"replacement"}');
    const { analyzeSql, exportJson } = await loadAnalyzer();
    const { resetWasm } = await import('../src/wasm-loader');

    try {
      await analyzeSql({ sql: 'SELECT 1', dialect: 'generic' });
      await exportJson(baseResult);
      resetWasm();
      wasmModuleMock.analyze_sql_json = replacementAnalyze;
      wasmModuleMock.export_json = replacementExportJson;

      const analysis = await analyzeSql({ sql: 'SELECT 2', dialect: 'generic' });
      const exported = await exportJson(baseResult);

      expect(analysis.summary.tableCount).toBe(2);
      expect(exported).toBe('{"lifecycle":"replacement"}');
      expect(replacementAnalyze).toHaveBeenCalledTimes(1);
      expect(replacementExportJson).toHaveBeenCalledTimes(1);
      expect(wasmModuleMock.default).toHaveBeenCalledTimes(2);
    } finally {
      wasmModuleMock.analyze_sql_json = originalAnalyze;
      wasmModuleMock.export_json = originalExportJson;
    }
  });

  it('validates input SQL and throws for empty strings', async () => {
    const { analyzeSql } = await loadAnalyzer();
    await expect(analyzeSql({ sql: '', dialect: 'generic' })).rejects.toThrow(
      /sql must be a non-empty string/
    );
  });

  it('rejects invalid dialect values', async () => {
    const { analyzeSql } = await loadAnalyzer();
    await expect(analyzeSql({ sql: 'SELECT 1', dialect: 'unsupported' as never })).rejects.toThrow(
      /Invalid dialect: unsupported/
    );
    expect(wasmModuleMock.analyze_sql_json).not.toHaveBeenCalled();
  });

  it('accepts Oracle dialect requests', async () => {
    const { analyzeSql } = await loadAnalyzer();

    await analyzeSql({ sql: 'SELECT 1 FROM dual', dialect: 'oracle' });

    const payloadJson = wasmModuleMock.analyze_sql_json.mock.calls[0]?.[0];
    expect(payloadJson).toBeDefined();
    const payload = JSON.parse(payloadJson as string);
    expect(payload.dialect).toBe('oracle');
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
    const payloadJson = wasmModuleMock.analyze_sql_json.mock.calls[0]?.[0];
    expect(payloadJson).toBeDefined();
    const payload = JSON.parse(payloadJson as string);
    expect(payload.dialect).toBe('generic');
  });
});
