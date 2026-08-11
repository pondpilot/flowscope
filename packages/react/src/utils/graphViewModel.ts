import type { Edge as FlowEdge, Node as FlowNode } from '@xyflow/react';

import { GRAPH_CONFIG } from '../constants';
import type { LineageViewMode } from '../types';
import type { SearchableType } from '../hooks/useSearchSuggestions';
import { getFastLayoutedNodes } from './layout';
import { isTableNodeData } from './graphTraversal';

const MINIMAP_NODE_LIMIT = 2000;

/**
 * Threshold for determining when to treat a graph as "new" vs "evolved".
 * If fewer than this fraction of nodes have existing positions, we use
 * fast layout for all nodes instead of preserving positions.
 */
const NODE_OVERLAP_THRESHOLD = 0.5;

export interface GraphElements {
  nodes: FlowNode[];
  edges: FlowEdge[];
}

export interface RenderGraphIndex {
  nodeDataById: Map<string, FlowNode['data']>;
  edgeById: Map<string, FlowEdge>;
}

interface SelectableNodeData {
  isSelected?: boolean;
  [key: string]: unknown;
}

function isSelectableNodeData(data: unknown): data is SelectableNodeData {
  return typeof data === 'object' && data !== null;
}

export function getGraphSearchableTypes(
  viewMode: LineageViewMode,
  showColumnEdges: boolean
): SearchableType[] {
  if (viewMode === 'script') {
    return ['script', 'table', 'view', 'cte'];
  }

  return showColumnEdges
    ? ['table', 'view', 'cte', 'column', 'script']
    : ['table', 'view', 'cte', 'script'];
}

export function enhanceGraphWithHighlights(
  graph: GraphElements,
  highlightIds: ReadonlySet<string>
): GraphElements {
  const nodes = graph.nodes.map((node) => {
    const isHighlighted = highlightIds.has(node.id);

    if (isTableNodeData(node.data)) {
      const nodeData = node.data;
      const columns = nodeData.columns.map((column) => ({
        ...column,
        isHighlighted: highlightIds.has(column.id),
      }));

      return {
        ...node,
        data: {
          ...nodeData,
          columns,
          isSelected: nodeData.isSelected || isHighlighted,
        },
      };
    }

    const currentIsSelected = isSelectableNodeData(node.data) ? node.data.isSelected : false;
    return {
      ...node,
      data: {
        ...node.data,
        isSelected: currentIsSelected || isHighlighted,
      },
    };
  });

  const edges = graph.edges.map((edge) => {
    const isHighlighted = highlightIds.has(edge.id);
    return {
      ...edge,
      animated: isHighlighted,
      zIndex: isHighlighted ? GRAPH_CONFIG.HIGHLIGHTED_EDGE_Z_INDEX : 0,
      data: {
        ...edge.data,
        isHighlighted,
      },
    };
  });

  return { nodes, edges };
}

export function createRenderGraphIndex(graph: GraphElements): RenderGraphIndex {
  return {
    nodeDataById: new Map(graph.nodes.map((node) => [node.id, node.data])),
    edgeById: new Map(graph.edges.map((edge) => [edge.id, edge])),
  };
}

export function prepareProgressiveLayoutNodes(
  currentNodes: FlowNode[],
  renderNodes: FlowNode[],
  direction: 'LR' | 'TB'
): FlowNode[] {
  if (currentNodes.length === 0) {
    return getFastLayoutedNodes(renderNodes, direction);
  }

  const positionById = new Map(currentNodes.map((node) => [node.id, node.position]));
  const nodesWithoutPosition = renderNodes.filter((node) => !positionById.has(node.id));
  const matchCount = renderNodes.length - nodesWithoutPosition.length;

  if (matchCount < renderNodes.length * NODE_OVERLAP_THRESHOLD) {
    return getFastLayoutedNodes(renderNodes, direction);
  }

  if (nodesWithoutPosition.length === 0) {
    return renderNodes.map((node) => ({
      ...node,
      position: positionById.get(node.id)!,
    }));
  }

  const fastLayoutNodes = getFastLayoutedNodes(renderNodes, direction);
  const fastPositionById = new Map(fastLayoutNodes.map((node) => [node.id, node.position]));

  return renderNodes.map((node) => ({
    ...node,
    position: positionById.get(node.id) ?? fastPositionById.get(node.id) ?? { x: 0, y: 0 },
  }));
}

export function applyRenderDataToNodes(
  nodes: FlowNode[],
  nodeDataById: RenderGraphIndex['nodeDataById'],
  currentNodes?: FlowNode[]
): FlowNode[] {
  const currentPositionById = currentNodes
    ? new Map(currentNodes.map((node) => [node.id, node.position]))
    : null;

  return nodes.map((node) => {
    const renderData = nodeDataById.get(node.id);
    const currentPosition = currentPositionById?.get(node.id);

    if (currentPosition) {
      return {
        ...node,
        position: currentPosition,
        data: renderData ?? node.data,
      };
    }

    if (!renderData || node.data === renderData) {
      return node;
    }

    return {
      ...node,
      data: renderData,
    };
  });
}

export function applyRenderDataToEdges(
  edges: FlowEdge[],
  edgeById: RenderGraphIndex['edgeById']
): FlowEdge[] {
  return edges.map((edge) => {
    const renderEdge = edgeById.get(edge.id);
    if (!renderEdge || edge === renderEdge) return edge;

    return {
      ...edge,
      type: renderEdge.type,
      label: renderEdge.label,
      animated: renderEdge.animated,
      zIndex: renderEdge.zIndex,
      data: renderEdge.data,
      style: renderEdge.style,
    };
  });
}

export function hasNodeCollapseChanged(
  nodes: FlowNode[],
  previousCollapseStates: ReadonlyMap<string, boolean>
): boolean {
  return nodes.some((node) => {
    if (!isTableNodeData(node.data)) return false;
    const previousCollapsed = previousCollapseStates.get(node.id);
    return (
      previousCollapsed !== undefined && previousCollapsed !== (node.data.isCollapsed ?? false)
    );
  });
}

export function getNodeCollapseStates(nodes: FlowNode[]): Map<string, boolean> {
  const collapseStates = new Map<string, boolean>();
  for (const node of nodes) {
    if (isTableNodeData(node.data)) {
      collapseStates.set(node.id, node.data.isCollapsed ?? false);
    }
  }
  return collapseStates;
}

export function shouldShowMiniMap(nodeCount: number): boolean {
  return nodeCount > 0 && nodeCount <= MINIMAP_NODE_LIMIT;
}
