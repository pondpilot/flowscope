import type { Edge as FlowEdge } from '@xyflow/react';

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
  const queue: string[] = [startId];
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
