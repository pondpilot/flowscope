import type { Edge as FlowEdge, Node as FlowNode } from '@xyflow/react';
import type { TableNodeData, ScriptNodeData } from '../types';

/**
 * Type guard for TableNodeData.
 * Checks if a node's data has the structure of a table/view/CTE node.
 */
export function isTableNodeData(data: unknown): data is TableNodeData {
  return (
    typeof data === 'object' &&
    data !== null &&
    'label' in data &&
    'nodeType' in data &&
    'columns' in data
  );
}

/**
 * Determines whether a node should be included in a filtered graph.
 * A node is included if its ID is in the highlight set, or if any of its columns are highlighted.
 */
export function shouldIncludeNode(node: FlowNode, highlightIds: Set<string>): boolean {
  if (highlightIds.has(node.id)) return true;

  if (isTableNodeData(node.data)) {
    return node.data.columns.some((col) => highlightIds.has(col.id));
  }

  return false;
}

/**
 * Determines whether an edge should be included in a filtered graph.
 * An edge is included if its source, target, or any of their handles are in the highlight set.
 */
export function shouldIncludeEdge(edge: FlowEdge, highlightIds: Set<string>): boolean {
  const sourceId = edge.sourceHandle || edge.source;
  const targetId = edge.targetHandle || edge.target;
  return highlightIds.has(sourceId) || highlightIds.has(targetId) || highlightIds.has(edge.id);
}

/**
 * Filter graph to only include nodes and edges in the highlight set.
 * Used when focus mode is enabled to show only lineage-related elements.
 */
export function filterGraphToHighlights(
  graph: { nodes: FlowNode[]; edges: FlowEdge[] },
  highlightIds: Set<string>
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const filteredNodes = graph.nodes.filter((node) => shouldIncludeNode(node, highlightIds));
  const filteredEdges = graph.edges.filter((edge) => shouldIncludeEdge(edge, highlightIds));
  return { nodes: filteredNodes, edges: filteredEdges };
}

/**
 * Find all node/column IDs that match the search term based on view mode.
 * - Column view: returns matching column IDs
 * - Table view: returns matching table node IDs
 * - Script view: returns matching script and table node IDs
 */
export function findSearchMatchIds(
  searchTerm: string,
  nodes: FlowNode[],
  viewMode: 'table' | 'column' | 'script'
): Set<string> {
  const matchIds = new Set<string>();
  if (!searchTerm) return matchIds;

  const lowerSearch = searchTerm.toLowerCase();

  for (const node of nodes) {
    if (viewMode === 'column') {
      // In column view, match individual columns and add their IDs
      const data = node.data as TableNodeData;
      if (data?.columns) {
        for (const col of data.columns) {
          if (col.name.toLowerCase().includes(lowerSearch)) {
            matchIds.add(col.id);
          }
        }
      }
      // Also match table names - add all column IDs from matching tables
      if (data?.label?.toLowerCase().includes(lowerSearch) && data?.columns) {
        for (const col of data.columns) {
          matchIds.add(col.id);
        }
      }
    } else if (viewMode === 'table') {
      // In table view, match table/CTE/view names
      const data = node.data as TableNodeData;
      if (data?.label?.toLowerCase().includes(lowerSearch)) {
        matchIds.add(node.id);
      }
    } else if (viewMode === 'script') {
      // In script view, match script names and table names
      if (node.type === 'scriptNode') {
        const data = node.data as ScriptNodeData;
        if (data?.label?.toLowerCase().includes(lowerSearch)) {
          matchIds.add(node.id);
        }
      } else if (node.type === 'simpleTableNode') {
        const data = node.data as TableNodeData;
        if (data?.label?.toLowerCase().includes(lowerSearch)) {
          matchIds.add(node.id);
        }
      }
    }
  }

  return matchIds;
}

/**
 * Find connected elements for multiple start IDs.
 * Returns union of all connected elements.
 */
export function findConnectedElementsMultiple(
  startIds: Set<string>,
  edges: FlowEdge[]
): Set<string> {
  const allConnected = new Set<string>();
  for (const startId of startIds) {
    const connected = findConnectedElements(startId, edges);
    for (const id of connected) {
      allConnected.add(id);
    }
  }
  return allConnected;
}

/**
 * Traverse the graph to find all connected elements (nodes/edges) upstream and downstream.
 * Uses bidirectional BFS to find all elements in the data lineage path.
 * @param startId The ID of the node or column to start the traversal from
 * @param edges All edges in the graph
 * @returns Set of all connected element IDs (nodes, columns, and edges)
 */
export function findConnectedElements(
  startId: string,
  edges: FlowEdge[]
): Set<string> {
  const visited = new Set<string>();
  visited.add(startId);

  // Build adjacency maps for efficient graph traversal
  const downstreamMap = new Map<string, string[]>(); // source -> edge IDs
  const upstreamMap = new Map<string, string[]>();   // target -> edge IDs
  const edgeMap = new Map<string, FlowEdge>();       // edge ID -> edge object

  edges.forEach(edge => {
    edgeMap.set(edge.id, edge);

    // Map using handles (column IDs) as these are the true nodes in column view
    // Falls back to source/target for table-level views
    const source = edge.sourceHandle || edge.source;
    const target = edge.targetHandle || edge.target;

    if (!downstreamMap.has(source)) downstreamMap.set(source, []);
    downstreamMap.get(source)?.push(edge.id);

    if (!upstreamMap.has(target)) upstreamMap.set(target, []);
    upstreamMap.get(target)?.push(edge.id);
  });

  // Forward traversal: Find all downstream consumers (following data flow direction)
  const forwardQueue = [startId];
  const forwardVisited = new Set<string>([startId]);
  while (forwardQueue.length > 0) {
    const currentId = forwardQueue.shift()!;

    if (edgeMap.has(currentId)) {
      // Current element is an edge - traverse to its target node/column
      const edge = edgeMap.get(currentId)!;
      const target = edge.targetHandle || edge.target;
      if (!forwardVisited.has(target)) {
        forwardVisited.add(target);
        visited.add(target);
        forwardQueue.push(target);
      }
    } else {
      // Current element is a node/column - find all outgoing edges
      const outgoingEdges = downstreamMap.get(currentId) || [];
      outgoingEdges.forEach(edgeId => {
        if (!forwardVisited.has(edgeId)) {
          forwardVisited.add(edgeId);
          visited.add(edgeId);
          forwardQueue.push(edgeId);
        }
      });
    }
  }

  // Backward traversal: Find all upstream sources (reverse data flow direction)
  const backwardQueue = [startId];
  const backwardVisited = new Set<string>([startId]);
  while (backwardQueue.length > 0) {
    const currentId = backwardQueue.shift()!;

    if (edgeMap.has(currentId)) {
      // Current element is an edge - traverse to its source node/column
      const edge = edgeMap.get(currentId)!;
      const source = edge.sourceHandle || edge.source;
      if (!backwardVisited.has(source)) {
        backwardVisited.add(source);
        visited.add(source);
        backwardQueue.push(source);
      }
    } else {
      // Current element is a node/column - find all incoming edges
      const incomingEdges = upstreamMap.get(currentId) || [];
      incomingEdges.forEach(edgeId => {
        if (!backwardVisited.has(edgeId)) {
          backwardVisited.add(edgeId);
          visited.add(edgeId);
          backwardQueue.push(edgeId);
        }
      });
    }
  }

  return visited;
}
