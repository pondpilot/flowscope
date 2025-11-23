import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';

const wasmModuleMock = vi.hoisted(() => ({
  default: vi.fn(async () => undefined),
  analyze_sql_json: vi.fn(),
  __wbindgen_free: vi.fn(),
}));

vi.mock('../wasm/flowscope_wasm', () => wasmModuleMock, { virtual: true });

async function loadLoader() {
  return import('../src/wasm-loader');
}

describe('wasm-loader', () => {
  beforeEach(() => {
    wasmModuleMock.default.mockClear();
    wasmModuleMock.default.mockImplementation(async () => undefined);
    if (typeof wasmModuleMock.analyze_sql_json === 'function') {
      wasmModuleMock.analyze_sql_json.mockClear();
    } else {
      (wasmModuleMock as unknown as { analyze_sql_json: ReturnType<typeof vi.fn> }).analyze_sql_json =
        vi.fn();
    }
    wasmModuleMock.analyze_sql_json.mockImplementation(() => JSON.stringify({}));
    wasmModuleMock.__wbindgen_free.mockClear();
  });

  afterEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  it('initializes wasm exactly once and caches the module', async () => {
    const loader = await loadLoader();

    const moduleA = await loader.initWasm();
    const moduleB = await loader.initWasm();

    expect(moduleA).toBe(moduleB);
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(1);
    expect(loader.isWasmInitialized()).toBe(true);
  });

  it('forwards wasmUrl option to the wasm initializer', async () => {
    const loader = await loadLoader();

    await loader.initWasm({ wasmUrl: '/custom/flowscope.wasm' });

    expect(wasmModuleMock.default).toHaveBeenCalledWith('/custom/flowscope.wasm');
  });

  it('throws if analyze_sql_json is missing on the wasm exports', async () => {
    const originalAnalyze = wasmModuleMock.analyze_sql_json;
    (wasmModuleMock as unknown as { analyze_sql_json?: undefined }).analyze_sql_json = undefined;
    const loader = await loadLoader();

    await expect(loader.initWasm()).rejects.toThrow(/analyze_sql_json function is not available/);
    expect(loader.isWasmInitialized()).toBe(false);

    (wasmModuleMock as unknown as { analyze_sql_json: typeof originalAnalyze }).analyze_sql_json =
      originalAnalyze;
  });

  it('resetWasm allows reinitialization after failure', async () => {
    const originalAnalyze = wasmModuleMock.analyze_sql_json;
    (wasmModuleMock as unknown as { analyze_sql_json?: undefined }).analyze_sql_json = undefined;
    const loader = await loadLoader();

    await expect(loader.initWasm()).rejects.toThrow();
    loader.resetWasm();

    vi.resetModules();
    vi.clearAllMocks();
    (wasmModuleMock as unknown as { analyze_sql_json: typeof originalAnalyze }).analyze_sql_json =
      originalAnalyze;
    wasmModuleMock.default.mockClear();
    const reloaded = await loadLoader();
    await expect(reloaded.initWasm()).resolves.toBeDefined();
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(1);
  });

  it('cleanupWasm frees resources and clears initialization state', async () => {
    const loader = await loadLoader();

    await loader.initWasm();
    await loader.cleanupWasm();

    expect(wasmModuleMock.__wbindgen_free).toHaveBeenCalledTimes(1);
    expect(loader.isWasmInitialized()).toBe(false);
  });

  it('getWasmModule throws when called before init', async () => {
    const loader = await loadLoader();
    expect(() => loader.getWasmModule()).toThrow(/not initialized/);
  });
});
