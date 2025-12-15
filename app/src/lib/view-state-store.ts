/**
 * View State Store
 *
 * Manages per-project, per-view UI state with localStorage persistence.
 * This allows users to switch between tabs/projects and return to the
 * same UI state (expanded nodes, filters, zoom levels, etc.)
 *
 * Also stores analysis results per project (in memory, not persisted to localStorage
 * due to size constraints).
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

// ============================================================================
// Types
// ============================================================================

export interface ViewportState {
  x: number;
  y: number;
  zoom: number;
}

/** HierarchyView state */
export interface HierarchyViewState {
  expandedNodes: string[];
  filter: string;
  detailsPanelHeight: number;
  focusedNodeKey: string | null;
  unusedExpanded: boolean;
}

/** MatrixView state */
export interface MatrixViewState {
  filterText: string;
  filterMode: 'rows' | 'columns' | 'fields';
  heatmapMode: boolean;
  xRayMode: boolean;
  xRayFilterMode: 'dim' | 'hide';
  clusterMode: boolean;
  complexityMode: boolean;
  showLegend: boolean;
  focusedNode: string | null;
  firstColumnWidth: number;
  headerHeight: number;
}

/** LineageView (GraphView) state */
export interface LineageViewState {
  searchTerm: string;
  selectedNodeId: string | null;
  viewport: ViewportState | null;
}

/** SchemaView state - minimal since viewport is handled internally */
export interface SchemaViewState {
  selectedTableName: string | null;
}

/** IssuesView state - for future expansion */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface IssuesViewState {
  // Future: collapsed categories, sort order, etc.
}

/** All view states for a single project */
export interface ProjectViewStates {
  activeTab: 'lineage' | 'hierarchy' | 'matrix' | 'schema' | 'tags' | 'issues';
  hierarchy: Partial<HierarchyViewState>;
  lineage: Partial<LineageViewState>;
  matrix: Partial<MatrixViewState>;
  schema: Partial<SchemaViewState>;
  issues: Partial<IssuesViewState>;
}

// ============================================================================
// Defaults
// ============================================================================

const DEFAULT_HIERARCHY_STATE: HierarchyViewState = {
  expandedNodes: [],
  filter: '',
  detailsPanelHeight: 150,
  focusedNodeKey: null,
  unusedExpanded: false,
};

const DEFAULT_MATRIX_STATE: MatrixViewState = {
  filterText: '',
  filterMode: 'rows',
  heatmapMode: false,
  xRayMode: false,
  xRayFilterMode: 'dim',
  clusterMode: false,
  complexityMode: false,
  showLegend: true,
  focusedNode: null,
  firstColumnWidth: 200,
  headerHeight: 160,
};

const DEFAULT_LINEAGE_STATE: LineageViewState = {
  searchTerm: '',
  selectedNodeId: null,
  viewport: null,
};

const DEFAULT_SCHEMA_STATE: SchemaViewState = {
  selectedTableName: null,
};

const DEFAULT_ISSUES_STATE: IssuesViewState = {};

function getDefaultProjectViewStates(): ProjectViewStates {
  return {
    activeTab: 'lineage',
    hierarchy: {},
    lineage: {},
    matrix: {},
    schema: {},
    issues: {},
  };
}

// ============================================================================
// Store
// ============================================================================

interface ViewStateStore {
  viewStates: Record<string, ProjectViewStates>;

  // Getters
  getProjectState: (projectId: string) => ProjectViewStates | undefined;
  getViewState: <K extends keyof Omit<ProjectViewStates, 'activeTab'>>(
    projectId: string,
    view: K
  ) => ProjectViewStates[K] | undefined;
  getActiveTab: (projectId: string) => ProjectViewStates['activeTab'];

  // Setters
  setActiveTab: (projectId: string, tab: ProjectViewStates['activeTab']) => void;
  updateViewState: <K extends keyof Omit<ProjectViewStates, 'activeTab'>>(
    projectId: string,
    view: K,
    state: Partial<ProjectViewStates[K]>
  ) => void;

  // Cleanup
  clearProjectState: (projectId: string) => void;
}

export const useViewStateStore = create<ViewStateStore>()(
  persist(
    (set, get) => ({
      viewStates: {},

      getProjectState: (projectId) => get().viewStates[projectId],

      getViewState: (projectId, view) => get().viewStates[projectId]?.[view],

      getActiveTab: (projectId) =>
        get().viewStates[projectId]?.activeTab ?? 'lineage',

      setActiveTab: (projectId, tab) =>
        set((state) => ({
          viewStates: {
            ...state.viewStates,
            [projectId]: {
              ...getDefaultProjectViewStates(),
              ...state.viewStates[projectId],
              activeTab: tab,
            },
          },
        })),

      updateViewState: (projectId, view, viewState) =>
        set((state) => ({
          viewStates: {
            ...state.viewStates,
            [projectId]: {
              ...getDefaultProjectViewStates(),
              ...state.viewStates[projectId],
              [view]: {
                ...state.viewStates[projectId]?.[view],
                ...viewState,
              },
            },
          },
        })),

      clearProjectState: (projectId) =>
        set((state) => {
          const { [projectId]: _removed, ...rest } = state.viewStates;
          void _removed;
          return { viewStates: rest };
        }),
    }),
    {
      name: 'flowscope-view-states',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ viewStates: state.viewStates }),
    }
  )
);

// ============================================================================
// Utility Hooks
// ============================================================================

/**
 * Get the full hierarchy state with defaults applied
 */
export function getHierarchyStateWithDefaults(
  stored: Partial<HierarchyViewState> | undefined
): HierarchyViewState {
  return {
    ...DEFAULT_HIERARCHY_STATE,
    ...stored,
  };
}

/**
 * Get the full matrix state with defaults applied
 */
export function getMatrixStateWithDefaults(
  stored: Partial<MatrixViewState> | undefined
): MatrixViewState {
  return {
    ...DEFAULT_MATRIX_STATE,
    ...stored,
  };
}

/**
 * Get the full lineage state with defaults applied
 */
export function getLineageStateWithDefaults(
  stored: Partial<LineageViewState> | undefined
): LineageViewState {
  return {
    ...DEFAULT_LINEAGE_STATE,
    ...stored,
  };
}

/**
 * Get the full schema state with defaults applied
 */
export function getSchemaStateWithDefaults(
  stored: Partial<SchemaViewState> | undefined
): SchemaViewState {
  return {
    ...DEFAULT_SCHEMA_STATE,
    ...stored,
  };
}

/**
 * Get the full issues state with defaults applied
 */
export function getIssuesStateWithDefaults(
  stored: Partial<IssuesViewState> | undefined
): IssuesViewState {
  return {
    ...DEFAULT_ISSUES_STATE,
    ...stored,
  };
}
