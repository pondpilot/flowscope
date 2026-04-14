import type { AnalyzeResult, FilterPredicate, Node } from './types';

const STATEMENT_FILTERS_METADATA_KEY = 'statementFilters';

function isFilterPredicateArray(value: unknown): value is FilterPredicate[] {
  return (
    Array.isArray(value) &&
    value.every((entry) => {
      if (!entry || typeof entry !== 'object') {
        return false;
      }
      const candidate = entry as Partial<FilterPredicate>;
      return typeof candidate.expression === 'string' && typeof candidate.clauseType === 'string';
    })
  );
}

export function getFiltersForStatement(node: Node, statementIndex: number): FilterPredicate[] {
  const perStatement = node.metadata?.[STATEMENT_FILTERS_METADATA_KEY];
  if (perStatement && typeof perStatement === 'object' && !Array.isArray(perStatement)) {
    const value = (perStatement as Record<string, unknown>)[String(statementIndex)];
    if (isFilterPredicateArray(value)) {
      return value;
    }
  }

  return node.filters ?? [];
}

export function scopeNodeToStatement(node: Node, statementIndex: number): Node {
  return {
    ...node,
    filters: getFiltersForStatement(node, statementIndex),
  };
}

export function scopeNodesToStatement(result: AnalyzeResult, statementIndex: number): Node[] {
  return result.nodes
    .filter((node) => node.statementIds.includes(statementIndex))
    .map((node) => scopeNodeToStatement(node, statementIndex));
}
