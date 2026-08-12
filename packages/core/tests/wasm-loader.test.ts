import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';

const wasmModuleMock = vi.hoisted(() => ({
  default: vi.fn(async () => undefined),
  analyze_sql_json: vi.fn(),
  set_panic_hook: vi.fn(),
  __wbindgen_free: vi.fn(),
}));

vi.mock('../src/wasm/flowscope_wasm', () => wasmModuleMock);

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
      (
        wasmModuleMock as unknown as { analyze_sql_json: ReturnType<typeof vi.fn> }
      ).analyze_sql_json = vi.fn();
    }
    wasmModuleMock.analyze_sql_json.mockImplementation(() => JSON.stringify({}));
    wasmModuleMock.set_panic_hook.mockClear();
    wasmModuleMock.__wbindgen_free.mockClear();
  });

  afterEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  it('shares one initialization attempt across concurrent callers', async () => {
    const loader = await loadLoader();

    const initA = loader.initWasm();
    const initB = loader.initWasm();
    const [moduleA, moduleB] = await Promise.all([initA, initB]);

    expect(initA).toBe(initB);
    expect(moduleA).toBe(moduleB);
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(1);
    expect(wasmModuleMock.set_panic_hook).toHaveBeenCalledTimes(1);
    expect(loader.isWasmInitialized()).toBe(true);
  });

  it('returns the cached module without initializing again', async () => {
    const loader = await loadLoader();

    const moduleA = await loader.initWasm();
    const moduleB = await loader.initWasm();

    expect(moduleA).toBe(moduleB);
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(1);
    expect(wasmModuleMock.set_panic_hook).toHaveBeenCalledTimes(1);
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

  it('allows a later call to retry after a transient failure', async () => {
    wasmModuleMock.default.mockRejectedValueOnce(new Error('transient load failure'));
    const loader = await loadLoader();

    await expect(loader.initWasm()).rejects.toThrow(/transient load failure/);
    expect(loader.isWasmInitialized()).toBe(false);

    await expect(loader.initWasm()).resolves.toBeDefined();
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(2);
    expect(loader.isWasmInitialized()).toBe(true);
  });

  it('allows reinitialization after reset', async () => {
    const loader = await loadLoader();

    await loader.initWasm();
    loader.resetWasm();
    loader.resetWasm();

    expect(loader.isWasmInitialized()).toBe(false);
    await expect(loader.initWasm()).resolves.toBeDefined();
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(2);
    expect(wasmModuleMock.set_panic_hook).toHaveBeenCalledTimes(2);
    expect(loader.isWasmInitialized()).toBe(true);
  });

  it('does not let an in-flight initialization undo reset', async () => {
    let resolveFirst!: () => void;
    wasmModuleMock.default.mockImplementationOnce(
      () =>
        new Promise<undefined>((resolve) => {
          resolveFirst = () => resolve(undefined);
        })
    );
    const loader = await loadLoader();

    const firstInit = loader.initWasm();
    await vi.waitFor(() => expect(wasmModuleMock.default).toHaveBeenCalledTimes(1));

    loader.resetWasm();
    resolveFirst();

    await expect(firstInit).rejects.toThrow(/superseded by reset or cleanup/);
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(1);
    expect(loader.isWasmInitialized()).toBe(false);

    await expect(loader.initWasm()).resolves.toBeDefined();
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(2);
    expect(loader.isWasmInitialized()).toBe(true);
  });

  it('does not let a superseded failure clear a newer initialization', async () => {
    let rejectFirst!: (reason: Error) => void;
    wasmModuleMock.default.mockImplementationOnce(
      () =>
        new Promise<undefined>((_resolve, reject) => {
          rejectFirst = reject;
        })
    );
    const loader = await loadLoader();

    const firstInit = loader.initWasm();
    const firstResult = firstInit.catch((error: unknown) => error);
    await vi.waitFor(() => expect(wasmModuleMock.default).toHaveBeenCalledTimes(1));

    loader.resetWasm();
    await expect(loader.initWasm()).resolves.toBeDefined();
    rejectFirst(new Error('superseded load failure'));

    await expect(firstResult).resolves.toEqual(
      expect.objectContaining({ message: expect.stringMatching(/superseded load failure/) })
    );
    await expect(loader.initWasm()).resolves.toBeDefined();
    expect(wasmModuleMock.default).toHaveBeenCalledTimes(2);
    expect(loader.isWasmInitialized()).toBe(true);
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
