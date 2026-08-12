import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BackendAdapter } from '@/lib/backend-adapter';
import { buildAnalysisCacheKey } from '@/lib/analysis-hash';
import { PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS } from '@/lib/analysis-cache-policy';
import { useAnalysisStore } from '@/lib/analysis-store';
import type { Project } from '@/lib/project-store';
import { AnalysisError, AnalysisErrorCode } from '@/types';

const lineageActions = vi.hoisted(() => ({
  setResult: vi.fn(),
  setAnalyzedContent: vi.fn(),
  setStalePaths: vi.fn(),
  setSql: vi.fn(),
}));

let currentProject: Project | null = null;
let activeProjectId: string | null = null;
let hideCTEs = false;
let backendSchema: unknown = null;
let showLintIssues = false;

vi.mock('@flowscope-react/store', () => ({
  useLineageState: (selector: (state: { hideCTEs: boolean }) => unknown) => selector({ hideCTEs }),
  useLineageActions: () => lineageActions,
}));

vi.mock('@/lib/project-store', () => ({
  useProject: () => ({ currentProject, activeProjectId, backendSchema }),
}));

vi.mock('@/lib/view-state-store', () => ({
  useViewStateStore: (
    selector: (state: {
      viewStates: Record<string, { issues: { showLintIssues: boolean } }>;
    }) => unknown
  ) =>
    selector({
      viewStates: activeProjectId ? { [activeProjectId]: { issues: { showLintIssues } } } : {},
    }),
  getIssuesStateWithDefaults: (issues?: { showLintIssues: boolean }) => ({
    showLintIssues: issues?.showLintIssues ?? false,
  }),
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
    hideCTEs = false;
    backendSchema = null;
    showLintIssues = false;
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

  it('batches rapid edits and project switches before proactive file sync', async () => {
    vi.useFakeTimers();
    currentProject = createProject('project-a', 'SELECT 1');
    activeProjectId = currentProject.id;
    const adapter = createAdapter();
    const { rerender } = renderHook(() => useAnalysis(true, { adapter }));

    await act(async () => {
      await Promise.resolve();
    });
    expect(adapter.syncFiles).toHaveBeenCalledTimes(1);
    vi.mocked(adapter.syncFiles).mockClear();

    act(() => {
      currentProject = createProject('project-a', 'SELECT 12');
      rerender();
      currentProject = createProject('project-b', 'SELECT 123');
      activeProjectId = currentProject.id;
      rerender();
      currentProject = createProject('project-c', 'SELECT 1234');
      activeProjectId = currentProject.id;
      rerender();
    });

    await vi.advanceTimersByTimeAsync(299);
    expect(adapter.syncFiles).not.toHaveBeenCalled();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(adapter.syncFiles).toHaveBeenCalledTimes(1);
    expect(adapter.syncFiles).toHaveBeenCalledWith([{ name: 'model.sql', content: 'SELECT 1234' }]);
  });

  it('discards an analysis result when project inputs change in flight', async () => {
    const projectBeforeEdit = createProject('project-1', 'SELECT 1');
    const cacheKey = buildProjectCacheKey(projectBeforeEdit);
    currentProject = projectBeforeEdit;
    activeProjectId = currentProject.id;

    let resolveAnalysis!: (value: {
      result: AnalyzeResult;
      cacheKey: string;
      cacheHit: boolean;
      skipped: boolean;
      timings: null;
    }) => void;
    const pendingAnalysis = new Promise<Parameters<typeof resolveAnalysis>[0]>((resolve) => {
      resolveAnalysis = resolve;
    });
    const adapter = createAdapter(vi.fn(() => pendingAnalysis));
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));
    lineageActions.setResult.mockClear();

    let runPromise!: Promise<void>;
    await act(async () => {
      runPromise = result.current.runAnalysis();
      await Promise.resolve();
    });

    act(() => {
      currentProject = createProject('project-1', 'SELECT 2');
      rerender();
    });

    await act(async () => {
      resolveAnalysis({
        result: cachedResult,
        cacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      });
      await runPromise;
    });

    expect(lineageActions.setResult).not.toHaveBeenCalledWith(cachedResult);
    expect(result.current.isAnalyzing).toBe(false);
  });

  it('stops before analysis when inputs change during the scheduling frame', async () => {
    let resumeAnalysis!: FrameRequestCallback;
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      resumeAnalysis = callback;
      return 1;
    });
    currentProject = createProject('project-1', 'SELECT 1');
    activeProjectId = currentProject.id;
    const adapter = createAdapter(vi.fn());
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));

    let runPromise!: Promise<void>;
    act(() => {
      runPromise = result.current.runAnalysis();
    });
    expect(result.current.isAnalyzing).toBe(true);

    act(() => {
      currentProject = createProject('project-1', 'SELECT 2');
      rerender();
      resumeAnalysis(0);
    });
    await act(async () => runPromise);

    expect(adapter.analyze).not.toHaveBeenCalled();
    expect(result.current.isAnalyzing).toBe(false);
  });

  it('does not cancel a run for a project metadata-only update', async () => {
    let resumeAnalysis!: FrameRequestCallback;
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      resumeAnalysis = callback;
      return 1;
    });
    currentProject = createProject('project-1', 'SELECT 1');
    activeProjectId = currentProject.id;
    const cacheKey = buildProjectCacheKey(currentProject);
    const adapter = createAdapter(
      vi.fn().mockResolvedValue({
        result: cachedResult,
        cacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      })
    );
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));

    let runPromise!: Promise<void>;
    act(() => {
      runPromise = result.current.runAnalysis();
    });
    act(() => {
      currentProject = { ...currentProject!, name: 'Renamed project' };
      rerender();
      resumeAnalysis(0);
    });
    await act(async () => runPromise);

    expect(adapter.analyze).toHaveBeenCalledTimes(1);
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
  });

  it('supports an active-file-only run without changing the project run mode', async () => {
    currentProject = createProject('project-1', 'SELECT 1');
    currentProject.files.push({
      id: 'project-1-other-file',
      name: 'other.sql',
      path: 'other.sql',
      content: 'SELECT 2',
      language: 'sql',
    });
    activeProjectId = currentProject.id;
    const adapter = createAdapter(
      vi.fn().mockImplementation(async (payload: { files: Array<{ name: string }> }) => ({
        result: cachedResult,
        cacheKey: buildAnalysisCacheKey({
          files: payload.files.map((file) => ({ ...file, content: 'SELECT 1' })),
          dialect: 'generic',
          schemaSQL: '',
          hideCTEs: false,
          enableColumnLineage: true,
          enableLinting: false,
          templateMode: 'raw',
        }),
        cacheHit: false,
        skipped: false,
        timings: null,
      }))
    );
    const { result } = renderHook(() => useAnalysis(true, { adapter }));

    await act(async () => {
      await result.current.runAnalysis('SELECT 1', 'model.sql', 'current');
    });

    expect(currentProject.runMode).toBe('all');
    expect(adapter.analyze).toHaveBeenCalledWith(
      expect.objectContaining({ files: [{ name: 'model.sql', content: 'SELECT 1' }] }),
      { knownCacheKey: null }
    );
  });

  it('does not let a pending proactive restore overwrite an explicit active-file run', async () => {
    const persistentResult = { ...cachedResult, issues: [{ code: 'PERSISTED' }] } as AnalyzeResult;
    const activeResult = { ...cachedResult, issues: [{ code: 'ACTIVE' }] } as AnalyzeResult;
    currentProject = createProject('project-1', 'SELECT 1');
    currentProject.files.push({
      id: 'project-1-other-file',
      name: 'other.sql',
      path: 'other.sql',
      content: 'SELECT 2',
      language: 'sql',
    });
    activeProjectId = currentProject.id;
    const allFilesCacheKey = buildAnalysisCacheKey({
      files: currentProject.files.map((file) => ({ name: file.path, content: file.content })),
      dialect: 'generic',
      schemaSQL: '',
      hideCTEs: false,
      enableColumnLineage: true,
      enableLinting: false,
      templateMode: 'raw',
    });
    const activeFileCacheKey = buildAnalysisCacheKey({
      files: [{ name: 'model.sql', content: 'SELECT 1' }],
      dialect: 'generic',
      schemaSQL: '',
      hideCTEs: false,
      enableColumnLineage: true,
      enableLinting: false,
      templateMode: 'raw',
    });
    let resolveRestore!: (value: {
      result: AnalyzeResult;
      cacheKey: string;
      cacheHit: boolean;
      skipped: boolean;
      timings: null;
    }) => void;
    const pendingRestore = new Promise<Parameters<typeof resolveRestore>[0]>((resolve) => {
      resolveRestore = resolve;
    });
    const adapter = createAdapter(
      vi.fn().mockResolvedValue({
        result: activeResult,
        cacheKey: activeFileCacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      })
    );
    adapter.getCached = vi.fn(() => pendingRestore);
    const { result } = renderHook(() => useAnalysis(true, { adapter }));
    await act(async () => Promise.resolve());
    expect(adapter.getCached).toHaveBeenCalledTimes(1);
    lineageActions.setResult.mockClear();

    await act(async () => {
      await result.current.runAnalysis('SELECT 1', 'model.sql', 'current');
    });
    await act(async () => {
      resolveRestore({
        result: persistentResult,
        cacheKey: allFilesCacheKey,
        cacheHit: true,
        skipped: false,
        timings: null,
      });
      await pendingRestore;
    });

    expect(lineageActions.setResult).toHaveBeenCalledWith(activeResult);
    expect(lineageActions.setResult).not.toHaveBeenCalledWith(persistentResult);
  });

  it('invalidates an active-file override when that unselected file changes', async () => {
    const staleResult = { ...cachedResult, issues: [{ code: 'STALE' }] } as AnalyzeResult;
    currentProject = createProject('project-1', 'SELECT 1');
    const activeFile = currentProject.files[0];
    const selectedFile = {
      id: 'project-1-selected-file',
      name: 'selected.sql',
      path: 'selected.sql',
      content: 'SELECT 2',
      language: 'sql' as const,
    };
    currentProject.files.push(selectedFile);
    currentProject.runMode = 'custom';
    currentProject.selectedFileIds = [selectedFile.id];
    activeProjectId = currentProject.id;
    const activeFileCacheKey = buildAnalysisCacheKey({
      files: [{ name: activeFile.path, content: activeFile.content }],
      dialect: 'generic',
      schemaSQL: '',
      hideCTEs: false,
      enableColumnLineage: true,
      enableLinting: false,
      templateMode: 'raw',
    });
    let resolveAnalysis!: (value: {
      result: AnalyzeResult;
      cacheKey: string;
      cacheHit: boolean;
      skipped: boolean;
      timings: null;
    }) => void;
    const adapter = createAdapter(
      vi.fn(
        () =>
          new Promise<Parameters<typeof resolveAnalysis>[0]>((resolve) => {
            resolveAnalysis = resolve;
          })
      )
    );
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));
    lineageActions.setResult.mockClear();

    let runPromise!: Promise<void>;
    await act(async () => {
      runPromise = result.current.runAnalysis(activeFile.content, activeFile.path, 'current');
      await Promise.resolve();
    });
    act(() => {
      currentProject = {
        ...currentProject!,
        files: [{ ...activeFile, content: 'SELECT 10' }, selectedFile],
      };
      rerender();
    });
    await act(async () => {
      resolveAnalysis({
        result: staleResult,
        cacheKey: activeFileCacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      });
      await runPromise;
    });

    expect(lineageActions.setResult).not.toHaveBeenCalledWith(staleResult);
    expect(result.current.isAnalyzing).toBe(false);
  });

  it('keeps an explicit result authoritative across metadata-only project updates', async () => {
    const persistentResult = { ...cachedResult, issues: [{ code: 'PERSISTED' }] } as AnalyzeResult;
    const activeResult = { ...cachedResult, issues: [{ code: 'ACTIVE' }] } as AnalyzeResult;
    currentProject = createProject('project-1', 'SELECT 1');
    currentProject.files.push({
      id: 'project-1-other-file',
      name: 'other.sql',
      path: 'other.sql',
      content: 'SELECT 2',
      language: 'sql',
    });
    activeProjectId = currentProject.id;
    const allFilesCacheKey = buildAnalysisCacheKey({
      files: currentProject.files.map((file) => ({ name: file.path, content: file.content })),
      dialect: 'generic',
      schemaSQL: '',
      hideCTEs: false,
      enableColumnLineage: true,
      enableLinting: false,
      templateMode: 'raw',
    });
    const activeFileCacheKey = buildAnalysisCacheKey({
      files: [{ name: 'model.sql', content: 'SELECT 1' }],
      dialect: 'generic',
      schemaSQL: '',
      hideCTEs: false,
      enableColumnLineage: true,
      enableLinting: false,
      templateMode: 'raw',
    });
    const adapter = createAdapter(
      vi.fn().mockResolvedValue({
        result: activeResult,
        cacheKey: activeFileCacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      })
    );
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));
    await act(async () => Promise.resolve());

    await act(async () => {
      await result.current.runAnalysis('SELECT 1', 'model.sql', 'current');
    });
    lineageActions.setResult.mockClear();
    vi.mocked(adapter.getCached).mockClear();
    vi.mocked(adapter.getCached).mockResolvedValue({
      result: persistentResult,
      cacheKey: allFilesCacheKey,
      cacheHit: true,
      skipped: false,
      timings: null,
    });

    act(() => {
      currentProject = { ...currentProject!, name: 'Renamed project' };
      rerender();
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 350));
    });

    expect(adapter.getCached).not.toHaveBeenCalled();
    expect(lineageActions.setResult).not.toHaveBeenCalledWith(persistentResult);
  });

  it('keeps a custom-mode run alive when an unselected file changes', async () => {
    let resumeAnalysis!: FrameRequestCallback;
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      resumeAnalysis = callback;
      return 1;
    });
    currentProject = createProject('project-1', 'SELECT 1');
    const selectedFile = currentProject.files[0];
    currentProject.files.push({
      id: 'project-1-unselected-file',
      name: 'unselected.sql',
      path: 'unselected.sql',
      content: 'SELECT 2',
      language: 'sql',
    });
    currentProject.runMode = 'custom';
    currentProject.selectedFileIds = [selectedFile.id];
    activeProjectId = currentProject.id;
    const cacheKey = buildAnalysisCacheKey({
      files: [{ name: selectedFile.path, content: selectedFile.content }],
      dialect: 'generic',
      schemaSQL: '',
      hideCTEs: false,
      enableColumnLineage: true,
      enableLinting: false,
      templateMode: 'raw',
    });
    const adapter = createAdapter(
      vi.fn().mockResolvedValue({
        result: cachedResult,
        cacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      })
    );
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));

    let runPromise!: Promise<void>;
    act(() => {
      runPromise = result.current.runAnalysis();
    });
    act(() => {
      currentProject = {
        ...currentProject!,
        files: [selectedFile, { ...currentProject!.files[1], content: 'SELECT 20' }],
      };
      rerender();
      resumeAnalysis(0);
    });
    await act(async () => runPromise);

    expect(adapter.analyze).toHaveBeenCalledTimes(1);
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
  });

  it('discards an analysis result when analysis options change in flight', async () => {
    currentProject = createProject('project-1', 'SELECT 1');
    activeProjectId = currentProject.id;
    const cacheKey = buildProjectCacheKey(currentProject);

    let resolveAnalysis!: (value: {
      result: AnalyzeResult;
      cacheKey: string;
      cacheHit: boolean;
      skipped: boolean;
      timings: null;
    }) => void;
    const adapter = createAdapter(
      vi.fn(
        () =>
          new Promise<Parameters<typeof resolveAnalysis>[0]>((resolve) => {
            resolveAnalysis = resolve;
          })
      )
    );
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));
    lineageActions.setResult.mockClear();

    let runPromise!: Promise<void>;
    await act(async () => {
      runPromise = result.current.runAnalysis();
      await Promise.resolve();
    });
    act(() => {
      hideCTEs = true;
      rerender();
    });
    await act(async () => {
      resolveAnalysis({
        result: cachedResult,
        cacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      });
      await runPromise;
    });

    expect(lineageActions.setResult).not.toHaveBeenCalledWith(cachedResult);
    expect(result.current.isAnalyzing).toBe(false);
  });

  it('discards an analysis result when the backend changes in flight', async () => {
    currentProject = createProject('project-1', 'SELECT 1');
    activeProjectId = currentProject.id;
    const cacheKey = buildProjectCacheKey(currentProject);

    let resolveAnalysis!: (value: {
      result: AnalyzeResult;
      cacheKey: string;
      cacheHit: boolean;
      skipped: boolean;
      timings: null;
    }) => void;
    const oldAdapter = createAdapter(
      vi.fn(
        () =>
          new Promise<Parameters<typeof resolveAnalysis>[0]>((resolve) => {
            resolveAnalysis = resolve;
          })
      )
    );
    let adapter = oldAdapter;
    const { result, rerender } = renderHook(() => useAnalysis(true, { adapter }));
    lineageActions.setResult.mockClear();

    let runPromise!: Promise<void>;
    await act(async () => {
      runPromise = result.current.runAnalysis();
      await Promise.resolve();
    });
    act(() => {
      adapter = createAdapter(vi.fn(), 'rest');
      backendSchema = { tables: [] };
      rerender();
    });
    await act(async () => {
      resolveAnalysis({
        result: cachedResult,
        cacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      });
      await runPromise;
    });

    expect(lineageActions.setResult).not.toHaveBeenCalledWith(cachedResult);
    expect(result.current.isAnalyzing).toBe(false);
  });

  it('forces a full file replacement before retrying missing worker content', async () => {
    currentProject = createProject('project-1', 'SELECT 1');
    activeProjectId = currentProject.id;
    const cacheKey = buildProjectCacheKey(currentProject);
    const analyze = vi
      .fn()
      .mockRejectedValueOnce(
        new AnalysisError(AnalysisErrorCode.MISSING_FILE_CONTENT, 'missing query.sql')
      )
      .mockResolvedValueOnce({
        result: cachedResult,
        cacheKey,
        cacheHit: false,
        skipped: false,
        timings: null,
      });
    const adapter = createAdapter(analyze);
    const { result } = renderHook(() => useAnalysis(true, { adapter }));
    vi.mocked(adapter.syncFiles).mockClear();

    await act(async () => result.current.runAnalysis());

    expect(analyze).toHaveBeenCalledTimes(2);
    expect(adapter.syncFiles).toHaveBeenCalledWith([{ name: 'model.sql', content: 'SELECT 1' }], {
      forceReplace: true,
    });
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
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
    expect(adapter.syncFiles).not.toHaveBeenCalled();
    expect(lineageActions.setResult).toHaveBeenCalledWith(cachedResult);
  });
});
