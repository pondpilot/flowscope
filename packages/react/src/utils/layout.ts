import dagre from 'dagre';
import ELK from 'elkjs/lib/elk.bundled.js';
import type { Node, Edge } from '@xyflow/react';

export type LayoutAlgorithm = 'dagre' | 'elk';

// Layout constants
const NODE_WIDTH = 200;
const NODE_HEIGHT_BASE = 50;
const NODE_HEIGHT_PER_COLUMN = 24;
const NODE_HEIGHT_FILTERS_BASE = 30; // Header + padding for filters section
const NODE_HEIGHT_PER_FILTER = 17; // Each filter line
const DAGRE_NODESEP_LR = 100;
const DAGRE_RANKSEP_LR = 150;
const DAGRE_EDGESEP = 50;
const DAGRE_MARGIN_X = 40;
const DAGRE_MARGIN_Y = 40;

// ELK instance
const elk = new ELK();

interface NodeData extends Record<string, unknown> {
  columns?: { id: string }[];
  filters?: { expression: string }[];
  isCollapsed?: boolean;
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
