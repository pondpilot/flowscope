/**
 * UI Constants for Flowscope React Components
 */

export const UI_CONSTANTS = {
  /** Delay in milliseconds before showing tooltips */
  TOOLTIP_DELAY: 300,

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
 * Color palette for graph nodes and UI elements
 */
export const COLORS = {
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
  accent: '#4C61FF',
} as const;
