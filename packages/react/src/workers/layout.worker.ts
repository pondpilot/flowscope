/**
 * Web Worker for graph layout computation.
 * Runs dagre/elk layout off the main thread to prevent UI blocking.
 */
import dagre from 'dagre';
import ELK from 'elkjs/lib/elk.bundled.js';
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

export type LayoutAlgorithm = 'dagre' | 'elk';

// ELK instance
const elk = new ELK();

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

/**
 * Compute layout using ELK.
 */
async function computeElkLayout(
  nodes: WorkerNodeData[],
  edges: WorkerEdgeData[],
  direction: 'LR' | 'TB'
): Promise<Record<string, { x: number; y: number }>> {
  const elkDirection = direction === 'LR' ? 'RIGHT' : 'DOWN';

  const graph = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': elkDirection,
      'elk.layered.spacing.nodeNodeBetweenLayers': '150',
      'elk.spacing.nodeNode': '80',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.edgeRouting': 'ORTHOGONAL',
    },
    children: nodes.map((node) => ({
      id: node.id,
      width: NODE_WIDTH,
      height: calculateNodeHeight(node),
    })),
    edges: edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  };

  const layoutedGraph = await elk.layout(graph);

  const positions: Record<string, { x: number; y: number }> = {};
  for (const child of layoutedGraph.children || []) {
    positions[child.id] = {
      x: child.x ?? 0,
      y: child.y ?? 0,
    };
  }

  return positions;
}

// Worker message handler
self.onmessage = async (event: MessageEvent<LayoutRequest>) => {
  const { type, requestId, nodes, edges, direction, algorithm } = event.data;

  if (type !== 'layout') {
    return;
  }

  try {
    let positions: Record<string, { x: number; y: number }>;

    if (algorithm === 'elk') {
      positions = await computeElkLayout(nodes, edges, direction);
    } else {
      positions = computeDagreLayout(nodes, edges, direction);
    }

    const response: LayoutResponse = {
      type: 'layout-result',
      requestId,
      positions,
    };

    self.postMessage(response);
  } catch (error) {
    const response: LayoutResponse = {
      type: 'layout-result',
      requestId,
      positions: {},
      error: error instanceof Error ? error.message : 'Unknown error',
    };

    self.postMessage(response);
  }
};
