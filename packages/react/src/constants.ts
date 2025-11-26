/**
 * UI Constants for Flowscope React Components
 */

/**
 * Maximum character length for filter expression display before truncation
 */
export const MAX_FILTER_DISPLAY_LENGTH = 40;

/**
 * Human-readable labels for SQL JOIN types.
 * Keys match the API format (e.g., LEFT_SEMI), values are display labels.
 */
export const JOIN_TYPE_LABELS: Record<string, string> = {
  INNER: 'Inner Join',
  LEFT: 'Left Join',
  RIGHT: 'Right Join',
  FULL: 'Full Join',
  CROSS: 'Cross Join',
  LEFT_SEMI: 'Left Semi',
  RIGHT_SEMI: 'Right Semi',
  LEFT_ANTI: 'Left Anti',
  RIGHT_ANTI: 'Right Anti',
  LEFT_MARK: 'Left Mark',
  CROSS_APPLY: 'Cross Apply',
  OUTER_APPLY: 'Outer Apply',
  AS_OF: 'As Of',
};

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
      accent: '#4957C1', // PondPilot brand blue - for minimap and icons
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
    view: {
      bg: '#EFF6FF',
      headerBg: '#DBEAFE',
      border: '#93C5FD',
      text: '#1E40AF',
      textSecondary: '#3B82F6',
      accent: '#3B82F6', // Blue
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

  // Semantic status colors (aligned with PondPilot)
  status: {
    error: '#EF486F', // PondPilot magenta
    errorBg: '#FDE5EB',
    warning: '#F4A462', // PondPilot orange
    warningBg: '#FDF2E9',
    info: '#4957C1', // PondPilot brand blue
    infoBg: '#E5E7F6',
    success: '#4CAE4F', // PondPilot green
    successBg: '#E6F4E6',
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
  filter: '#059669', // Emerald - filter predicates
  // WCAG AA compliant colors for badges (minimum 4.5:1 contrast ratio on light backgrounds)
  aggregation: '#B45309', // Amber-700 - aggregation/GROUP BY (5.0:1 contrast)
  groupingKey: '#1D4ED8', // Blue-700 - GROUP BY key columns (7.2:1 contrast)
} as const;

/**
 * Dark mode color overrides (aligned with PondPilot blue-grey palette)
 */
export const COLORS_DARK = {
  nodes: {
    table: {
      bg: '#242B35', // PondPilot background.primary.dark
      headerBg: '#384252', // PondPilot blue-grey-700
      border: '#5B6B86', // PondPilot border.dark
      text: '#FDFDFD', // PondPilot text.primary.dark
      textSecondary: '#A8B3C4', // PondPilot text.secondary.dark
      accent: '#4C61FF', // PondPilot accent.dark
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
      accent: '#75C277', // PondPilot success.dark
    },
    script: {
      bg: '#431407',
      headerBg: '#7C2D12',
      border: '#EA580C',
      text: '#FFEDD5',
      textSecondary: '#FED7AA',
      accent: '#F7B987', // PondPilot warning.dark
    },
    view: {
      bg: '#172554', // Blue-950
      headerBg: '#1E3A8A', // Blue-900
      border: '#3B82F6', // Blue-500
      text: '#DBEAFE', // Blue-100
      textSecondary: '#93C5FD', // Blue-300
      accent: '#60A5FA', // Blue-400
    },
  },
  edges: {
    dataFlow: '#8292AA', // PondPilot blue-grey-500
    derivation: '#A78BFA',
    aggregation: '#F7B987', // PondPilot warning.dark
    highlighted: '#4C61FF', // PondPilot accent.dark
    muted: '#5B6B86', // PondPilot blue-grey-600
  },
  status: {
    error: '#F37391', // PondPilot error.dark
    errorBg: '#990D2E',
    warning: '#F7B987', // PondPilot warning.dark
    warningBg: '#A8520C',
    info: '#4C61FF', // PondPilot accent.dark
    infoBg: '#1B255A',
    success: '#75C277', // PondPilot success.dark
    successBg: '#2B612C',
  },
  interactive: {
    selection: '#4C61FF', // PondPilot accent.dark
    selectionRing: '#4C61FF40',
    hover: '#4C61FF15',
    related: '#4C61FF25',
    focus: '#4C61FF',
  },
  recursive: '#F7B987', // PondPilot warning.dark
  accent: '#4C61FF', // PondPilot accent.dark
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
    case 'view':
      return COLORS.nodes.view.accent;
    case 'table':
    default:
      return COLORS.nodes.table.accent;
  }
}
