import { useState, useCallback, useEffect, useMemo, useRef, startTransition } from 'react';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { useLineage } from '@pondpilot/flowscope-react';
import { analyzeWithWorker, getCachedAnalysis, syncAnalysisFiles } from '@/lib/analysis-worker';
import type { BackendAdapter, AnalysisPayload } from '@/lib/backend-adapter';
import { useProject } from '@/lib/project-store';
import type { Project } from '@/lib/project-store';
import {
  getAnalysisCacheRestoreDecision,
  useAnalysisStore,
  type AnalysisCacheIdentity,
} from '@/lib/analysis-store';
import { buildAnalysisCacheKey } from '@/lib/analysis-hash';
import { canBuildProactiveAnalysisCacheKey } from '@/lib/analysis-cache-policy';
import { useViewStateStore, getIssuesStateWithDefaults } from '@/lib/view-state-store';
import { FILE_LIMITS, ANALYSIS_SQL_PREVIEW_LIMITS } from '@/lib/constants';
import { AnalysisErrorCode, isAnalysisError } from '@/types';
import type { AnalysisState, AnalysisContext, FileValidationResult } from '@/types';
import { useDebounce } from './useDebounce';

// Maximum retry attempts for file sync errors to prevent infinite loops
const MAX_FILE_SYNC_RETRIES = 1;
const ANALYSIS_CACHE_KEY_DEBOUNCE_MS = 300;

// Debug flag for analysis-related logging - only enabled in development
const ANALYSIS_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

// Safe time measurement function with fallback for test environments
function nowMs(): number {
  if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
    return performance.now();
  }
  return Date.now();
}

/**
 * Options for the useAnalysis hook.
 */
export interface UseAnalysisOptions {
  /** Backend adapter to use for analysis (optional, falls back to direct worker calls) */
  adapter?: BackendAdapter | null;
}

/**
 * Hook for running lineage analysis.
 *
 * @param backendReady - Whether the backend (REST or WASM) is initialized and ready
 * @param options - Optional configuration including the backend adapter
 */
export function useAnalysis(backendReady: boolean, options?: UseAnalysisOptions) {
  const adapter = options?.adapter;
  const { currentProject, activeProjectId, backendSchema } = useProject();
  const { actions, state: lineageState } = useLineage();
  const { hideCTEs } = lineageState;
  const { getResult, setResult: storeResult, setMetrics } = useAnalysisStore();
  const getViewState = useViewStateStore((s) => s.getViewState);
  const enableLinting = activeProjectId
    ? getIssuesStateWithDefaults(getViewState(activeProjectId, 'issues')).showLintIssues
    : false;
  const [state, setState] = useState<AnalysisState>({
    isAnalyzing: false,
    error: null,
    lastAnalyzedAt: null,
  });
  const analysisRequestRef = useRef(0);
  const attemptedCacheIdentityRef = useRef<AnalysisCacheIdentity | null>(null);

  // Use ref for actions to avoid dependency issues (actions object changes every render)
  const actionsRef = useRef(actions);
  useEffect(() => {
    actionsRef.current = actions;
  }, [actions]);

  const setAnalyzing = useCallback((isAnalyzing: boolean) => {
    setState((prev) => ({ ...prev, isAnalyzing }));
  }, []);

  const setError = useCallback((error: string | null) => {
    setState((prev) => ({ ...prev, error }));
  }, []);

  const validateFiles = useCallback(
    (files: Array<{ name: string; content: string }>): FileValidationResult => {
      if (files.length === 0) {
        return { valid: false, error: 'No files to analyze' };
      }

      if (files.length > FILE_LIMITS.MAX_COUNT) {
        return {
          valid: false,
          error: `Too many files selected (max ${FILE_LIMITS.MAX_COUNT}). Currently selected: ${files.length} files.`,
        };
      }

      for (const file of files) {
        if (file.content.length > FILE_LIMITS.MAX_SIZE) {
          return {
            valid: false,
            error: `File "${file.name}" is too large (max ${FILE_LIMITS.MAX_SIZE / 1024 / 1024}MB). File size: ${(file.content.length / 1024 / 1024).toFixed(2)}MB.`,
          };
        }
      }

      return { valid: true };
    },
    []
  );

  const buildAnalysisContext = useCallback(
    (
      project: Project | null,
      activeFileContent?: string,
      // Use path (not just basename) for consistency with custom/all modes.
      // This ensures sourceName matches across all run modes.
      activeFilePath?: string
    ): AnalysisContext | null => {
      if (!project) return null;

      let contextDescription = '';
      let filesToAnalyze: Array<{ name: string; content: string }> = [];
      const runMode = project.runMode;

      if (runMode === 'current' && activeFileContent && activeFilePath) {
        filesToAnalyze = [{ name: activeFilePath, content: activeFileContent }];
        contextDescription = `Analyzing file: ${activeFilePath}`;
      } else if (runMode === 'custom') {
        const selectedIds = project.selectedFileIds || [];
        const selectedFiles = project.files.filter(
          (f) => selectedIds.includes(f.id) && f.name.endsWith('.sql')
        );
        // Use path instead of name to avoid collisions when files in different
        // directories have the same basename (e.g., "dir1/query.sql" and "dir2/query.sql")
        filesToAnalyze = selectedFiles.map((f) => ({ name: f.path, content: f.content }));
        contextDescription = `Analyzing selected: ${filesToAnalyze.length} files`;
      } else {
        const sqlFiles = project.files.filter((f) => f.name.endsWith('.sql'));
        // Use path instead of name to avoid collisions when files in different
        // directories have the same basename (e.g., "dir1/query.sql" and "dir2/query.sql")
        filesToAnalyze = sqlFiles.map((f) => ({ name: f.path, content: f.content }));
        contextDescription = `Analyzing project: ${sqlFiles.length} files`;
      }

      return {
        description: contextDescription,
        fileCount: filesToAnalyze.length,
        files: filesToAnalyze,
      };
    },
    []
  );

  const buildAnalysisPayload = useCallback(
    (project: Project | null, activeFileContent?: string, activeFilePath?: string) => {
      const context = buildAnalysisContext(project, activeFileContent, activeFilePath);
      if (!project || !context) {
        return null;
      }

      const payload: AnalysisPayload = {
        files: context.files,
        dialect: project.dialect,
        schemaSQL: project.schemaSQL ?? '',
        hideCTEs,
        enableColumnLineage: true,
        enableLinting,
        templateMode: project.templateMode,
      };

      return {
        context,
        payload,
      };
    },
    [buildAnalysisContext, enableLinting, hideCTEs]
  );

  const canUseMemoryCache = !adapter || adapter.type === 'wasm';
  // The canonical hash scans every file character. Skip it for REST (which cannot
  // reuse memory results), debounce it for interactive WASM project changes, and
  // keep proactive hashing bounded. Explicit analysis runs still build exact keys.
  const currentAnalysisPayload = useMemo(() => {
    if (!canUseMemoryCache || !activeProjectId) {
      return null;
    }

    const activeFile = currentProject?.files.find(
      (file) => file.id === currentProject.activeFileId
    );
    const analysisPayload = buildAnalysisPayload(
      currentProject,
      activeFile?.content,
      activeFile?.path
    );
    if (!analysisPayload || !canBuildProactiveAnalysisCacheKey(analysisPayload.payload)) {
      return null;
    }

    return { ...analysisPayload, projectId: activeProjectId };
  }, [activeProjectId, buildAnalysisPayload, canUseMemoryCache, currentProject]);
  const debouncedAnalysisPayload = useDebounce(
    currentAnalysisPayload,
    ANALYSIS_CACHE_KEY_DEBOUNCE_MS
  );
  const proactiveCacheInputIsSettled = currentAnalysisPayload === debouncedAnalysisPayload;
  const currentAnalysisInput = useMemo(() => {
    if (
      !canUseMemoryCache ||
      !debouncedAnalysisPayload ||
      debouncedAnalysisPayload.projectId !== activeProjectId
    ) {
      return null;
    }

    return {
      context: debouncedAnalysisPayload.context,
      payload: debouncedAnalysisPayload.payload,
      cacheKey: buildAnalysisCacheKey(debouncedAnalysisPayload.payload),
    };
  }, [activeProjectId, canUseMemoryCache, debouncedAnalysisPayload]);
  const currentAnalysisCacheKey = currentAnalysisInput?.cacheKey ?? null;

  // The REST server owns additional schema/template configuration that is not
  // fully represented by AnalysisPayload. Never reuse its results in memory;
  // this identity only clears the displayed result when the visible schema changes.
  const backendSchemaIdentity = useMemo(
    () => (adapter?.type === 'rest' ? JSON.stringify(backendSchema) : null),
    [adapter?.type, backendSchema]
  );

  useEffect(() => {
    if (!backendReady || !currentProject) {
      return;
    }

    let cancelled = false;
    // Use file.path as name to match how buildAnalysisContext keys files.
    // This ensures the worker cache uses consistent keys (paths) across sync and analysis.
    const sqlFiles = currentProject.files
      .filter((file) => file.name.endsWith('.sql'))
      .map((f) => ({ name: f.path, content: f.content }));

    if (ANALYSIS_DEBUG)
      console.log(`[useAnalysis] File sync effect triggered (${sqlFiles.length} SQL files)`);
    const syncEffectStart = nowMs();

    const syncFiles = adapter ? adapter.syncFiles(sqlFiles) : syncAnalysisFiles(sqlFiles);

    syncFiles
      .then(() => {
        if (!cancelled && ANALYSIS_DEBUG) {
          console.log(
            `[useAnalysis] File sync effect completed in ${(nowMs() - syncEffectStart).toFixed(1)}ms`
          );
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          console.warn('Failed to sync analysis files:', error);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [currentProject, backendReady, adapter]);

  // Restore a result only when switching to a project whose canonical analysis
  // key matches. Input changes within the active project may keep the current
  // graph visible for the staleness UI, but that result is never restored or
  // sent to the worker as a known cache hit under a different key.
  useEffect(() => {
    if (ANALYSIS_DEBUG)
      console.log(
        `[useAnalysis] Memory cache effect triggered (projectId: ${activeProjectId?.slice(0, 8) ?? 'null'})`
      );
    const memoryCacheStart = nowMs();

    if (!activeProjectId || !canUseMemoryCache) {
      attemptedCacheIdentityRef.current = null;
      actionsRef.current.setResult(null);
      return;
    }

    if (!currentAnalysisCacheKey) {
      if (attemptedCacheIdentityRef.current?.projectId !== activeProjectId) {
        actionsRef.current.setResult(null);
        attemptedCacheIdentityRef.current = { projectId: activeProjectId, cacheKey: null };
      }
      return;
    }

    const cachedResult = getResult(activeProjectId, currentAnalysisCacheKey);
    const nextIdentity = { projectId: activeProjectId, cacheKey: currentAnalysisCacheKey };
    const restoreDecision = getAnalysisCacheRestoreDecision(
      attemptedCacheIdentityRef.current,
      nextIdentity,
      cachedResult
    );
    attemptedCacheIdentityRef.current = nextIdentity;
    if (ANALYSIS_DEBUG)
      console.log(
        `[useAnalysis] Memory cache ${cachedResult ? 'HIT' : 'MISS'} (${(nowMs() - memoryCacheStart).toFixed(1)}ms)`
      );
    if (!restoreDecision.shouldSetResult) {
      return;
    }
    // Use startTransition to make the result update low-priority,
    // allowing UI interactions and worker callbacks to proceed without blocking
    startTransition(() => {
      actionsRef.current.setResult(restoreDecision.result);
      if (restoreDecision.result && currentAnalysisInput) {
        actionsRef.current.setAnalyzedContent(
          new Map(currentAnalysisInput.context.files.map((file) => [file.name, file.content]))
        );
        actionsRef.current.setStalePaths([]);
      }
    });
  }, [
    activeProjectId,
    backendReady,
    backendSchemaIdentity,
    canUseMemoryCache,
    currentAnalysisCacheKey,
    currentAnalysisInput,
    getResult,
  ]);

  // Check worker's IndexedDB cache for persisted analysis results.
  // This runs after the memory cache effect and may update the result
  // if a cached result is found in the worker's persistent storage.
  useEffect(() => {
    if (ANALYSIS_DEBUG)
      console.log(
        `[useAnalysis] IndexedDB cache effect triggered (projectId: ${activeProjectId?.slice(0, 8) ?? 'null'})`
      );

    if (
      !backendReady ||
      !activeProjectId ||
      !currentAnalysisInput ||
      !proactiveCacheInputIsSettled
    ) {
      return;
    }

    const cachedResult = canUseMemoryCache
      ? getResult(activeProjectId, currentAnalysisInput.cacheKey)
      : null;
    if (cachedResult) {
      if (ANALYSIS_DEBUG) console.log('[useAnalysis] IndexedDB cache skipped (memory cache hit)');
      return;
    }

    const { context, payload: cachePayload, cacheKey } = currentAnalysisInput;
    if (context.files.length === 0) {
      return;
    }

    let cancelled = false;
    const cacheStart = nowMs();
    if (ANALYSIS_DEBUG)
      console.log(`[useAnalysis] Checking IndexedDB cache for ${context.files.length} files`);

    const syncAndGetCache = adapter
      ? adapter.syncFiles(context.files).then(() => {
          if (ANALYSIS_DEBUG) console.log(`[useAnalysis] Files synced, checking cache...`);
          return adapter.getCached(cachePayload);
        })
      : syncAnalysisFiles(context.files).then(() => {
          if (ANALYSIS_DEBUG)
            console.log(`[useAnalysis] Files synced, checking IndexedDB cache...`);
          return getCachedAnalysis({
            fileNames: context.files.map((file) => file.name),
            dialect: cachePayload.dialect,
            schemaSQL: cachePayload.schemaSQL,
            hideCTEs: cachePayload.hideCTEs,
            enableColumnLineage: cachePayload.enableColumnLineage,
            enableLinting: cachePayload.enableLinting,
            templateMode: cachePayload.templateMode,
          });
        });

    syncAndGetCache
      .then((cached) => {
        const durationMs = nowMs() - cacheStart;
        if (cancelled) {
          if (ANALYSIS_DEBUG)
            console.log(`[useAnalysis] IndexedDB cache cancelled after ${durationMs.toFixed(1)}ms`);
          return;
        }
        if (!cached?.result || cached.cacheKey !== cacheKey) {
          if (ANALYSIS_DEBUG)
            console.log(`[useAnalysis] IndexedDB cache MISS after ${durationMs.toFixed(1)}ms`);
          return;
        }
        if (ANALYSIS_DEBUG)
          console.log(
            `[useAnalysis] IndexedDB cache HIT after ${durationMs.toFixed(1)}ms - calling setResult`
          );
        // Use startTransition to make the result update low-priority,
        // allowing UI interactions and worker callbacks to proceed without blocking
        startTransition(() => {
          actionsRef.current.setResult(cached.result);
          // Cache hits imply the current file content already matches what
          // was analyzed (cache key is content-derived), so seed the
          // staleness snapshot from the live files used to build `context`.
          actionsRef.current.setAnalyzedContent(
            new Map(context.files.map((f) => [f.name, f.content]))
          );
          actionsRef.current.setStalePaths([]);
        });
        if (canUseMemoryCache) {
          storeResult(activeProjectId, cacheKey, cached.result);
        }
        setMetrics(activeProjectId, {
          lastDurationMs: durationMs,
          lastCacheHit: true,
          lastCacheKey: cached.cacheKey,
          lastAnalyzedAt: Date.now(),
          workerTimings: cached.timings ?? null,
        });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          console.warn('Failed to restore cached analysis:', error);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    activeProjectId,
    canUseMemoryCache,
    currentAnalysisInput,
    proactiveCacheInputIsSettled,
    getResult,
    storeResult,
    setMetrics,
    backendReady,
    adapter,
  ]);

  const runAnalysis = useCallback(
    async (activeFileContent?: string, activeFilePath?: string) => {
      if (!backendReady || !currentProject) return;

      const requestId = analysisRequestRef.current + 1;
      analysisRequestRef.current = requestId;

      setAnalyzing(true);
      setError(null);

      const analysisStart = performance.now();
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

      try {
        const analysisInput = buildAnalysisPayload(
          currentProject,
          activeFileContent,
          activeFilePath
        );

        if (!analysisInput) {
          setError('No project context available');
          return;
        }

        const { context, payload: adapterPayload } = analysisInput;

        if (context.files.length === 0) {
          if (currentProject.runMode === 'custom') {
            setError('No files selected for analysis.');
            return;
          }
          if (currentProject.files.length > 0) {
            setError('No .sql files found in project.');
            return;
          }
          return;
        }

        const validation = validateFiles(context.files);
        if (!validation.valid) {
          setError(validation.error || 'Validation failed');
          return;
        }
        const cacheKey = canUseMemoryCache ? buildAnalysisCacheKey(adapterPayload) : null;

        console.log(context.description);

        let shouldBuildPreview = context.files.length <= ANALYSIS_SQL_PREVIEW_LIMITS.MAX_FILES;
        let totalChars = 0;

        if (shouldBuildPreview) {
          totalChars = context.files.reduce((sum, file) => sum + file.content.length, 0);
          shouldBuildPreview = totalChars <= ANALYSIS_SQL_PREVIEW_LIMITS.MAX_CHARS;
        }

        if (shouldBuildPreview) {
          const representativeSql = context.files
            .map((f) => `-- File: ${f.name}\n${f.content}`)
            .join('\n\n');
          actionsRef.current.setSql(representativeSql);
        } else if (activeFileContent) {
          actionsRef.current.setSql(activeFileContent);
        }

        const cachedResult =
          activeProjectId && cacheKey ? getResult(activeProjectId, cacheKey) : null;
        const knownCacheKey = cachedResult ? cacheKey : null;
        const displayResult = (result: AnalyzeResult) => {
          startTransition(() => {
            actionsRef.current.setResult(result);
            actionsRef.current.setAnalyzedContent(
              new Map(context.files.map((file) => [file.name, file.content]))
            );
            actionsRef.current.setStalePaths([]);
          });
        };

        // A known key asks the worker to omit its result payload, so install the
        // exact memory hit before allowing that response optimization.
        if (cachedResult) {
          displayResult(cachedResult);
        }

        let analysisResponse: Awaited<ReturnType<typeof analyzeWithWorker>>;
        let fileSyncRetries = 0;

        // Use adapter if available, otherwise fall back to direct worker calls
        if (adapter) {
          while (true) {
            try {
              analysisResponse = await adapter.analyze(adapterPayload, { knownCacheKey });
              break;
            } catch (error) {
              if (
                isAnalysisError(error, AnalysisErrorCode.MISSING_FILE_CONTENT) &&
                fileSyncRetries < MAX_FILE_SYNC_RETRIES
              ) {
                fileSyncRetries++;
                await adapter.syncFiles(context.files);
                continue;
              }
              throw error;
            }
          }
        } else {
          // Fallback to direct worker calls for backwards compatibility
          const workerPayload = {
            fileNames: context.files.map((file) => file.name),
            dialect: adapterPayload.dialect,
            schemaSQL: adapterPayload.schemaSQL,
            hideCTEs: adapterPayload.hideCTEs,
            enableColumnLineage: adapterPayload.enableColumnLineage,
            enableLinting: adapterPayload.enableLinting,
            templateMode: adapterPayload.templateMode,
          };

          while (true) {
            try {
              analysisResponse = await analyzeWithWorker(workerPayload, { knownCacheKey });
              break;
            } catch (error) {
              // Handle missing file content by syncing files and retrying.
              // Uses structured error codes instead of string matching for reliability.
              // Limited retries prevent infinite loops if sync consistently fails.
              if (
                isAnalysisError(error, AnalysisErrorCode.MISSING_FILE_CONTENT) &&
                fileSyncRetries < MAX_FILE_SYNC_RETRIES
              ) {
                fileSyncRetries++;
                await syncAnalysisFiles(context.files);
                continue;
              }
              throw error;
            }
          }
        }

        if (analysisRequestRef.current !== requestId) {
          return;
        }

        const durationMs = performance.now() - analysisStart;

        if (!analysisResponse.skipped && analysisResponse.result) {
          if (cacheKey && analysisResponse.cacheKey !== cacheKey) {
            throw new Error('Analysis response cache key did not match the requested inputs');
          }

          // Snapshot the exact content that was just analyzed so the staleness
          // gate (#22) has a baseline keyed like analyzer statements.
          displayResult(analysisResponse.result);
          if (activeProjectId && cacheKey) {
            storeResult(activeProjectId, cacheKey, analysisResponse.result);
          }
        }

        if (activeProjectId) {
          setMetrics(activeProjectId, {
            lastDurationMs: durationMs,
            lastCacheHit: analysisResponse.cacheHit,
            lastCacheKey: analysisResponse.cacheKey,
            lastAnalyzedAt: Date.now(),
            workerTimings: analysisResponse.timings ?? null,
          });
        }
        setState((prev) => ({ ...prev, lastAnalyzedAt: Date.now() }));
      } catch (error) {
        if (analysisRequestRef.current !== requestId) {
          return;
        }
        setError(error instanceof Error ? error.message : 'Analysis failed');
        console.error(error);
      } finally {
        if (analysisRequestRef.current === requestId) {
          setAnalyzing(false);
        }
      }
    },
    [
      backendReady,
      currentProject,
      activeProjectId,
      storeResult,
      setMetrics,
      getResult,
      buildAnalysisPayload,
      validateFiles,
      setAnalyzing,
      setError,
      canUseMemoryCache,
      adapter,
    ]
  );

  return {
    ...state,
    runAnalysis,
    setError,
  };
}
