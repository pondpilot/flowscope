import dagre from 'dagre';
import ELK from 'elkjs/lib/elk.bundled.js';
import type { Node, Edge } from '@xyflow/react';
import {
  computeLayoutInWorker,
  applyPositionsToNodes,
  isWorkerSupported,
  cancelPendingLayouts,
} from './layoutWorkerService';
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
  FAST_LAYOUT_GAP_X,
  FAST_LAYOUT_GAP_Y,
} from './layoutConstants';

export type LayoutAlgorithm = 'dagre' | 'elk';

const LAYOUT_CACHE_LIMIT = 6;

// ELK instance
const elk = new ELK();

interface LayoutCacheEntry {
  positions: Record<string, { x: number; y: number }>;
  lastUsedAt: number;
}

const layoutCache = new Map<string, LayoutCacheEntry>();

interface NodeData extends Record<string, unknown> {
  columns?: { id: string }[];
  filters?: { expression: string }[];
  isCollapsed?: boolean;
}

function hashString(value: string, seed: number): number {
  let hash = seed;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function buildLayoutCacheKey<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB',
  algorithm: LayoutAlgorithm
): string {
  let hash = 2166136261;
  hash = hashString(algorithm, hash);
  hash = hashString(direction, hash);
  hash = Math.imul(hash ^ nodes.length, 16777619) >>> 0;
  hash = Math.imul(hash ^ edges.length, 16777619) >>> 0;

  for (const node of nodes) {
    hash = hashString(node.id, hash);
    hash = Math.imul(hash ^ (node.data?.isCollapsed ? 1 : 0), 16777619) >>> 0;
    hash = Math.imul(hash ^ (node.data?.columns?.length ?? 0), 16777619) >>> 0;
    hash = Math.imul(hash ^ (node.data?.filters?.length ?? 0), 16777619) >>> 0;
  }

  for (const edge of edges) {
    hash = hashString(edge.source, hash);
    hash = hashString(edge.target, hash);
  }

  return hash.toString(16);
}

function readLayoutCache<N extends NodeData>(nodes: Node<N>[], cacheKey: string): Node<N>[] | null {
  const cached = layoutCache.get(cacheKey);
  if (!cached) {
    return null;
  }

  cached.lastUsedAt = Date.now();
  layoutCache.set(cacheKey, cached);

  return applyPositionsToNodes(nodes, cached.positions);
}

function writeLayoutCache(cacheKey: string, positions: Record<string, { x: number; y: number }>): void {
  if (Object.keys(positions).length === 0) {
    return;
  }

  layoutCache.set(cacheKey, { positions, lastUsedAt: Date.now() });

  if (layoutCache.size <= LAYOUT_CACHE_LIMIT) {
    return;
  }

  const entries = Array.from(layoutCache.entries()).sort((left, right) => left[1].lastUsedAt - right[1].lastUsedAt);
  while (entries.length > LAYOUT_CACHE_LIMIT) {
    const entry = entries.shift();
    if (entry) {
      layoutCache.delete(entry[0]);
    }
  }
}

export function getFastLayoutedNodes<N extends NodeData>(nodes: Node<N>[], direction: 'LR' | 'TB'): Node<N>[] {
  if (nodes.length === 0) {
    return nodes;
  }

  const columns = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
  const rowHeights: number[] = [];

  nodes.forEach((node, index) => {
    const row = Math.floor(index / columns);
    const height = calculateNodeHeight(node.data);
    rowHeights[row] = Math.max(rowHeights[row] ?? 0, height);
  });

  const rowOffsets: number[] = [];
  let currentY = 0;
  rowHeights.forEach((height, rowIndex) => {
    rowOffsets[rowIndex] = currentY;
    currentY += height + FAST_LAYOUT_GAP_Y;
  });

  return nodes.map((node, index) => {
    const row = Math.floor(index / columns);
    const column = index % columns;
    const x = column * (NODE_WIDTH + FAST_LAYOUT_GAP_X);
    const y = rowOffsets[row] ?? 0;

    return {
      ...node,
      position: direction === 'TB' ? { x: y, y: x } : { x, y },
    };
  });
}

/**
 * Calculate the height of a node based on its content.
 */
function calculateNodeHeight(data: NodeData | undefined): number {
  if (!data) return NODE_HEIGHT_BASE;

  // If collapsed, only show header
  if (data.isCollapsed) {
    return NODE_HEIGHT_BASE;
  }

  let height = NODE_HEIGHT_BASE;

  // Add height for columns
  const columnCount = data.columns?.length || 0;
  if (columnCount > 0) {
    height += columnCount * NODE_HEIGHT_PER_COLUMN;
  }

  // Add height for filters section
  const filterCount = data.filters?.length || 0;
  if (filterCount > 0) {
    height += NODE_HEIGHT_FILTERS_BASE + filterCount * NODE_HEIGHT_PER_FILTER;
  }

  return height;
}

/**
 * Arranges nodes using the Dagre algorithm (synchronous)
 */
function layoutWithDagre<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB'
): { nodes: Node<N>[]; edges: Edge<E>[] } {
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

  nodes.forEach((node) => {
    const height = calculateNodeHeight(node.data);
    dagreGraph.setNode(node.id, { width: NODE_WIDTH, height });
  });

  edges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target);
  });

  dagre.layout(dagreGraph);

  const layoutedNodes = nodes.map((node) => {
    const nodeWithPosition = dagreGraph.node(node.id);
    if (!nodeWithPosition) return node;

    const height = calculateNodeHeight(node.data);
    return {
      ...node,
      position: {
        x: nodeWithPosition.x - NODE_WIDTH / 2,
        y: nodeWithPosition.y - height / 2,
      },
    };
  });

  return { nodes: layoutedNodes, edges };
}

/**
 * Arranges nodes using the ELK algorithm (asynchronous)
 */
async function layoutWithElk<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB'
): Promise<{ nodes: Node<N>[]; edges: Edge<E>[] }> {
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
    children: nodes.map((node) => {
      const height = calculateNodeHeight(node.data);
      return {
        id: node.id,
        width: NODE_WIDTH,
        height,
      };
    }),
    edges: edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  };

  const layoutedGraph = await elk.layout(graph);

  const layoutedNodes = nodes.map((node) => {
    const elkNode = layoutedGraph.children?.find((n) => n.id === node.id);
    if (!elkNode) return node;

    return {
      ...node,
      position: {
        x: elkNode.x ?? 0,
        y: elkNode.y ?? 0,
      },
    };
  });

  return { nodes: layoutedNodes, edges };
}

/**
 * Arranges nodes in a directed graph layout using the specified algorithm.
 * Dagre is synchronous, ELK is asynchronous.
 */
export function getLayoutedElements<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB' = 'LR',
  algorithm: LayoutAlgorithm = 'dagre'
): { nodes: Node<N>[]; edges: Edge<E>[] } {
  if (nodes.length === 0) return { nodes, edges };

  // For dagre, return synchronously
  if (algorithm === 'dagre') {
    return layoutWithDagre(nodes, edges, direction);
  }

  // For ELK, return the current positions (async layout will be triggered separately)
  return { nodes, edges };
}

/**
 * Async version for ELK layout - use this when you need ELK specifically.
 */
export async function getLayoutedElementsAsync<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB' = 'LR',
  algorithm: LayoutAlgorithm = 'dagre'
): Promise<{ nodes: Node<N>[]; edges: Edge<E>[] }> {
  if (nodes.length === 0) return { nodes, edges };

  if (algorithm === 'elk') {
    return layoutWithElk(nodes, edges, direction);
  }

  return layoutWithDagre(nodes, edges, direction);
}

/**
 * Compute layout in a Web Worker for non-blocking UI.
 * Falls back to main thread if workers aren't supported.
 *
 * This is the preferred method for large graphs as it keeps the UI responsive.
 */
export async function getLayoutedElementsInWorker<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB' = 'LR',
  algorithm: LayoutAlgorithm = 'dagre'
): Promise<{ nodes: Node<N>[]; edges: Edge<E>[] }> {
  if (nodes.length === 0) return { nodes, edges };

  const cacheKey = buildLayoutCacheKey(nodes, edges, direction, algorithm);
  const cachedNodes = readLayoutCache(nodes, cacheKey);
  if (cachedNodes) {
    return { nodes: cachedNodes, edges };
  }

  // ELK uses its own internal web worker, so run it on main thread directly.
  // Running ELK inside our layout worker would create nested workers which fail.
  if (algorithm === 'elk') {
    const layouted = await getLayoutedElementsAsync(nodes, edges, direction, algorithm);
    const positions: Record<string, { x: number; y: number }> = {};
    layouted.nodes.forEach((node) => {
      positions[node.id] = node.position;
    });
    writeLayoutCache(cacheKey, positions);
    return layouted;
  }

  // Fall back to main thread if workers aren't supported
  if (!isWorkerSupported()) {
    const layouted = await getLayoutedElementsAsync(nodes, edges, direction, algorithm);
    const positions: Record<string, { x: number; y: number }> = {};
    layouted.nodes.forEach((node) => {
      positions[node.id] = node.position;
    });
    writeLayoutCache(cacheKey, positions);
    return layouted;
  }

  try {
    cancelPendingLayouts();
    const positions = await computeLayoutInWorker(nodes, edges, direction, algorithm);
    writeLayoutCache(cacheKey, positions);
    const layoutedNodes = applyPositionsToNodes(nodes, positions);
    return { nodes: layoutedNodes, edges };
  } catch (error) {
    // Fall back to main thread on worker error
    console.warn('[Layout] Worker failed, falling back to main thread:', error);
    const layouted = await getLayoutedElementsAsync(nodes, edges, direction, algorithm);
    const positions: Record<string, { x: number; y: number }> = {};
    layouted.nodes.forEach((node) => {
      positions[node.id] = node.position;
    });
    writeLayoutCache(cacheKey, positions);
    return layouted;
  }
}

export function cancelLayoutRequests(): void {
  cancelPendingLayouts();
}
