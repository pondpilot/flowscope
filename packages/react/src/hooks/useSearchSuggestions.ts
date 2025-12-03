import { useMemo } from 'react';
import { isTableLikeType } from '@pondpilot/flowscope-core';
import { useLineageStore } from '../store';

export type SearchableType = 'table' | 'view' | 'cte' | 'column' | 'script';

export interface SearchSuggestion {
  id: string;
  type: SearchableType;
  label: string;
  subtitle?: string;
  /** Number of occurrences of this name (for deduplication display) */
  count?: number;
}

export interface UseSearchSuggestionsOptions {
  searchTerm: string;
  searchableTypes: SearchableType[];
  maxSuggestions?: number;
}

export interface UseSearchSuggestionsResult {
  suggestions: SearchSuggestion[];
  allItems: SearchSuggestion[];
}

const DEFAULT_MAX_SUGGESTIONS = 8;

/**
 * Group items by label and type, returning deduplicated suggestions with counts.
 */
function deduplicateSuggestions(items: SearchSuggestion[]): SearchSuggestion[] {
  // Group by label + type
  const groups = new Map<string, { item: SearchSuggestion; count: number; contexts: Set<string> }>();

  for (const item of items) {
    const key = `${item.type}:${item.label.toLowerCase()}`;
    const existing = groups.get(key);

    if (existing) {
      existing.count++;
      if (item.subtitle) {
        existing.contexts.add(item.subtitle);
      }
    } else {
      groups.set(key, {
        item,
        count: 1,
        contexts: item.subtitle ? new Set([item.subtitle]) : new Set(),
      });
    }
  }

  // Convert back to array with updated subtitles
  return Array.from(groups.values()).map(({ item, count, contexts }) => {
    let subtitle: string | undefined;

    if (item.type === 'column') {
      // For columns, show count of tables they appear in
      if (count > 1) {
        subtitle = `in ${count} tables`;
      } else if (contexts.size === 1) {
        // Single occurrence - show the table name
        subtitle = Array.from(contexts)[0];
      }
    } else if (item.type === 'script') {
      subtitle = 'file';
    } else {
      // For tables/views/CTEs, show the type
      subtitle = item.type.toUpperCase();
    }

    return {
      // Use just the label as ID since we're deduplicating
      id: `${item.type}:${item.label}`,
      type: item.type,
      label: item.label,
      subtitle,
      count,
    };
  });
}

export function useSearchSuggestions({
  searchTerm,
  searchableTypes,
  maxSuggestions = DEFAULT_MAX_SUGGESTIONS,
}: UseSearchSuggestionsOptions): UseSearchSuggestionsResult {
  const result = useLineageStore((state) => state.result);

  // Index all searchable items (before deduplication)
  const allItems = useMemo(() => {
    const items: SearchSuggestion[] = [];

    if (!result) return items;

    // Index Scripts (Files)
    if (searchableTypes.includes('script')) {
      const uniqueScripts = new Set<string>();
      if (result.statements) {
        result.statements.forEach((stmt) => {
          if (stmt.sourceName) {
            uniqueScripts.add(stmt.sourceName);
          }
        });
      }

      uniqueScripts.forEach((scriptName) => {
        items.push({
          id: `script:${scriptName}`,
          type: 'script',
          label: scriptName,
          subtitle: 'file',
        });
      });
    }

    // Index Nodes (Tables, Views, CTEs, Columns)
    if (result.globalLineage) {
      result.globalLineage.nodes.forEach((node) => {
        // Determine type
        let type: SearchableType;
        if (node.type === 'cte') {
          type = 'cte';
        } else if (node.type === 'column') {
          type = 'column';
        } else if (node.type === 'view') {
          type = 'view';
        } else {
          type = 'table';
        }

        // Skip if type is not in searchable types
        if (!searchableTypes.includes(type)) {
          return;
        }

        // Build context/subtitle
        let subtitle: string | undefined = undefined;
        if (type === 'column' && node.canonicalName) {
          const parts: string[] = [];
          if (node.canonicalName.schema) parts.push(node.canonicalName.schema);
          if (node.canonicalName.name) parts.push(node.canonicalName.name);
          if (parts.length > 0) subtitle = `in ${parts.join('.')}`;
        } else if (isTableLikeType(node.type)) {
          subtitle = node.type?.toUpperCase();
        }

        items.push({
          id: node.id,
          type,
          label: node.label,
          subtitle,
        });
      });
    }

    return items;
  }, [result, searchableTypes]);

  // Filter, deduplicate, and sort suggestions based on search term
  const suggestions = useMemo(() => {
    if (!searchTerm.trim()) return [];

    const lowerTerm = searchTerm.toLowerCase();

    // First filter by search term
    const filtered = allItems.filter((item) => {
      const labelMatch = item.label.toLowerCase().includes(lowerTerm);
      return labelMatch;
    });

    // Deduplicate by name + type
    const deduplicated = deduplicateSuggestions(filtered);

    // Sort
    return deduplicated
      .sort((a, b) => {
        const aLabel = a.label.toLowerCase();
        const bLabel = b.label.toLowerCase();

        // Prioritize exact matches
        if (aLabel === lowerTerm && bLabel !== lowerTerm) return -1;
        if (bLabel === lowerTerm && aLabel !== lowerTerm) return 1;

        // Then starts with
        if (aLabel.startsWith(lowerTerm) && !bLabel.startsWith(lowerTerm)) return -1;
        if (bLabel.startsWith(lowerTerm) && !aLabel.startsWith(lowerTerm)) return 1;

        // Prioritize Tables/Views/Scripts over Columns
        const score = (type: SearchableType) =>
          type === 'table' || type === 'view' || type === 'cte' || type === 'script' ? 1 : 0;
        const typeScoreDiff = score(b.type) - score(a.type);
        if (typeScoreDiff !== 0) return typeScoreDiff;

        // For same type, sort by count (more occurrences = more relevant)
        return (b.count ?? 1) - (a.count ?? 1);
      })
      .slice(0, maxSuggestions);
  }, [allItems, searchTerm, maxSuggestions]);

  return { suggestions, allItems };
}
