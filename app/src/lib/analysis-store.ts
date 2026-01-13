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

interface AnalysisStore {
  /** Analysis results keyed by project ID, with metadata for cache validation */
  results: Record<string, CachedResult>;

  /** Get analysis result for a project if cache is valid for given hideCTEs */
  getResult: (projectId: string, hideCTEs: boolean) => AnalyzeResult | null;

  /** Set analysis result for a project with its hideCTEs context */
  setResult: (projectId: string, result: AnalyzeResult, hideCTEs: boolean) => void;

  /** Clear analysis result for a project */
  clearResult: (projectId: string) => void;

  /** Clear all results */
  clearAllResults: () => void;
}

export const useAnalysisStore = create<AnalysisStore>((set, get) => ({
  results: {},

  getResult: (projectId, hideCTEs) => {
    const cached = get().results[projectId];
    // Return cached result only if it was computed with the same hideCTEs setting
    if (cached && cached.hideCTEs === hideCTEs) {
      return cached.result;
    }
    return null;
  },

  setResult: (projectId, result, hideCTEs) =>
    set((state) => ({
      results: {
        ...state.results,
        [projectId]: { result, hideCTEs },
      },
    })),

  clearResult: (projectId) =>
    set((state) => {
      const { [projectId]: _removed, ...rest } = state.results;
      void _removed;
      return { results: rest };
    }),

  clearAllResults: () => set({ results: {} }),
}));
