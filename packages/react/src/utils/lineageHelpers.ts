import type { StatementLineage, Node, Edge } from '@pondpilot/flowscope-core';

const CREATE_STATEMENT_TYPES = new Set(['CREATE_TABLE', 'CREATE_TABLE_AS', 'CREATE_VIEW']);

/** Node type constant for output nodes. */
export const OUTPUT_NODE_TYPE = 'output' as Node['type'];

/** Edge type constant for join dependency edges. */
export const JOIN_DEPENDENCY_EDGE_TYPE = 'join_dependency' as Edge['type'];

/**
 * Returns the node ids for relations created by a statement (e.g. CREATE TABLE/VIEW).
 * For CREATE statements we prefer nodes that receive data_flow edges; when lineage
 * does not include explicit flows (simple CREATE TABLE), we fall back to the sole
 * relation node or one that matches the statement type.
 */
export function getCreatedRelationNodeIds(stmt: StatementLineage): Set<string> {
  if (!CREATE_STATEMENT_TYPES.has(stmt.statementType)) {
    return new Set();
  }

  const relationNodes = stmt.nodes.filter((n) => n.type === 'table' || n.type === 'view');
  const relationNodeIds = new Set(relationNodes.map((n) => n.id));

  const createdNodeIds = new Set<string>();
  for (const edge of stmt.edges) {
    if (edge.type === 'data_flow' && relationNodeIds.has(edge.to)) {
      createdNodeIds.add(edge.to);
    }
  }

  if (createdNodeIds.size > 0) {
    return createdNodeIds;
  }

  if (relationNodes.length === 1) {
    createdNodeIds.add(relationNodes[0].id);
    return createdNodeIds;
  }

  // When lineage data does not include flows, fall back to the relation type that matches the statement.
  const targetType = stmt.statementType === 'CREATE_VIEW' ? 'view' : 'table';
  const matchingNodes = relationNodes.filter((node) => node.type === targetType);
  if (matchingNodes.length === 1) {
    createdNodeIds.add(matchingNodes[0].id);
  }

  return createdNodeIds;
}

/**
 * Build a map from column IDs to their owning table info.
 * @param edges - The edges to search for ownership relationships
 * @param tableNodes - The table/relation nodes to look up
 * @param mapper - Function to extract the desired value from a table node
 */
export function buildColumnOwnershipMap<T>(
  edges: Edge[],
  tableNodes: Node[],
  mapper: (node: Node) => T
): Map<string, T> {
  const result = new Map<string, T>();
  for (const edge of edges) {
    if (edge.type === 'ownership') {
      const tableNode = tableNodes.find((t) => t.id === edge.from);
      if (tableNode) {
        result.set(edge.to, mapper(tableNode));
      }
    }
  }
  return result;
}
