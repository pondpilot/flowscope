import {
  useState,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  startTransition,
} from 'react';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { useLineageActions, useLineageState } from '@flowscope-react/store';
import { analyzeWithWorker, getCachedAnalysis, syncAnalysisFiles } from '@/lib/analysis-worker';
import type { BackendAdapter, AnalysisPayload } from '@/lib/backend-adapter';
import { useProject } from '@/lib/project-store';
import type { Project, RunMode } from '@/lib/project-store';
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
/** Idle window before proactive cache hashing and worker synchronization. */
const ANALYSIS_CACHE_KEY_DEBOUNCE_MS = 300;

interface ActiveAnalysisRequest {
  requestId: number;
  projectId: string | null;
  runMode: RunMode;
  currentFilePath?: string;
  payload: AnalysisPayload;
  adapter: BackendAdapter | null | undefined;
  backendReady: boolean;
  backendSchemaIdentity: string | null;
}

interface ExplicitResultAuthority {
  requestId: number;
  projectId: string;
  configuredPayload: AnalysisPayload;
  adapter: BackendAdapter | null | undefined;
  backendReady: boolean;
  backendSchemaIdentity: string | null;
}

function analysisPayloadsEqual(left: AnalysisPayload, right: AnalysisPayload): boolean {
  return (
    left.dialect === right.dialect &&
    left.schemaSQL === right.schemaSQL &&
    left.hideCTEs === right.hideCTEs &&
    left.enableColumnLineage === right.enableColumnLineage &&
    left.enableLinting === right.enableLinting &&
    left.templateMode === right.templateMode &&
    left.files.length === right.files.length &&
    left.files.every(
      (file, index) =>
        file.name === right.files[index].name && file.content === right.files[index].content
    )
  );
}

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
  const actions = useLineageActions();
  const hideCTEs = useLineageState((state) => state.hideCTEs);
  const { getResult, setResult: storeResult, setMetrics } = useAnalysisStore();
  const issuesState = useViewStateStore((store) =>
    activeProjectId ? store.viewStates[activeProjectId]?.issues : undefined
  );
  const enableLinting = getIssuesStateWithDefaults(issuesState).showLintIssues;
  const [state, setState] = useState<AnalysisState>({
    isAnalyzing: false,
    error: null,
    lastAnalyzedAt: null,
  });
  const analysisRequestRef = useRef(0);
  const activeAnalysisRequestRef = useRef<ActiveAnalysisRequest | null>(null);
  const proactiveRestoreRequestRef = useRef(0);
  const explicitResultAuthorityRef = useRef<ExplicitResultAuthority | null>(null);
  const attemptedCacheIdentityRef = useRef<AnalysisCacheIdentity | null>(null);

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
      activeFilePath?: string,
      runModeOverride?: RunMode
    ): AnalysisContext | null => {
      if (!project) return null;

      let contextDescription = '';
      let filesToAnalyze: Array<{ name: string; content: string }> = [];
      const runMode = runModeOverride ?? project.runMode;

      if (
        runMode === 'current' &&
        activeFileContent !== undefined &&
        activeFilePath !== undefined
      ) {
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
    (
      project: Project | null,
      activeFileContent?: string,
      activeFilePath?: string,
      runModeOverride?: RunMode
    ) => {
      const context = buildAnalysisContext(
        project,
        activeFileContent,
        activeFilePath,
        runModeOverride
      );
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

  // An explicit result remains authoritative while the configured analysis
  // inputs are semantically unchanged. This prevents a metadata-only project
  // replacement from scheduling the same proactive cache restore and replacing
  // an active-file result with the saved all/custom-mode result.
  useLayoutEffect(() => {
    const authority = explicitResultAuthorityRef.current;
    if (!authority) return;

    if (
      authority.projectId !== activeProjectId ||
      authority.adapter !== adapter ||
      authority.backendReady !== backendReady ||
      authority.backendSchemaIdentity !== backendSchemaIdentity ||
      !currentAnalysisPayload ||
      !analysisPayloadsEqual(authority.configuredPayload, currentAnalysisPayload.payload)
    ) {
      explicitResultAuthorityRef.current = null;
    }
  }, [activeProjectId, adapter, backendReady, backendSchemaIdentity, currentAnalysisPayload]);

  // A late result belongs to the exact inputs captured by runAnalysis, which
  // may use a one-off run mode. Rebuild that same mode from the live project so
  // unrelated edits and metadata updates do not cancel it, while an edit to an
  // explicitly analyzed file always does.
  useLayoutEffect(() => {
    const activeRequest = activeAnalysisRequestRef.current;
    if (!activeRequest) return;

    let liveAnalysisInput: ReturnType<typeof buildAnalysisPayload> = null;
    if (activeRequest.runMode === 'current') {
      const currentFile = currentProject?.files.find(
        (file) => file.path === activeRequest.currentFilePath
      );
      if (currentFile) {
        liveAnalysisInput = buildAnalysisPayload(
          currentProject,
          currentFile.content,
          currentFile.path,
          'current'
        );
      }
    } else {
      liveAnalysisInput = buildAnalysisPayload(
        currentProject,
        undefined,
        undefined,
        activeRequest.runMode
      );
    }

    const shouldInvalidate =
      activeRequest.projectId !== activeProjectId ||
      activeRequest.adapter !== adapter ||
      activeRequest.backendReady !== backendReady ||
      activeRequest.backendSchemaIdentity !== backendSchemaIdentity ||
      !liveAnalysisInput ||
      !analysisPayloadsEqual(activeRequest.payload, liveAnalysisInput.payload);

    if (!shouldInvalidate) return;

    analysisRequestRef.current += 1;
    activeAnalysisRequestRef.current = null;
    if (explicitResultAuthorityRef.current?.requestId === activeRequest.requestId) {
      explicitResultAuthorityRef.current = null;
    }
    setState((previous) => (previous.isAnalyzing ? { ...previous, isAnalyzing: false } : previous));
  }, [
    activeProjectId,
    adapter,
    backendReady,
    backendSchemaIdentity,
    buildAnalysisPayload,
    currentProject,
  ]);

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
      actions.setResult(null);
      return;
    }

    if (!currentAnalysisCacheKey) {
      if (attemptedCacheIdentityRef.current?.projectId !== activeProjectId) {
        actions.setResult(null);
        attemptedCacheIdentityRef.current = { projectId: activeProjectId, cacheKey: null };
      }
      return;
    }

    const nextIdentity = { projectId: activeProjectId, cacheKey: currentAnalysisCacheKey };
    const explicitAuthority = explicitResultAuthorityRef.current;
    if (
      explicitAuthority?.projectId === activeProjectId &&
      currentAnalysisInput &&
      analysisPayloadsEqual(explicitAuthority.configuredPayload, currentAnalysisInput.payload)
    ) {
      attemptedCacheIdentityRef.current = nextIdentity;
      return;
    }

    const cachedResult = getResult(activeProjectId, currentAnalysisCacheKey);
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
      actions.setResult(restoreDecision.result);
      if (restoreDecision.result && currentAnalysisInput) {
        actions.setAnalyzedContent(
          new Map(currentAnalysisInput.context.files.map((file) => [file.name, file.content]))
        );
        actions.setStalePaths([]);
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
    actions,
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

    const explicitAuthority = explicitResultAuthorityRef.current;
    if (
      explicitAuthority?.projectId === activeProjectId &&
      analysisPayloadsEqual(explicitAuthority.configuredPayload, currentAnalysisInput.payload)
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
    const restoreRequestId = proactiveRestoreRequestRef.current + 1;
    proactiveRestoreRequestRef.current = restoreRequestId;
    const cacheStart = nowMs();
    if (ANALYSIS_DEBUG)
      console.log(`[useAnalysis] Checking IndexedDB cache for ${context.files.length} files`);

    const syncAndGetCache = adapter
      ? adapter.syncFiles(context.files).then(() => {
          if (cancelled || proactiveRestoreRequestRef.current !== restoreRequestId) return null;
          if (ANALYSIS_DEBUG) console.log(`[useAnalysis] Files synced, checking cache...`);
          return adapter.getCached(cachePayload);
        })
      : syncAnalysisFiles(context.files).then(() => {
          if (cancelled || proactiveRestoreRequestRef.current !== restoreRequestId) return null;
          if (ANALYSIS_DEBUG)
            console.log(`[useAnalysis] Files synced, checking IndexedDB cache...`);
          return getCachedAnalysis(
            {
              fileNames: context.files.map((file) => file.name),
              dialect: cachePayload.dialect,
              schemaSQL: cachePayload.schemaSQL,
              hideCTEs: cachePayload.hideCTEs,
              enableColumnLineage: cachePayload.enableColumnLineage,
              enableLinting: cachePayload.enableLinting,
              templateMode: cachePayload.templateMode,
            },
            context.files
          );
        });

    syncAndGetCache
      .then((cached) => {
        const durationMs = nowMs() - cacheStart;
        if (cancelled || proactiveRestoreRequestRef.current !== restoreRequestId) {
          if (ANALYSIS_DEBUG)
            console.log(`[useAnalysis] IndexedDB cache cancelled after ${durationMs.toFixed(1)}ms`);
          return;
        }
        const latestExplicitAuthority = explicitResultAuthorityRef.current;
        if (
          latestExplicitAuthority?.projectId === activeProjectId &&
          analysisPayloadsEqual(
            latestExplicitAuthority.configuredPayload,
            currentAnalysisInput.payload
          )
        ) {
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
          actions.setResult(cached.result);
          // Cache hits imply the current file content already matches what
          // was analyzed (cache key is content-derived), so seed the
          // staleness snapshot from the live files used to build `context`.
          actions.setAnalyzedContent(new Map(context.files.map((f) => [f.name, f.content])));
          actions.setStalePaths([]);
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
    actions,
  ]);

  const runAnalysis = useCallback(
    async (activeFileContent?: string, activeFilePath?: string, runModeOverride?: RunMode) => {
      if (!backendReady || !currentProject) return;

      const analysisInput = buildAnalysisPayload(
        currentProject,
        activeFileContent,
        activeFilePath,
        runModeOverride
      );
      const runMode = runModeOverride ?? currentProject.runMode;
      const requestId = analysisRequestRef.current + 1;
      analysisRequestRef.current = requestId;
      activeAnalysisRequestRef.current = analysisInput
        ? {
            requestId,
            projectId: activeProjectId,
            runMode,
            currentFilePath: runMode === 'current' ? activeFilePath : undefined,
            payload: analysisInput.payload,
            adapter,
            backendReady,
            backendSchemaIdentity,
          }
        : null;
      // An explicit user run owns the visible result. Prevent an older
      // proactive IndexedDB restore from replacing it after the worker queue
      // drains, including active-file runs that do not change project mode.
      proactiveRestoreRequestRef.current += 1;
      explicitResultAuthorityRef.current =
        canUseMemoryCache && activeProjectId && currentAnalysisPayload
          ? {
              requestId,
              projectId: activeProjectId,
              configuredPayload: currentAnalysisPayload.payload,
              adapter,
              backendReady,
              backendSchemaIdentity,
            }
          : null;

      setAnalyzing(true);
      setError(null);

      const analysisStart = performance.now();
      let explicitResultAccepted = false;
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

      if (analysisRequestRef.current !== requestId) {
        return;
      }

      try {
        if (!analysisInput) {
          setError('No project context available');
          return;
        }

        const { context, payload: adapterPayload } = analysisInput;

        if (context.files.length === 0) {
          if ((runModeOverride ?? currentProject.runMode) === 'custom') {
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
          actions.setSql(representativeSql);
        } else if (activeFileContent) {
          actions.setSql(activeFileContent);
        }

        const cachedResult =
          activeProjectId && cacheKey ? getResult(activeProjectId, cacheKey) : null;
        const knownCacheKey = cachedResult ? cacheKey : null;
        const displayResult = (result: AnalyzeResult) => {
          explicitResultAccepted = true;
          startTransition(() => {
            actions.setResult(result);
            actions.setAnalyzedContent(
              new Map(context.files.map((file) => [file.name, file.content]))
            );
            actions.setStalePaths([]);
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
                await adapter.syncFiles(context.files, { forceReplace: true });
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
              analysisResponse = await analyzeWithWorker(workerPayload, {
                knownCacheKey,
                fileSnapshot: context.files,
              });
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
                await syncAnalysisFiles(context.files, { forceReplace: true });
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
          activeAnalysisRequestRef.current = null;
          if (
            !explicitResultAccepted &&
            explicitResultAuthorityRef.current?.requestId === requestId
          ) {
            explicitResultAuthorityRef.current = null;
          }
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
      currentAnalysisPayload,
      adapter,
      backendSchemaIdentity,
      actions,
    ]
  );

  return {
    ...state,
    runAnalysis,
    setError,
  };
}
