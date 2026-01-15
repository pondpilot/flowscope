/**
 * Shared layout constants used by both the main thread layout
 * and the Web Worker layout computation.
 */

// Node dimensions
export const NODE_WIDTH = 200;
export const NODE_HEIGHT_BASE = 50;
export const NODE_HEIGHT_PER_COLUMN = 24;
export const NODE_HEIGHT_FILTERS_BASE = 30; // Header + padding for filters section
export const NODE_HEIGHT_PER_FILTER = 17; // Each filter line

// Dagre layout settings
export const DAGRE_NODESEP_LR = 100;
export const DAGRE_RANKSEP_LR = 150;
export const DAGRE_EDGESEP = 50;
export const DAGRE_MARGIN_X = 40;
export const DAGRE_MARGIN_Y = 40;

// Fast layout settings
export const FAST_LAYOUT_GAP_X = 120;
export const FAST_LAYOUT_GAP_Y = 80;
