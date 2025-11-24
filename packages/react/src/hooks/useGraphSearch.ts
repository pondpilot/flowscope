import { useMemo } from 'react';
import type { GlobalNode } from '@pondpilot/flowscope-core';
import { useLineageStore } from '../store';

export interface SearchResultItem {
  id: string;
  type: 'table' | 'column' | 'cte' | 'script';
  title: string;
  subtitle?: string;
  nodeId: string;
}

export function useGraphSearch(searchTerm: string) {
  const result = useLineageStore((state) => state.result);

  // Indexing all searchable items
  const items = useMemo(() => {
    const searchItems: SearchResultItem[] = [];

    if (!result) return searchItems;

    // 1. Index Scripts (Files)
    const uniqueScripts = new Set<string>();
    if (result.statements) {
      result.statements.forEach((stmt) => {
        if (stmt.sourceName) {
          uniqueScripts.add(stmt.sourceName);
        }
      });
    }

    uniqueScripts.forEach((scriptName) => {
      searchItems.push({
        id: `script:${scriptName}`,
        type: 'script',
        title: scriptName,
        subtitle: 'FILE',
        nodeId: `script:${scriptName}`,
      });
    });

    // 2. Index Nodes (Tables, CTEs, Columns)
    if (result.globalLineage) {
      // Use type assertion or check if result.globalLineage.nodes matches expected type
      const nodes = result.globalLineage.nodes as unknown as any[]; 
      
      nodes.forEach((node) => {
        // Determine type and label
        let type: SearchResultItem['type'] = 'table';
        if (node.type === 'cte') type = 'cte';
        if (node.type === 'column') type = 'column';

        // Build context/subtitle (e.g. "in users" for a column)
        let subtitle = undefined;
        if (type === 'column' && node.canonicalName) {
          const parts = [];
          if (node.canonicalName.schema) parts.push(node.canonicalName.schema);
          if (node.canonicalName.table) parts.push(node.canonicalName.table);
          if (parts.length > 0) subtitle = `in ${parts.join('.')}`;
        } else if (node.type === 'cte' || node.type === 'table') {
           subtitle = node.type?.toUpperCase();
        }

        searchItems.push({
          id: node.id,
          type,
          title: node.label,
          subtitle,
          nodeId: node.id
        });
      });
    }

    return searchItems;
  }, [result]);

  // Filtering logic
  const results = useMemo(() => {
    if (!searchTerm.trim()) return [];

    const lowerTerm = searchTerm.toLowerCase();
    
    // Simple operator support
    const typeFilter = lowerTerm.match(/type:(\w+)/);
    let filterType: string | null = null;
    let textTerm = lowerTerm;

    if (typeFilter) {
      filterType = typeFilter[1];
      textTerm = lowerTerm.replace(typeFilter[0], '').trim();
    }

    return items
      .filter(item => {
        // Type filter
        if (filterType && !item.type.includes(filterType)) {
          return false;
        }

        // Text match (if any text remains)
        if (!textTerm) return true;

        const titleMatch = item.title.toLowerCase().includes(textTerm);
        const subtitleMatch = item.subtitle?.toLowerCase().includes(textTerm);
        return titleMatch || subtitleMatch;
      })
      .sort((a, b) => {
        if (!textTerm) return 0;

        // Prioritize exact matches
        const aTitle = a.title.toLowerCase();
        const bTitle = b.title.toLowerCase();
        
        if (aTitle === textTerm && bTitle !== textTerm) return -1;
        if (bTitle === textTerm && aTitle !== textTerm) return 1;

        // Then starts with
        if (aTitle.startsWith(textTerm) && !bTitle.startsWith(textTerm)) return -1;
        if (bTitle.startsWith(textTerm) && !aTitle.startsWith(textTerm)) return 1;

        // Prioritize Tables/Scripts over Columns
        const score = (type: string) => (type === 'table' || type === 'cte' || type === 'script' ? 1 : 0);
        return score(b.type) - score(a.type);
      });
  }, [items, searchTerm]);

  return results;
}
