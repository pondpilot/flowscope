import { useMemo } from 'react';
import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/react';
import type { TableFilter, NamespaceFilter } from '../types';
import {
  buildGraphIndex,
  findConnectedElementsIndexed,
  findSearchMatchIds,
  findConnectedElementsMultipleIndexed,
  applyFilters,
  pruneDanglingEdges,
  filterByNamespace,
} from '../utils/graphTraversal';

const LARGE_GRAPH_THRESHOLD = 1000;

// Track if we've already warned about large graphs this session
let hasWarnedLargeGraph = false;

export interface UseGraphFilteringOptions {
  /** The graph to filter */
  graph: { nodes: FlowNode[]; edges: FlowEdge[] };
  /** Currently selected node ID */
  selectedNodeId: string | null;
  /** Search term for highlighting */
  searchTerm: string | undefined;
  /** View mode for determining search matching strategy */
  viewMode: 'script' | 'table';
  /** Whether column-level edges are shown (affects search matching in table view) */
  showColumnEdges?: boolean;
  /** Whether focus mode is enabled */
  focusMode: boolean;
  /** Table filter configuration */
  tableFilter: TableFilter;
  /** Namespace filter configuration - filter by schema/database */
  namespaceFilter?: NamespaceFilter;
}

export interface UseGraphFilteringResult {
  /** The filtered graph */
  filteredGraph: { nodes: FlowNode[]; edges: FlowEdge[] };
  /** Set of IDs that should be highlighted */
  highlightIds: Set<string>;
}

/**
 * Hook that applies search highlighting, focus mode filtering, and table filtering to a graph.
 * Consolidates the repeated filter logic used across different view modes in GraphView.
 *
 * @param options - Filtering configuration
 * @returns The filtered graph and highlight IDs
 */
export function useGraphFiltering(options: UseGraphFilteringOptions): UseGraphFilteringResult {
  const {
    graph,
    selectedNodeId,
    searchTerm,
    viewMode,
    showColumnEdges = false,
    focusMode,
    tableFilter,
    namespaceFilter,
  } = options;

  return useMemo(() => {
    // Warn once per session about potentially expensive operations on large graphs
    if (!hasWarnedLargeGraph && graph.nodes.length > LARGE_GRAPH_THRESHOLD) {
      hasWarnedLargeGraph = true;
      console.warn(
        `[useGraphFiltering] Large graph detected: ${graph.nodes.length} nodes. ` +
          'Consider implementing virtualization for better performance.'
      );
    }

    const sanitizedGraph = pruneDanglingEdges(graph);

    // Apply namespace filter first (schema/database filtering)
    const namespaceFilteredGraph = filterByNamespace(sanitizedGraph, namespaceFilter);

    // Build graph index once for all traversal operations (performance optimization)
    const graphIndex = buildGraphIndex(namespaceFilteredGraph.edges);

    // Collect highlight IDs from selection and search matches
    let highlightIds = new Set<string>();

    if (selectedNodeId) {
      highlightIds = findConnectedElementsIndexed(selectedNodeId, graphIndex);
    }

    // Add search-based highlights
    if (searchTerm) {
      const searchMatchIds = findSearchMatchIds(
        searchTerm,
        namespaceFilteredGraph.nodes,
        viewMode,
        showColumnEdges
      );
      const searchConnected = findConnectedElementsMultipleIndexed(searchMatchIds, graphIndex);
      for (const id of searchConnected) {
        highlightIds.add(id);
      }
    }

    // Apply focus mode and table filter with error handling
    try {
      const filterResult = applyFilters({
        graph: namespaceFilteredGraph,
        highlightIds,
        focusMode,
        effectiveSearchTerm: searchTerm,
        tableFilter,
        graphIndex, // Pass pre-built index for efficient table filter traversal
      });

      return {
        filteredGraph: filterResult.graph,
        highlightIds,
      };
    } catch (error) {
      console.error('[useGraphFiltering] Graph filtering failed:', error);
      return {
        filteredGraph: { nodes: [], edges: [] },
        highlightIds: new Set<string>(),
      };
    }
  }, [
    graph,
    selectedNodeId,
    searchTerm,
    viewMode,
    showColumnEdges,
    focusMode,
    tableFilter,
    namespaceFilter,
  ]);
}
