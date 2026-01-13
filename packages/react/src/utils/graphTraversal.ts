import type { Edge as FlowEdge, Node as FlowNode } from '@xyflow/react';
import type { TableNodeData, ScriptNodeData, TableFilterDirection, TableFilter } from '../types';

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
    'columns' in data &&
    Array.isArray((data as { columns: unknown }).columns)
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

  // Build a set of all valid node IDs after filtering (including column IDs from table nodes)
  const validNodeIds = new Set<string>();
  for (const node of filteredNodes) {
    validNodeIds.add(node.id);
    if (isTableNodeData(node.data)) {
      for (const col of node.data.columns) {
        validNodeIds.add(col.id);
      }
    }
  }

  // Only include edges where both source and target exist in the filtered graph
  const filteredEdges = graph.edges.filter((edge) => {
    const sourceId = edge.sourceHandle || edge.source;
    const targetId = edge.targetHandle || edge.target;
    return validNodeIds.has(sourceId) && validNodeIds.has(targetId);
  });

  return { nodes: filteredNodes, edges: filteredEdges };
}

/**
 * Find all node/column IDs that match the search term based on view mode.
 * - Table view with column edges: returns matching column IDs
 * - Table view without column edges: returns matching table node IDs
 * - Script view: returns matching script and table node IDs
 */
export function findSearchMatchIds(
  searchTerm: string,
  nodes: FlowNode[],
  viewMode: 'table' | 'script',
  showColumnEdges: boolean = false
): Set<string> {
  const matchIds = new Set<string>();
  if (!searchTerm) return matchIds;

  const lowerSearch = searchTerm.toLowerCase();

  for (const node of nodes) {
    if (viewMode === 'table' && showColumnEdges) {
      // With column edges, match individual columns and add their IDs
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

/**
 * Traverse the graph to find connected elements in a specific direction.
 * @param startId The ID of the node to start the traversal from
 * @param edges All edges in the graph
 * @param direction Which direction to traverse: 'upstream', 'downstream', or 'both'
 * @returns Set of all connected element IDs in the specified direction
 */
export function findConnectedElementsDirectional(
  startId: string,
  edges: FlowEdge[],
  direction: TableFilterDirection
): Set<string> {
  if (!startId || !edges || !direction) {
    return new Set(startId ? [startId] : []);
  }

  const visited = new Set<string>();
  visited.add(startId);

  // Build adjacency maps for efficient graph traversal
  const downstreamMap = new Map<string, string[]>(); // source -> edge IDs
  const upstreamMap = new Map<string, string[]>();   // target -> edge IDs
  const edgeMap = new Map<string, FlowEdge>();       // edge ID -> edge object

  edges.forEach(edge => {
    edgeMap.set(edge.id, edge);

    const source = edge.sourceHandle || edge.source;
    const target = edge.targetHandle || edge.target;

    if (!downstreamMap.has(source)) downstreamMap.set(source, []);
    downstreamMap.get(source)?.push(edge.id);

    if (!upstreamMap.has(target)) upstreamMap.set(target, []);
    upstreamMap.get(target)?.push(edge.id);
  });

  // Forward traversal (downstream): Find all consumers
  if (direction === 'downstream' || direction === 'both') {
    const forwardQueue = [startId];
    const forwardVisited = new Set<string>([startId]);
    while (forwardQueue.length > 0) {
      const currentId = forwardQueue.shift()!;

      if (edgeMap.has(currentId)) {
        const edge = edgeMap.get(currentId)!;
        const target = edge.targetHandle || edge.target;
        if (!forwardVisited.has(target)) {
          forwardVisited.add(target);
          visited.add(target);
          forwardQueue.push(target);
        }
      } else {
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
  }

  // Backward traversal (upstream): Find all sources
  if (direction === 'upstream' || direction === 'both') {
    const backwardQueue = [startId];
    const backwardVisited = new Set<string>([startId]);
    while (backwardQueue.length > 0) {
      const currentId = backwardQueue.shift()!;

      if (edgeMap.has(currentId)) {
        const edge = edgeMap.get(currentId)!;
        const source = edge.sourceHandle || edge.source;
        if (!backwardVisited.has(source)) {
          backwardVisited.add(source);
          visited.add(source);
          backwardQueue.push(source);
        }
      } else {
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
  }

  return visited;
}

/**
 * Find connected elements for multiple start IDs with directional support.
 * Returns union of all connected elements in the specified direction.
 */
export function findConnectedElementsMultipleDirectional(
  startIds: Set<string>,
  edges: FlowEdge[],
  direction: TableFilterDirection
): Set<string> {
  if (!startIds || startIds.size === 0 || !edges || !direction) {
    return new Set(startIds);
  }

  const allConnected = new Set<string>();
  for (const startId of startIds) {
    const connected = findConnectedElementsDirectional(startId, edges, direction);
    for (const id of connected) {
      allConnected.add(id);
    }
  }
  return allConnected;
}

export interface ApplyTableFilterResult {
  graph: { nodes: FlowNode[]; edges: FlowEdge[] };
}

export interface ApplyFiltersOptions {
  graph: { nodes: FlowNode[]; edges: FlowEdge[] };
  highlightIds: Set<string>;
  focusMode: boolean;
  effectiveSearchTerm: string | undefined;
  tableFilter: TableFilter;
}

export interface ApplyFiltersResult {
  graph: { nodes: FlowNode[]; edges: FlowEdge[] };
  tableLabelMap: Map<string, string[]>;
}

/**
 * Apply focus mode and table filtering to a graph.
 * This consolidates the repeated filter logic across different view modes.
 *
 * @param options - Configuration for filtering
 * @returns The filtered graph and the table label map (for potential reuse)
 */
export function applyFilters(options: ApplyFiltersOptions): ApplyFiltersResult {
  const { highlightIds, focusMode, effectiveSearchTerm, tableFilter } = options;
  let graph = options.graph;

  // Apply focus mode filtering if enabled and we have search matches
  if (focusMode && effectiveSearchTerm && highlightIds.size > 0) {
    graph = filterGraphToHighlights(graph, highlightIds);
  }

  // Apply table filter (filter only, no highlighting)
  const tableLabelMap = buildTableLabelMap(graph.nodes);
  const filterResult = applyTableFilter(graph, tableFilter, tableLabelMap);

  return {
    graph: filterResult.graph,
    tableLabelMap,
  };
}

/**
 * Apply table filter to a graph, filtering to show only nodes connected to selected tables.
 * This function only filters the graph - it does NOT add highlights (that's for search/selection).
 *
 * @param graph - The graph to filter
 * @param tableFilter - Filter configuration with selected table labels and direction
 * @param tableLabelToNodeIds - Pre-computed mapping from table labels to node IDs for performance
 * @returns Filtered graph containing only nodes connected to selected tables
 */
export function applyTableFilter(
  graph: { nodes: FlowNode[]; edges: FlowEdge[] },
  tableFilter: TableFilter,
  tableLabelToNodeIds?: Map<string, string[]>
): ApplyTableFilterResult {
  // Input validation
  if (!graph || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) {
    return { graph: { nodes: [], edges: [] } };
  }

  if (!tableFilter || tableFilter.selectedTableLabels.size === 0) {
    return { graph };
  }

  // Find all node IDs that match the selected labels
  const matchingNodeIds = new Set<string>();
  // Also collect column IDs from matching tables for column-level graph traversal
  const matchingColumnIds = new Set<string>();

  if (tableLabelToNodeIds) {
    // Use pre-computed mapping for better performance
    for (const label of tableFilter.selectedTableLabels) {
      const nodeIds = tableLabelToNodeIds.get(label);
      if (nodeIds) {
        for (const nodeId of nodeIds) {
          matchingNodeIds.add(nodeId);
        }
      }
    }
  } else {
    // Fallback to iterating over nodes
    for (const node of graph.nodes) {
      if (isTableNodeData(node.data) && tableFilter.selectedTableLabels.has(node.data.label)) {
        matchingNodeIds.add(node.id);
      }
    }
  }

  // For column-level graphs, edges use column handles rather than table IDs.
  // We need to also include column IDs from matching tables as starting points.
  for (const node of graph.nodes) {
    if (matchingNodeIds.has(node.id) && isTableNodeData(node.data)) {
      for (const col of node.data.columns) {
        matchingColumnIds.add(col.id);
      }
    }
  }

  if (matchingNodeIds.size === 0) {
    // No matching tables in the current graph, so return an empty graph to reflect the active filter
    return { graph: { nodes: [], edges: [] } };
  }

  // Combine table IDs and column IDs for traversal
  const allStartIds = new Set([...matchingNodeIds, ...matchingColumnIds]);

  const tableFilterConnected = findConnectedElementsMultipleDirectional(
    allStartIds,
    graph.edges,
    tableFilter.direction
  );

  // Also include the original table node IDs (they may not be in traversal results
  // if they have no edges, but we still want to show them)
  for (const nodeId of matchingNodeIds) {
    tableFilterConnected.add(nodeId);
  }

  return {
    graph: filterGraphToHighlights(graph, tableFilterConnected),
  };
}

/**
 * Build a mapping from table labels to their node IDs for efficient lookup.
 * Use this to avoid O(n) iteration when applying table filters repeatedly.
 */
export function buildTableLabelMap(nodes: FlowNode[]): Map<string, string[]> {
  const map = new Map<string, string[]>();

  for (const node of nodes) {
    if (isTableNodeData(node.data)) {
      const label = node.data.label;
      const existing = map.get(label);
      if (existing) {
        existing.push(node.id);
      } else {
        map.set(label, [node.id]);
      }
    }
  }

  return map;
}
