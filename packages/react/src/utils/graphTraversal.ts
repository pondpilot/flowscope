import type { Edge as FlowEdge, Node as FlowNode } from '@xyflow/react';
import type {
  TableNodeData,
  ScriptNodeData,
  TableFilterDirection,
  TableFilter,
  NamespaceFilter,
} from '../types';

/**
 * Pre-computed graph index for efficient traversal operations.
 * Build once when graph changes, reuse for all traversal operations.
 */
export interface GraphIndex {
  /** Map from source (node/column ID) to outgoing edge IDs */
  downstreamMap: Map<string, string[]>;
  /** Map from target (node/column ID) to incoming edge IDs */
  upstreamMap: Map<string, string[]>;
  /** Map from edge ID to edge object */
  edgeMap: Map<string, FlowEdge>;
}

/**
 * Traverse the graph in a single direction using BFS.
 * @param startId Starting node/column ID
 * @param edgeMap Map from edge ID to edge object
 * @param adjacencyMap Map from node/column ID to adjacent edge IDs
 * @param getNextId Function to extract the next node ID from an edge (source or target)
 * @returns Set of visited IDs (nodes, columns, and edges)
 */
function traverseDirection(
  startId: string,
  edgeMap: Map<string, FlowEdge>,
  adjacencyMap: Map<string, string[]>,
  getNextId: (edge: FlowEdge) => string
): Set<string> {
  const visited = new Set<string>([startId]);
  const queue = [startId];

  while (queue.length > 0) {
    const currentId = queue.shift()!;

    if (edgeMap.has(currentId)) {
      const edge = edgeMap.get(currentId)!;
      const nextId = getNextId(edge);
      if (!visited.has(nextId)) {
        visited.add(nextId);
        queue.push(nextId);
      }
    } else {
      const adjacentEdges = adjacencyMap.get(currentId) || [];
      for (const edgeId of adjacentEdges) {
        if (!visited.has(edgeId)) {
          visited.add(edgeId);
          queue.push(edgeId);
        }
      }
    }
  }

  return visited;
}

/**
 * Build a graph index from edges for efficient traversal.
 * Call this once when the graph changes, then pass to traversal functions.
 */
export function buildGraphIndex(edges: FlowEdge[]): GraphIndex {
  const downstreamMap = new Map<string, string[]>();
  const upstreamMap = new Map<string, string[]>();
  const edgeMap = new Map<string, FlowEdge>();

  for (const edge of edges) {
    edgeMap.set(edge.id, edge);

    const source = edge.sourceHandle || edge.source;
    const target = edge.targetHandle || edge.target;

    let downstream = downstreamMap.get(source);
    if (!downstream) {
      downstream = [];
      downstreamMap.set(source, downstream);
    }
    downstream.push(edge.id);

    let upstream = upstreamMap.get(target);
    if (!upstream) {
      upstream = [];
      upstreamMap.set(target, upstream);
    }
    upstream.push(edge.id);
  }

  return { downstreamMap, upstreamMap, edgeMap };
}

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
 * Remove edges that reference missing nodes or handles.
 * This guards against dangling connections when nodes are filtered or rebuilt.
 */
export function pruneDanglingEdges(graph: { nodes: FlowNode[]; edges: FlowEdge[] }): {
  nodes: FlowNode[];
  edges: FlowEdge[];
} {
  // Defensive check: graph may be undefined during initial render or state transitions
  if (!graph || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) {
    return { nodes: [], edges: [] };
  }

  const nodeById = new Map<string, FlowNode>();
  const handleIdsByNodeId = new Map<string, Set<string>>();

  for (const node of graph.nodes) {
    nodeById.set(node.id, node);
    if (isTableNodeData(node.data)) {
      handleIdsByNodeId.set(node.id, new Set(node.data.columns.map((col) => col.id)));
    }
  }

  const filteredEdges = graph.edges.filter((edge) => {
    if (!nodeById.has(edge.source) || !nodeById.has(edge.target)) {
      return false;
    }

    if (typeof edge.sourceHandle === 'string' && edge.sourceHandle.length > 0) {
      const sourceHandles = handleIdsByNodeId.get(edge.source);
      if (!sourceHandles || !sourceHandles.has(edge.sourceHandle)) {
        return false;
      }
    }

    if (typeof edge.targetHandle === 'string' && edge.targetHandle.length > 0) {
      const targetHandles = handleIdsByNodeId.get(edge.target);
      if (!targetHandles || !targetHandles.has(edge.targetHandle)) {
        return false;
      }
    }

    return true;
  });

  if (filteredEdges.length === graph.edges.length) {
    return graph;
  }

  return { ...graph, edges: filteredEdges };
}

/**
 * Filter nodes by namespace (schema/database).
 *
 * Filtering behavior:
 * - If schemas are selected: nodes with a schema must have it in the selected list
 * - If databases are selected: nodes with a database must have it in the selected list
 * - Nodes without schema/database ("unscoped") always pass through
 * - Non-table nodes (like script nodes) are always preserved
 * - Empty filter arrays mean "show all" for that dimension
 */
export function filterByNamespace(
  graph: { nodes: FlowNode[]; edges: FlowEdge[] },
  namespaceFilter: NamespaceFilter | undefined
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  if (!namespaceFilter) return graph;

  const { schemas, databases } = namespaceFilter;

  // If no filters selected, show all
  if (schemas.length === 0 && databases.length === 0) {
    return graph;
  }

  // Use Sets for O(1) lookups
  const schemaSet = new Set(schemas);
  const databaseSet = new Set(databases);

  // Filter nodes by their schema/database
  const filteredNodes = graph.nodes.filter((node) => {
    if (!isTableNodeData(node.data)) {
      // Non-table nodes (like script nodes) don't have namespace info, so keep them.
      // Edge pruning will handle removing edges to filtered-out tables.
      return true;
    }

    const nodeData = node.data as TableNodeData;

    // Check schema filter - only filter nodes that HAVE a schema value
    // Nodes without schema are "unscoped" and pass through the filter
    if (schemaSet.size > 0 && nodeData.schema) {
      if (!schemaSet.has(nodeData.schema)) {
        return false;
      }
    }

    // Check database filter - only filter nodes that HAVE a database value
    // Nodes without database are "unscoped" and pass through the filter
    if (databaseSet.size > 0 && nodeData.database) {
      if (!databaseSet.has(nodeData.database)) {
        return false;
      }
    }

    return true;
  });

  // Get valid node IDs (including column IDs from table nodes)
  const validNodeIds = new Set<string>();
  for (const node of filteredNodes) {
    validNodeIds.add(node.id);
    if (isTableNodeData(node.data)) {
      for (const col of node.data.columns) {
        validNodeIds.add(col.id);
      }
    }
  }

  // Filter edges to only include those connecting valid nodes
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
 * Find connected elements for multiple start IDs using a pre-built index.
 * Returns union of all connected elements.
 */
export function findConnectedElementsMultipleIndexed(
  startIds: Set<string>,
  index: GraphIndex
): Set<string> {
  const allConnected = new Set<string>();
  for (const startId of startIds) {
    const connected = findConnectedElementsIndexed(startId, index);
    for (const id of connected) {
      allConnected.add(id);
    }
  }
  return allConnected;
}

/**
 * Find connected elements for multiple start IDs.
 * Returns union of all connected elements.
 *
 * NOTE: This is a convenience wrapper that builds an index internally.
 * For multiple traversals on the same graph, use buildGraphIndex() once
 * and call findConnectedElementsMultipleIndexed() for better performance.
 */
export function findConnectedElementsMultiple(
  startIds: Set<string>,
  edges: FlowEdge[]
): Set<string> {
  const index = buildGraphIndex(edges);
  return findConnectedElementsMultipleIndexed(startIds, index);
}

/**
 * Traverse the graph using a pre-built index.
 * More efficient when performing multiple traversals on the same graph.
 * @param startId The ID of the node or column to start the traversal from
 * @param index Pre-built graph index from buildGraphIndex()
 * @returns Set of all connected element IDs (nodes, columns, and edges)
 */
export function findConnectedElementsIndexed(startId: string, index: GraphIndex): Set<string> {
  const { downstreamMap, upstreamMap, edgeMap } = index;

  // Forward traversal: Find all downstream consumers
  const downstream = traverseDirection(
    startId,
    edgeMap,
    downstreamMap,
    (edge) => edge.targetHandle || edge.target
  );

  // Backward traversal: Find all upstream sources
  const upstream = traverseDirection(
    startId,
    edgeMap,
    upstreamMap,
    (edge) => edge.sourceHandle || edge.source
  );

  // Merge both directions (startId is in both sets)
  for (const id of upstream) {
    downstream.add(id);
  }

  return downstream;
}

/**
 * Traverse the graph to find all connected elements (nodes/edges) upstream and downstream.
 * Uses bidirectional BFS to find all elements in the data lineage path.
 *
 * NOTE: This is a convenience wrapper that builds an index internally.
 * For multiple traversals on the same graph, use buildGraphIndex() once
 * and call findConnectedElementsIndexed() for better performance.
 *
 * @param startId The ID of the node or column to start the traversal from
 * @param edges All edges in the graph
 * @returns Set of all connected element IDs (nodes, columns, and edges)
 */
export function findConnectedElements(startId: string, edges: FlowEdge[]): Set<string> {
  const index = buildGraphIndex(edges);
  return findConnectedElementsIndexed(startId, index);
}

/**
 * Traverse the graph using a pre-built index in a specific direction.
 * @param startId The ID of the node to start the traversal from
 * @param index Pre-built graph index
 * @param direction Which direction to traverse: 'upstream', 'downstream', or 'both'
 * @returns Set of all connected element IDs in the specified direction
 */
export function findConnectedElementsDirectionalIndexed(
  startId: string,
  index: GraphIndex,
  direction: TableFilterDirection
): Set<string> {
  if (!startId || !direction) {
    return new Set(startId ? [startId] : []);
  }

  const { downstreamMap, upstreamMap, edgeMap } = index;
  const visited = new Set<string>([startId]);

  // Forward traversal (downstream): Find all consumers
  if (direction === 'downstream' || direction === 'both') {
    const downstream = traverseDirection(
      startId,
      edgeMap,
      downstreamMap,
      (edge) => edge.targetHandle || edge.target
    );
    for (const id of downstream) {
      visited.add(id);
    }
  }

  // Backward traversal (upstream): Find all sources
  if (direction === 'upstream' || direction === 'both') {
    const upstream = traverseDirection(
      startId,
      edgeMap,
      upstreamMap,
      (edge) => edge.sourceHandle || edge.source
    );
    for (const id of upstream) {
      visited.add(id);
    }
  }

  return visited;
}

/**
 * Traverse the graph to find connected elements in a specific direction.
 *
 * NOTE: This is a convenience wrapper that builds an index internally.
 * For multiple traversals on the same graph, use buildGraphIndex() once
 * and call findConnectedElementsDirectionalIndexed() for better performance.
 *
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
  const index = buildGraphIndex(edges);
  return findConnectedElementsDirectionalIndexed(startId, index, direction);
}

/**
 * Find connected elements for multiple start IDs with directional support using pre-built index.
 * Returns union of all connected elements in the specified direction.
 */
export function findConnectedElementsMultipleDirectionalIndexed(
  startIds: Set<string>,
  index: GraphIndex,
  direction: TableFilterDirection
): Set<string> {
  if (!startIds || startIds.size === 0 || !direction) {
    return new Set(startIds);
  }

  const allConnected = new Set<string>();
  for (const startId of startIds) {
    const connected = findConnectedElementsDirectionalIndexed(startId, index, direction);
    for (const id of connected) {
      allConnected.add(id);
    }
  }
  return allConnected;
}

/**
 * Find connected elements for multiple start IDs with directional support.
 * Returns union of all connected elements in the specified direction.
 *
 * NOTE: This is a convenience wrapper that builds an index internally.
 * For multiple traversals on the same graph, use buildGraphIndex() once
 * and call findConnectedElementsMultipleDirectionalIndexed() for better performance.
 */
export function findConnectedElementsMultipleDirectional(
  startIds: Set<string>,
  edges: FlowEdge[],
  direction: TableFilterDirection
): Set<string> {
  if (!startIds || startIds.size === 0 || !edges || !direction) {
    return new Set(startIds);
  }
  const index = buildGraphIndex(edges);
  return findConnectedElementsMultipleDirectionalIndexed(startIds, index, direction);
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
  /** Optional pre-built graph index for performance */
  graphIndex?: GraphIndex;
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
  const { highlightIds, focusMode, effectiveSearchTerm, tableFilter, graphIndex } = options;
  let graph = options.graph;

  // Apply focus mode filtering if enabled and we have search matches
  if (focusMode && effectiveSearchTerm && highlightIds.size > 0) {
    graph = filterGraphToHighlights(graph, highlightIds);
  }

  // Apply table filter (filter only, no highlighting)
  const tableLabelMap = buildTableLabelMap(graph.nodes);
  const filterResult = applyTableFilter(graph, tableFilter, tableLabelMap, graphIndex);

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
 * @param graphIndex - Optional pre-built graph index for efficient traversal
 * @returns Filtered graph containing only nodes connected to selected tables
 */
export function applyTableFilter(
  graph: { nodes: FlowNode[]; edges: FlowEdge[] },
  tableFilter: TableFilter,
  tableLabelToNodeIds?: Map<string, string[]>,
  graphIndex?: GraphIndex
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

  // Use pre-built index if provided, otherwise build on-the-fly
  const tableFilterConnected = graphIndex
    ? findConnectedElementsMultipleDirectionalIndexed(
        allStartIds,
        graphIndex,
        tableFilter.direction
      )
    : findConnectedElementsMultipleDirectional(allStartIds, graph.edges, tableFilter.direction);

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
