/**
 * Analysis Store
 *
 * Manages per-project analysis results in memory.
 * Results are NOT persisted to localStorage due to size constraints.
 * When the user switches projects, we restore their cached result if available.
 *
 * Each project keeps only its latest result, tagged with the canonical analysis
 * cache key. A lookup with any other key is a cache miss, so callers cannot
 * restore a result produced from different semantic inputs.
 */

import { create } from 'zustand';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';

interface CachedResult {
  result: AnalyzeResult;
  cacheKey: string;
  storedAt: number;
}

/** Large analysis graphs stay bounded even when many projects are opened. */
export const ANALYSIS_MEMORY_CACHE_MAX_ENTRIES = 10;

let cacheSequence = 0;

export interface AnalysisCacheIdentity {
  projectId: string;
  cacheKey: string | null;
}

export interface AnalysisCacheRestoreDecision {
  shouldSetResult: boolean;
  result: AnalyzeResult | null;
}

/**
 * Project switches replace the visible graph even on a miss. Within one
 * project, exact hits replace it while misses preserve the stale graph.
 */
export function getAnalysisCacheRestoreDecision(
  previous: AnalysisCacheIdentity | null,
  next: AnalysisCacheIdentity,
  cachedResult: AnalyzeResult | null
): AnalysisCacheRestoreDecision {
  if (previous?.projectId === next.projectId && previous.cacheKey === next.cacheKey) {
    return { shouldSetResult: false, result: cachedResult };
  }

  return {
    shouldSetResult: previous?.projectId !== next.projectId || cachedResult !== null,
    result: cachedResult,
  };
}

export interface AnalysisWorkerTimings {
  totalMs: number;
  cacheReadMs: number;
  schemaParseMs: number;
  analyzeMs: number;
}

export interface AnalysisMetrics {
  lastDurationMs: number | null;
  lastCacheHit: boolean | null;
  lastCacheKey: string | null;
  lastAnalyzedAt: number | null;
  workerTimings: AnalysisWorkerTimings | null;
}

interface AnalysisStore {
  /** Latest analysis result per project, bounded by ANALYSIS_MEMORY_CACHE_MAX_ENTRIES */
  results: Record<string, CachedResult>;
  /** Performance metrics keyed by project ID */
  metrics: Record<string, AnalysisMetrics>;

  /** Get a result only when its canonical analysis cache key matches exactly */
  getResult: (projectId: string, cacheKey: string) => AnalyzeResult | null;
  /** Get performance metrics for a project */
  getMetrics: (projectId: string) => AnalysisMetrics | null;

  /** Replace a project's cached result and evict the oldest project if needed */
  setResult: (projectId: string, cacheKey: string, result: AnalyzeResult) => void;
  /** Set performance metrics for a project */
  setMetrics: (projectId: string, metrics: AnalysisMetrics) => void;

  /** Clear analysis result for a project */
  clearResult: (projectId: string) => void;

  /** Clear all results */
  clearAllResults: () => void;
}

export const useAnalysisStore = create<AnalysisStore>((set, get) => ({
  results: {},
  metrics: {},

  getResult: (projectId, cacheKey) => {
    const cached = get().results[projectId];
    if (cached?.cacheKey === cacheKey) {
      return cached.result;
    }
    return null;
  },

  getMetrics: (projectId) => get().metrics[projectId] ?? null,

  setResult: (projectId, cacheKey, result) =>
    set((state) => {
      const results = {
        ...state.results,
        [projectId]: { result, cacheKey, storedAt: (cacheSequence += 1) },
      };
      const projectIds = Object.keys(results);

      if (projectIds.length <= ANALYSIS_MEMORY_CACHE_MAX_ENTRIES) {
        return { results };
      }

      const oldestProjectId = projectIds.reduce((oldest, candidate) =>
        results[candidate].storedAt < results[oldest].storedAt ? candidate : oldest
      );
      const { [oldestProjectId]: _evictedResult, ...boundedResults } = results;
      const { [oldestProjectId]: _evictedMetrics, ...boundedMetrics } = state.metrics;
      void _evictedResult;
      void _evictedMetrics;

      return { results: boundedResults, metrics: boundedMetrics };
    }),

  setMetrics: (projectId, metrics) =>
    set((state) => ({
      metrics: {
        ...state.metrics,
        [projectId]: metrics,
      },
    })),

  clearResult: (projectId) =>
    set((state) => {
      const { [projectId]: _removedResult, ...restResults } = state.results;
      const { [projectId]: _removedMetrics, ...restMetrics } = state.metrics;
      void _removedResult;
      void _removedMetrics;
      return { results: restResults, metrics: restMetrics };
    }),

  clearAllResults: () => set({ results: {}, metrics: {} }),
}));
