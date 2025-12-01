/**
 * Analysis Store
 *
 * Manages per-project analysis results in memory.
 * Results are NOT persisted to localStorage due to size constraints.
 * When the user switches projects, we restore their cached result if available.
 */

import { create } from 'zustand';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';

interface AnalysisStore {
  /** Analysis results keyed by project ID */
  results: Record<string, AnalyzeResult>;

  /** Get analysis result for a project */
  getResult: (projectId: string) => AnalyzeResult | null;

  /** Set analysis result for a project */
  setResult: (projectId: string, result: AnalyzeResult) => void;

  /** Clear analysis result for a project */
  clearResult: (projectId: string) => void;

  /** Clear all results */
  clearAllResults: () => void;
}

export const useAnalysisStore = create<AnalysisStore>((set, get) => ({
  results: {},

  getResult: (projectId) => get().results[projectId] ?? null,

  setResult: (projectId, result) =>
    set((state) => ({
      results: {
        ...state.results,
        [projectId]: result,
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
