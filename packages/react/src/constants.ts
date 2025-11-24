/**
 * UI Constants for Flowscope React Components
 */

export const UI_CONSTANTS = {
  /** Delay in milliseconds before showing tooltips */
  TOOLTIP_DELAY: 300,

  /** Delay in milliseconds for node tooltips (fast display) */
  TOOLTIP_DELAY_NODE: 200,

  /** Delay in milliseconds for debouncing search input */
  SEARCH_DEBOUNCE_DELAY: 300,

  /** Minimum width in pixels for search input */
  SEARCH_MIN_WIDTH: 240,

  /** Maximum height in pixels for column lists in table nodes */
  COLUMN_MAX_HEIGHT: 1000,

  /** Z-index for highlighted edges */
  HIGHLIGHTED_EDGE_Z_INDEX: 1000,

  /** Delay in milliseconds for tooltip display (fast display) */
  TOOLTIP_DELAY_FAST: 0,

  /** Maximum number of tables to show in tooltips before truncating */
  MAX_TOOLTIP_TABLES: 5,

  /** Maximum number of shared tables to show in edge labels */
  MAX_EDGE_LABEL_TABLES: 2,
} as const;

/**
 * Graph-specific configuration
 */
export const GRAPH_CONFIG = {
  MAX_COLUMN_HEIGHT: UI_CONSTANTS.COLUMN_MAX_HEIGHT,
  TOOLTIP_DELAY: UI_CONSTANTS.TOOLTIP_DELAY_FAST,
  HIGHLIGHTED_EDGE_Z_INDEX: UI_CONSTANTS.HIGHLIGHTED_EDGE_Z_INDEX,
  VIRTUAL_OUTPUT_NODE_ID: 'virtual:output',
} as const;

/**
 * Unified color palette for graph nodes and UI elements
 * All colors are defined here for consistency across views
 */
export const COLORS = {
  // Node type palettes
  nodes: {
    table: {
      bg: '#FFFFFF',
      headerBg: '#F2F4F8',
      border: '#DBDDE1',
      text: '#212328',
      textSecondary: '#6F7785',
      accent: '#3B82F6', // Blue - for minimap and icons
    },
    cte: {
      bg: '#F5F3FF',
      headerBg: '#EDE9FE',
      border: '#C4B5FD',
      text: '#5B21B6',
      textSecondary: '#7C3AED',
      accent: '#8B5CF6', // Purple
    },
    virtualOutput: {
      bg: '#F0FDF4',
      headerBg: '#DCFCE7',
      border: '#6EE7B7',
      text: '#047857',
      textSecondary: '#065F46',
      accent: '#10B981', // Green
    },
    script: {
      bg: '#FFF7ED',
      headerBg: '#FFEDD5',
      border: '#FDBA74',
      text: '#9A3412',
      textSecondary: '#C2410C',
      accent: '#F97316', // Orange
    },
  },

  // Edge type colors
  edges: {
    dataFlow: '#94A3B8', // Slate gray - direct data movement
    derivation: '#8B5CF6', // Purple - transformation
    aggregation: '#F59E0B', // Amber - GROUP BY / aggregates
    highlighted: '#4C61FF', // Bright blue - selected
    muted: '#CBD5E1', // Light gray - dimmed edges
  },

  // Semantic status colors
  status: {
    error: '#EF4444',
    errorBg: '#FEE2E2',
    warning: '#F59E0B',
    warningBg: '#FEF3C7',
    info: '#3B82F6',
    infoBg: '#DBEAFE',
    success: '#22C55E',
    successBg: '#DCFCE7',
  },

  // Interactive state colors
  interactive: {
    selection: '#4C61FF',
    selectionRing: '#4C61FF40', // 40% opacity
    hover: '#4C61FF15', // 15% opacity
    related: '#4C61FF25', // 25% opacity
    focus: '#4C61FF',
  },

  // Special indicators
  recursive: '#F59E0B',
  accent: '#4C61FF',

  // Legacy aliases for backwards compatibility
  table: {
    bg: '#FFFFFF',
    headerBg: '#F2F4F8',
    border: '#DBDDE1',
    text: '#212328',
    textSecondary: '#6F7785',
  },
  cte: {
    bg: '#F5F3FF',
    headerBg: '#EDE9FE',
    border: '#C4B5FD',
    text: '#5B21B6',
    textSecondary: '#7C3AED',
  },
  virtualOutput: {
    bg: '#F0FDF4',
    headerBg: '#DCFCE7',
    border: '#6EE7B7',
    text: '#047857',
    textSecondary: '#065F46',
  },
} as const;

/**
 * Dark mode color overrides
 */
export const COLORS_DARK = {
  nodes: {
    table: {
      bg: '#1E293B',
      headerBg: '#334155',
      border: '#475569',
      text: '#F1F5F9',
      textSecondary: '#94A3B8',
      accent: '#60A5FA',
    },
    cte: {
      bg: '#2E1065',
      headerBg: '#3B0764',
      border: '#6D28D9',
      text: '#E9D5FF',
      textSecondary: '#C4B5FD',
      accent: '#A78BFA',
    },
    virtualOutput: {
      bg: '#052E16',
      headerBg: '#14532D',
      border: '#16A34A',
      text: '#DCFCE7',
      textSecondary: '#BBF7D0',
      accent: '#34D399',
    },
    script: {
      bg: '#431407',
      headerBg: '#7C2D12',
      border: '#EA580C',
      text: '#FFEDD5',
      textSecondary: '#FED7AA',
      accent: '#FB923C',
    },
  },
  edges: {
    dataFlow: '#64748B',
    derivation: '#A78BFA',
    aggregation: '#FBBF24',
    highlighted: '#818CF8',
    muted: '#475569',
  },
  status: {
    error: '#F87171',
    errorBg: '#450A0A',
    warning: '#FBBF24',
    warningBg: '#451A03',
    info: '#60A5FA',
    infoBg: '#172554',
    success: '#4ADE80',
    successBg: '#052E16',
  },
  interactive: {
    selection: '#818CF8',
    selectionRing: '#818CF840',
    hover: '#818CF815',
    related: '#818CF825',
    focus: '#818CF8',
  },
  recursive: '#FBBF24',
  accent: '#818CF8',
} as const;

/**
 * Edge style configurations
 */
export const EDGE_STYLES = {
  dataFlow: {
    stroke: COLORS.edges.dataFlow,
    strokeWidth: 2,
    strokeDasharray: undefined, // Solid line
  },
  derivation: {
    stroke: COLORS.edges.derivation,
    strokeWidth: 2,
    strokeDasharray: '6 4', // Dashed line
  },
  aggregation: {
    stroke: COLORS.edges.aggregation,
    strokeWidth: 2,
    strokeDasharray: '2 2', // Dotted line
  },
  highlighted: {
    stroke: COLORS.edges.highlighted,
    strokeWidth: 3,
    strokeDasharray: undefined,
  },
} as const;

/**
 * Get minimap node color based on node type
 */
export function getMinimapNodeColor(nodeType: string): string {
  switch (nodeType) {
    case 'cte':
      return COLORS.nodes.cte.accent;
    case 'script':
      return COLORS.nodes.script.accent;
    case 'virtualOutput':
    case 'output':
      return COLORS.nodes.virtualOutput.accent;
    case 'table':
    default:
      return COLORS.nodes.table.accent;
  }
}
