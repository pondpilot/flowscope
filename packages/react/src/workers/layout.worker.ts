/**
 * Web Worker for graph layout computation.
 * Runs dagre layout off the main thread to prevent UI blocking.
 *
 * Note: ELK layout is NOT supported in the worker because ELK.js tries to spawn
 * its own internal web worker, which fails in a nested worker context.
 * ELK requests are handled on the main thread instead (see layout.ts).
 */
import dagre from 'dagre';
import {
  NODE_WIDTH,
  NODE_HEIGHT_BASE,
  NODE_HEIGHT_PER_COLUMN,
  NODE_HEIGHT_FILTERS_BASE,
  NODE_HEIGHT_PER_FILTER,
  DAGRE_NODESEP_LR,
  DAGRE_RANKSEP_LR,
  DAGRE_EDGESEP,
  DAGRE_MARGIN_X,
  DAGRE_MARGIN_Y,
} from '../utils/layoutConstants';

// Debug flag - workers can't import from debug.ts due to bundling,
// so we replicate the pattern here
const LAYOUT_WORKER_DEBUG = !!(import.meta as { env?: { DEV?: boolean } }).env?.DEV;

export type LayoutAlgorithm = 'dagre' | 'elk';

/**
 * Serializable node data for worker communication.
 */
export interface WorkerNodeData {
  id: string;
  columnCount: number;
  filterCount: number;
  isCollapsed: boolean;
}

/**
 * Serializable edge data for worker communication.
 */
export interface WorkerEdgeData {
  id: string;
  source: string;
  target: string;
}

/**
 * Request message to the worker.
 */
export interface LayoutRequest {
  type: 'layout';
  requestId: string;
  nodes: WorkerNodeData[];
  edges: WorkerEdgeData[];
  direction: 'LR' | 'TB';
  algorithm: LayoutAlgorithm;
}

/**
 * Response message from the worker.
 */
export interface LayoutResponse {
  type: 'layout-result';
  requestId: string;
  positions: Map<string, { x: number; y: number }> | Record<string, { x: number; y: number }>;
  error?: string;
}

/**
 * Calculate the height of a node based on its content.
 */
function calculateNodeHeight(node: WorkerNodeData): number {
  if (node.isCollapsed) {
    return NODE_HEIGHT_BASE;
  }

  let height = NODE_HEIGHT_BASE;

  if (node.columnCount > 0) {
    height += node.columnCount * NODE_HEIGHT_PER_COLUMN;
  }

  if (node.filterCount > 0) {
    height += NODE_HEIGHT_FILTERS_BASE + node.filterCount * NODE_HEIGHT_PER_FILTER;
  }

  return height;
}

/**
 * Compute layout using Dagre.
 */
function computeDagreLayout(
  nodes: WorkerNodeData[],
  edges: WorkerEdgeData[],
  direction: 'LR' | 'TB'
): Record<string, { x: number; y: number }> {
  const dagreGraph = new dagre.graphlib.Graph();
  dagreGraph.setDefaultEdgeLabel(() => ({}));

  dagreGraph.setGraph({
    rankdir: direction,
    nodesep: DAGRE_NODESEP_LR,
    ranksep: DAGRE_RANKSEP_LR,
    edgesep: DAGRE_EDGESEP,
    marginx: DAGRE_MARGIN_X,
    marginy: DAGRE_MARGIN_Y,
  });

  for (const node of nodes) {
    const height = calculateNodeHeight(node);
    dagreGraph.setNode(node.id, { width: NODE_WIDTH, height });
  }

  for (const edge of edges) {
    dagreGraph.setEdge(edge.source, edge.target);
  }

  dagre.layout(dagreGraph);

  const positions: Record<string, { x: number; y: number }> = {};
  for (const node of nodes) {
    const nodeWithPosition = dagreGraph.node(node.id);
    if (nodeWithPosition) {
      const height = calculateNodeHeight(node);
      positions[node.id] = {
        x: nodeWithPosition.x - NODE_WIDTH / 2,
        y: nodeWithPosition.y - height / 2,
      };
    }
  }

  return positions;
}

// Log worker initialization
if (LAYOUT_WORKER_DEBUG) {
  console.log('[Layout Worker] Worker initialized');
}

// Worker message handler
self.onmessage = async (event: MessageEvent<LayoutRequest>) => {
  const { type, requestId, nodes, edges, direction, algorithm } = event.data;

  if (type !== 'layout') {
    return;
  }

  if (LAYOUT_WORKER_DEBUG) {
    console.log(`[Layout Worker] Received request ${requestId}: ${nodes.length} nodes, ${edges.length} edges`);
  }
  const startTime = performance.now();

  try {
    // ELK is not supported in worker - it tries to spawn nested workers which fail
    if (algorithm === 'elk') {
      throw new Error('ELK layout not supported in worker');
    }

    const positions = computeDagreLayout(nodes, edges, direction);
    const duration = performance.now() - startTime;
    if (LAYOUT_WORKER_DEBUG) {
      console.log(`[Layout Worker] Dagre layout completed in ${duration.toFixed(2)}ms`);
    }

    const response: LayoutResponse = {
      type: 'layout-result',
      requestId,
      positions,
    };

    self.postMessage(response);
  } catch (error) {
    console.error('[Layout Worker] Error:', error);
    const response: LayoutResponse = {
      type: 'layout-result',
      requestId,
      positions: {},
      error: error instanceof Error ? error.message : 'Unknown error',
    };

    self.postMessage(response);
  }
};
