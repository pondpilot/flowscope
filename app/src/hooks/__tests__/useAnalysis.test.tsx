import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BackendAdapter } from '@/lib/backend-adapter';
import { buildAnalysisCacheKey } from '@/lib/analysis-hash';
import { PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS } from '@/lib/analysis-cache-policy';
import { useAnalysisStore } from '@/lib/analysis-store';
import type { Project } from '@/lib/project-store';

const lineageActions = vi.hoisted(() => ({
  setResult: vi.fn(),
  setAnalyzedContent: vi.fn(),
  setStalePaths: vi.fn(),
  setSql: vi.fn(),
}));

let currentProject: Project | null = null;
let activeProjectId: string | null = null;

vi.mock('@pondpilot/flowscope-react', () => ({
  useLineage: () => ({
    state: { hideCTEs: false },
    actions: lineageActions,
  }),
}));

vi.mock('@/lib/project-store', () => ({
  useProject: () => ({ currentProject, activeProjectId, backendSchema: null }),
}));

vi.mock('@/lib/view-state-store', () => ({
  useViewStateStore: (selector: (state: { getViewState: () => undefined }) => unknown) =>
    selector({ getViewState: () => undefined }),
  getIssuesStateWithDefaults: () => ({ showLintIssues: false }),
}));

import { useAnalysis } from '../useAnalysis';

const cachedResult = {
  nodes: [],
  edges: [],
  statements: [],
  issues: [],
} as unknown as AnalyzeResult;

function createProject(id: string, content: string): Project {
  return {
    id,
    name: id,
    files: [
      {
        id: `${id}-file`,
        name: 'model.sql',
        path: 'model.sql',
        content,
        language: 'sql',
      },
    ],
    activeFileId: `${id}-file`,
    dialect: 'generic',
    runMode: 'all',
    selectedFileIds: [],
    schemaSQL: '',
    templateMode: 'raw',
  };
}

function buildProjectCacheKey(project: Project): string {
  const file = project.files[0];
  return buildAnalysisCacheKey({
    files: [{ name: file.path, content: file.content }],
    dialect: project.dialect,
    schemaSQL: project.schemaSQL,
    hideCTEs: false,
    enableColumnLineage: true,
    enableLinting: false,
    templateMode: project.templateMode,
  });
}

function createAdapter(analyze = vi.fn(), type: BackendAdapter['type'] = 'wasm'): BackendAdapter {
  return {
    type,
    initialize: vi.fn(),
    analyze,
    getCached: vi.fn().mockResolvedValue(null),
    getVersion: vi.fn().mockResolvedValue(null),
    syncFiles: vi.fn().mockResolvedValue(undefined),
    clearCache: vi.fn(),
  } as unknown as BackendAdapter;
}

describe('useAnalysis memory cache', () => {
  beforeEach(() => {
    activeProjectId = 'project-1';
    currentProject = createProject(
      activeProjectId,
      'x'.repeat(PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS + 1)
    );
    useAnalysisStore.getState().clearAllResults();
    vi.clearAllMocks();
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('displays an exact memory hit before accepting a skipped worker response', async () => {
    const file = currentProject!.files[0];
    const cacheKey = buildProjectCacheKey(currentProject!);
    useAnalysisStore.getState().setResult(activeProjectId!, cacheKey, cachedResult);

    const analyze = vi.fn().mockResolvedValue({
      result: null,
      cacheKey,
      cacheHit: true,
      skipped: true,
      timings: null,
    });
    const adapter = createAdapter(analyze);
    const { result } = renderHook(() => useAnalysis(true, { adapter }));
    lineageActions.setResult.mockClear();

    await act(async () => {
      await result.current.runAnalysis();
    });

    expect(analyze).toHaveBeenCalledWith(expect.any(Object), { knownCacheKey: cacheKey });
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
    expect(lineageActions.setAnalyzedContent).toHaveBeenCalledWith(
      new Map([['model.sql', file.content]])
    );
    expect(lineageActions.setStalePaths).toHaveBeenCalledWith([]);
  });

  it('records the active project while clearing during the no-key debounce window', async () => {
    vi.useFakeTimers();
    const projectA = createProject('project-a', 'SELECT 1');
    const projectB = createProject('project-b', 'SELECT 2');
    const keyA = buildProjectCacheKey(projectA);
    activeProjectId = projectA.id;
    currentProject = projectA;
    useAnalysisStore.getState().setResult(projectA.id, keyA, cachedResult);

    const adapter = createAdapter();
    const { rerender } = renderHook(() => useAnalysis(true, { adapter }));
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
    lineageActions.setResult.mockClear();

    act(() => {
      activeProjectId = projectB.id;
      currentProject = projectB;
      rerender();
    });
    expect(lineageActions.setResult).toHaveBeenCalledTimes(1);
    expect(lineageActions.setResult).toHaveBeenLastCalledWith(null);
    lineageActions.setResult.mockClear();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(lineageActions.setResult).not.toHaveBeenCalled();

    act(() => {
      activeProjectId = projectA.id;
      currentProject = projectA;
      rerender();
    });
    expect(lineageActions.setResult).toHaveBeenLastCalledWith(null);
    lineageActions.setResult.mockClear();
    lineageActions.setAnalyzedContent.mockClear();
    lineageActions.setStalePaths.mockClear();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
    expect(lineageActions.setAnalyzedContent).toHaveBeenCalledWith(
      new Map([['model.sql', projectA.files[0].content]])
    );
    expect(lineageActions.setStalePaths).toHaveBeenCalledWith([]);
  });

  it('ignores an old persistent lookup that resolves during the debounce window', async () => {
    const projectA = createProject('project-1', 'SELECT 1');
    const projectB = createProject('project-1', 'SELECT 2');
    const keyA = buildProjectCacheKey(projectA);
    activeProjectId = projectA.id;
    currentProject = projectA;

    let resolveLookup!: (value: {
      result: AnalyzeResult;
      cacheKey: string;
      cacheHit: boolean;
      skipped: boolean;
      timings: null;
    }) => void;
    const lookup = new Promise<Parameters<typeof resolveLookup>[0]>((resolve) => {
      resolveLookup = resolve;
    });
    const adapter = createAdapter();
    adapter.getCached = vi.fn(() => lookup);
    const { rerender } = renderHook(() => useAnalysis(true, { adapter }));

    await act(async () => {
      await Promise.resolve();
    });
    expect(adapter.getCached).toHaveBeenCalledTimes(1);
    lineageActions.setResult.mockClear();

    act(() => {
      currentProject = projectB;
      rerender();
    });

    await act(async () => {
      resolveLookup({
        result: cachedResult,
        cacheKey: keyA,
        cacheHit: true,
        skipped: false,
        timings: null,
      });
      await lookup;
    });

    expect(lineageActions.setResult).not.toHaveBeenCalledWith(cachedResult);
    expect(useAnalysisStore.getState().getResult(projectA.id, keyA)).toBeNull();
  });

  it('does not build a canonical cache key for explicit REST analysis', async () => {
    const charCodeAt = vi.fn(() => {
      throw new Error('REST content was hashed');
    });
    const hashSentinel = {
      length: PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS + 1,
      charCodeAt,
    } as unknown as string;
    currentProject = createProject('project-1', hashSentinel);
    activeProjectId = currentProject.id;

    const analyze = vi.fn().mockResolvedValue({
      result: cachedResult,
      cacheKey: '',
      cacheHit: false,
      skipped: false,
      timings: null,
    });
    const adapter = createAdapter(analyze, 'rest');
    const { result } = renderHook(() => useAnalysis(true, { adapter }));

    await act(async () => {
      await result.current.runAnalysis();
    });

    expect(charCodeAt).not.toHaveBeenCalled();
    expect(analyze).toHaveBeenCalledTimes(1);
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
  });
});
