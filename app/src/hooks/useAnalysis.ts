import { useState, useCallback, useEffect, useRef, startTransition } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import { analyzeWithWorker, getCachedAnalysis, syncAnalysisFiles } from '@/lib/analysis-worker';
import type { BackendAdapter, AnalysisPayload } from '@/lib/backend-adapter';
import { useProject } from '@/lib/project-store';
import type { Project } from '@/lib/project-store';
import { useAnalysisStore } from '@/lib/analysis-store';
import { FILE_LIMITS, ANALYSIS_SQL_PREVIEW_LIMITS } from '@/lib/constants';
import { AnalysisErrorCode, isAnalysisError } from '@/types';
import type { AnalysisState, AnalysisContext, FileValidationResult } from '@/types';

// Maximum retry attempts for file sync errors to prevent infinite loops
const MAX_FILE_SYNC_RETRIES = 1;

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
 * @param backendReady - Whether the backend is ready (wasmReady for backwards compatibility)
 * @param options - Optional configuration including the backend adapter
 */
export function useAnalysis(backendReady: boolean, options?: UseAnalysisOptions) {
  const adapter = options?.adapter;
  const { currentProject, activeProjectId } = useProject();
  const { actions, state: lineageState } = useLineage();
  const { hideCTEs } = lineageState;
  const { getResult, getMetrics, setResult: storeResult, setMetrics } = useAnalysisStore();
  const [state, setState] = useState<AnalysisState>({
    isAnalyzing: false,
    error: null,
    lastAnalyzedAt: null,
  });
  const analysisRequestRef = useRef(0);
  const currentProjectRef = useRef<Project | null>(currentProject);

  useEffect(() => {
    currentProjectRef.current = currentProject;
  }, [currentProject]);

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

  // Restore cached analysis result from memory when project or hideCTEs changes.
  // Cache validation is built into getResult - it returns null if the cached
  // result was computed with a different hideCTEs setting.
  useEffect(() => {
    if (ANALYSIS_DEBUG)
      console.log(
        `[useAnalysis] Memory cache effect triggered (projectId: ${activeProjectId?.slice(0, 8) ?? 'null'})`
      );
    const memoryCacheStart = nowMs();

    if (!activeProjectId) {
      actionsRef.current.setResult(null);
      return;
    }

    const cachedResult = getResult(activeProjectId, hideCTEs);
    if (ANALYSIS_DEBUG)
      console.log(
        `[useAnalysis] Memory cache ${cachedResult ? 'HIT' : 'MISS'} (${(nowMs() - memoryCacheStart).toFixed(1)}ms)`
      );
    // Use startTransition to make the result update low-priority,
    // allowing UI interactions and worker callbacks to proceed without blocking
    startTransition(() => {
      actionsRef.current.setResult(cachedResult);
    });

    if (cachedResult || !backendReady) {
      return;
    }
  }, [activeProjectId, hideCTEs, getResult, backendReady]);

  // Check worker's IndexedDB cache for persisted analysis results.
  // This runs after the memory cache effect and may update the result
  // if a cached result is found in the worker's persistent storage.
  useEffect(() => {
    if (ANALYSIS_DEBUG)
      console.log(
        `[useAnalysis] IndexedDB cache effect triggered (projectId: ${activeProjectId?.slice(0, 8) ?? 'null'})`
      );

    if (!backendReady || !activeProjectId) {
      return;
    }

    const cachedResult = getResult(activeProjectId, hideCTEs);
    if (cachedResult) {
      if (ANALYSIS_DEBUG) console.log('[useAnalysis] IndexedDB cache skipped (memory cache hit)');
      return;
    }

    const project = currentProjectRef.current;
    if (!project) {
      return;
    }

    const activeFile = project.files.find((file) => file.id === project.activeFileId);
    const context = buildAnalysisContext(project, activeFile?.content, activeFile?.path);
    if (!context || context.files.length === 0) {
      return;
    }

    let cancelled = false;
    const cacheStart = nowMs();
    if (ANALYSIS_DEBUG)
      console.log(`[useAnalysis] Checking IndexedDB cache for ${context.files.length} files`);

    const cachePayload: AnalysisPayload = {
      files: context.files,
      dialect: project.dialect,
      schemaSQL: project.schemaSQL ?? '',
      hideCTEs,
      enableColumnLineage: true,
      templateMode: project.templateMode,
    };

    const syncAndGetCache = adapter
      ? adapter.syncFiles(context.files).then(() => {
          if (ANALYSIS_DEBUG) console.log(`[useAnalysis] Files synced, checking cache...`);
          return adapter.getCached(cachePayload);
        })
      : syncAnalysisFiles(context.files).then(() => {
          if (ANALYSIS_DEBUG) console.log(`[useAnalysis] Files synced, checking IndexedDB cache...`);
          return getCachedAnalysis({
            fileNames: context.files.map((file) => file.name),
            dialect: project.dialect,
            schemaSQL: project.schemaSQL ?? '',
            hideCTEs,
            enableColumnLineage: true,
            templateMode: project.templateMode,
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
        if (!cached?.result) {
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
        });
        storeResult(activeProjectId, cached.result, hideCTEs);
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
    hideCTEs,
    getResult,
    storeResult,
    setMetrics,
    backendReady,
    buildAnalysisContext,
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
        const context = buildAnalysisContext(currentProject, activeFileContent, activeFilePath);

        if (!context) {
          setError('No project context available');
          return;
        }

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

        const adapterPayload: AnalysisPayload = {
          files: context.files,
          dialect: currentProject.dialect,
          schemaSQL: currentProject.schemaSQL ?? '',
          hideCTEs,
          enableColumnLineage: true,
          templateMode: currentProject.templateMode,
        };

        const cachedResult = activeProjectId ? getResult(activeProjectId, hideCTEs) : null;
        const knownCacheKey =
          cachedResult && activeProjectId
            ? (getMetrics(activeProjectId)?.lastCacheKey ?? null)
            : null;

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
            dialect: currentProject.dialect,
            schemaSQL: currentProject.schemaSQL ?? '',
            hideCTEs,
            enableColumnLineage: true,
            templateMode: currentProject.templateMode,
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
          // Use startTransition to make the result update low-priority,
          // allowing UI interactions and worker callbacks to proceed without blocking
          startTransition(() => {
            actionsRef.current.setResult(analysisResponse.result);
          });
          if (activeProjectId) {
            storeResult(activeProjectId, analysisResponse.result, hideCTEs);
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
      getMetrics,
      getResult,
      buildAnalysisContext,
      validateFiles,
      setAnalyzing,
      setError,
      hideCTEs,
      adapter,
    ]
  );

  return {
    ...state,
    runAnalysis,
    setError,
  };
}
