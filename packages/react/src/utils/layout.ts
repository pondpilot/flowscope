import dagre from 'dagre';
import type { Node, Edge } from '@xyflow/react';

// Layout constants
const NODE_WIDTH = 200;
const NODE_HEIGHT_BASE = 50;
const NODE_HEIGHT_PER_COLUMN = 24;
const DAGRE_NODESEP_LR = 100;
const DAGRE_RANKSEP_LR = 150;
const DAGRE_EDGESEP = 50;
const DAGRE_MARGIN_X = 40;
const DAGRE_MARGIN_Y = 40;

interface NodeData extends Record<string, unknown> {
  columns?: { id: string }[];
}

/**
 * Arranges nodes in a directed graph layout using dagre algorithm
 */
export function getLayoutedElements<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB' = 'LR'
): { nodes: Node<N>[]; edges: Edge<E>[] } {
  if (nodes.length === 0) return { nodes, edges };

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
    const height = NODE_HEIGHT_BASE + (node.data?.columns?.length || 0) * NODE_HEIGHT_PER_COLUMN;
    dagreGraph.setNode(node.id, { width: NODE_WIDTH, height });
  });

  edges.forEach((edge) => {
    dagreGraph.setEdge(edge.source, edge.target);
  });

  dagre.layout(dagreGraph);

  const layoutedNodes = nodes.map((node) => {
    const nodeWithPosition = dagreGraph.node(node.id);
    if (!nodeWithPosition) return node;

    const height = NODE_HEIGHT_BASE + (node.data?.columns?.length || 0) * NODE_HEIGHT_PER_COLUMN;
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
