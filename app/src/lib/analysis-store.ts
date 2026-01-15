/**
 * Analysis Store
 *
 * Manages per-project analysis results in memory.
 * Results are NOT persisted to localStorage due to size constraints.
 * When the user switches projects, we restore their cached result if available.
 *
 * Each cached result tracks the hideCTEs setting used during analysis,
 * allowing cache validation when options change.
 */

import { create } from 'zustand';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';

interface CachedResult {
  result: AnalyzeResult;
  /** The hideCTEs value used when this result was computed */
  hideCTEs: boolean;
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
  /** Analysis results keyed by project ID, with metadata for cache validation */
  results: Record<string, CachedResult>;
  /** Performance metrics keyed by project ID */
  metrics: Record<string, AnalysisMetrics>;

  /** Get analysis result for a project if cache is valid for given hideCTEs */
  getResult: (projectId: string, hideCTEs: boolean) => AnalyzeResult | null;
  /** Get performance metrics for a project */
  getMetrics: (projectId: string) => AnalysisMetrics | null;

  /** Set analysis result for a project with its hideCTEs context */
  setResult: (projectId: string, result: AnalyzeResult, hideCTEs: boolean) => void;
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

  getResult: (projectId, hideCTEs) => {
    const cached = get().results[projectId];
    // Return cached result only if it was computed with the same hideCTEs setting
    if (cached && cached.hideCTEs === hideCTEs) {
      return cached.result;
    }
    return null;
  },

  getMetrics: (projectId) => get().metrics[projectId] ?? null,

  setResult: (projectId, result, hideCTEs) =>
    set((state) => ({
      results: {
        ...state.results,
        [projectId]: { result, hideCTEs },
      },
    })),

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
