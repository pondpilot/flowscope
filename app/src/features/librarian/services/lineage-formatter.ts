import type { AnalyzeResult } from '@pondpilot/flowscope-core';

/**
 * Formats an AnalyzeResult into a human-readable lineage summary for the LLM.
 */
export function formatLineage(result: AnalyzeResult | null): string {
  if (!result) return '';

  const { nodes, edges, resolvedSchema } = result;
  const sections: string[] = [];

  // Tables and columns from resolved schema
  if (resolvedSchema?.tables && resolvedSchema.tables.length > 0) {
    const tableLines: string[] = [];
    for (const table of resolvedSchema.tables) {
      const qualifiedName = [table.catalog, table.schema, table.name].filter(Boolean).join('.');
      const columns = table.columns.map((col) => {
        const parts = [col.name];
        if (col.dataType) parts.push(col.dataType);
        if (col.isPrimaryKey) parts.push('PK');
        if (col.foreignKey) {
          parts.push(`FK -> ${col.foreignKey.table}.${col.foreignKey.column}`);
        }
        return `  - ${parts.join(' | ')}`;
      });
      tableLines.push(`${qualifiedName}\n${columns.join('\n')}`);
    }
    sections.push(`Tables:\n${tableLines.join('\n\n')}`);
  }

  // Table-level nodes from the flat lineage graph (only if no resolved schema)
  if ((!resolvedSchema?.tables || resolvedSchema.tables.length === 0) && nodes.length > 0) {
    const tableNodes = nodes.filter(
      (n) => n.type === 'table' || n.type === 'view' || n.type === 'cte'
    );
    if (tableNodes.length > 0) {
      const names = tableNodes.map((n) => `- ${n.label}`).join('\n');
      sections.push(`Tables:\n${names}`);
    }
  }

  // Relationships from edges
  if (edges.length > 0) {
    const nodeMap = new Map(nodes.map((n) => [n.id, n.label]));
    const relationships: string[] = [];
    for (const edge of edges) {
      const from = nodeMap.get(edge.from) ?? edge.from;
      const to = nodeMap.get(edge.to) ?? edge.to;
      relationships.push(`- ${from} --[${edge.type}]--> ${to}`);
    }
    sections.push(`Relationships:\n${relationships.join('\n')}`);
  }

  return sections.join('\n\n');
}
