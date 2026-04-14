import { vi } from 'vitest';

export const useLineageState = vi.fn(() => ({
  result: null,
  viewMode: 'table',
  layoutAlgorithm: 'dagre',
  hideCTEs: false,
  highlightedSpan: null,
}));

export const useLineageActions = vi.fn(() => ({
  highlightSpan: vi.fn(),
  setViewMode: vi.fn(),
  toggleColumnEdges: vi.fn(),
  setAllNodesCollapsed: vi.fn(),
  toggleShowScriptTables: vi.fn(),
  setLayoutAlgorithm: vi.fn(),
}));

export const SqlView = vi.fn(() => null);
