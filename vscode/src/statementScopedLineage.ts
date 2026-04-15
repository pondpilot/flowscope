import { nodesInStatement, type AnalyzeResult, type FilterPredicate, type Node } from './types';

const STATEMENT_FILTERS_METADATA_KEY = 'statementFilters';
const OCCURRENCE_SPANS_METADATA_KEY = 'occurrenceSpans';
const OCCURRENCE_STATEMENT_IDS_METADATA_KEY = 'occurrenceStatementIds';
const BODY_SPANS_METADATA_KEY = 'bodySpans';
const BODY_STATEMENT_IDS_METADATA_KEY = 'bodyStatementIds';
const STATEMENT_AGGREGATIONS_METADATA_KEY = 'statementAggregations';

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

function isSpan(value: unknown): value is NonNullable<Node['span']> {
  return (
    typeof value === 'object' &&
    value !== null &&
    'start' in value &&
    'end' in value &&
    typeof value.start === 'number' &&
    typeof value.end === 'number'
  );
}

function readSpanArray(value: unknown): NonNullable<Node['span']>[] {
  return Array.isArray(value) ? value.filter(isSpan) : [];
}

function readNumberArray(value: unknown): number[] {
  return Array.isArray(value)
    ? value.filter((entry): entry is number => typeof entry === 'number')
    : [];
}

function isAggregationInfo(value: unknown): value is NonNullable<Node['aggregation']> {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const candidate = value as Partial<NonNullable<Node['aggregation']>>;
  return (
    typeof candidate.isGroupingKey === 'boolean' &&
    (candidate.function === undefined || typeof candidate.function === 'string') &&
    (candidate.distinct === undefined || typeof candidate.distinct === 'boolean')
  );
}

function buildOccurrenceSpans(node: Node): NonNullable<Node['span']>[] {
  const explicit = readSpanArray(node.metadata?.[OCCURRENCE_SPANS_METADATA_KEY]);
  if (explicit.length > 0) {
    return explicit;
  }
  if (node.nameSpans && node.nameSpans.length > 0) {
    return node.nameSpans;
  }
  return node.span ? [node.span] : [];
}

function buildOccurrenceStatementIds(node: Node): number[] {
  const explicit = readNumberArray(node.metadata?.[OCCURRENCE_STATEMENT_IDS_METADATA_KEY]);
  if (explicit.length > 0) {
    return explicit;
  }

  const occurrenceCount = buildOccurrenceSpans(node).length;
  if (occurrenceCount === 0) {
    return [];
  }
  if (node.statementIds.length === 1) {
    return Array.from({ length: occurrenceCount }, () => node.statementIds[0]);
  }
  if (node.statementIds.length === occurrenceCount) {
    return node.statementIds;
  }
  return [];
}

function buildBodySpans(node: Node): NonNullable<Node['span']>[] {
  const explicit = readSpanArray(node.metadata?.[BODY_SPANS_METADATA_KEY]);
  if (explicit.length > 0) {
    return explicit;
  }
  return node.bodySpan ? [node.bodySpan] : [];
}

function buildBodyStatementIds(node: Node): number[] {
  const explicit = readNumberArray(node.metadata?.[BODY_STATEMENT_IDS_METADATA_KEY]);
  if (explicit.length > 0) {
    return explicit;
  }

  const bodySpanCount = buildBodySpans(node).length;
  if (bodySpanCount === 0) {
    return [];
  }
  if (node.statementIds.length === 1) {
    return Array.from({ length: bodySpanCount }, () => node.statementIds[0]);
  }
  if (node.statementIds.length === bodySpanCount) {
    return node.statementIds;
  }
  return [];
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

export function getAggregationForStatement(
  node: Node,
  statementIndex: number
): Node['aggregation'] {
  const perStatement = node.metadata?.[STATEMENT_AGGREGATIONS_METADATA_KEY];
  if (perStatement && typeof perStatement === 'object' && !Array.isArray(perStatement)) {
    const key = String(statementIndex);
    if (Object.prototype.hasOwnProperty.call(perStatement, key)) {
      const value = (perStatement as Record<string, unknown>)[key];
      if (value === null) {
        return undefined;
      }
      if (isAggregationInfo(value)) {
        return value;
      }
    }
  }

  return node.aggregation;
}

export function scopeNodeToStatement(node: Node, statementIndex: number): Node {
  const occurrenceSpans = buildOccurrenceSpans(node);
  const occurrenceStatementIds = buildOccurrenceStatementIds(node);
  const scopedOccurrenceSpans =
    occurrenceStatementIds.length > 0
      ? occurrenceSpans.filter((_, index) => occurrenceStatementIds[index] === statementIndex)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? occurrenceSpans
        : [];

  const bodySpans = buildBodySpans(node);
  const bodyStatementIds = buildBodyStatementIds(node);
  const scopedBodySpans =
    bodyStatementIds.length > 0
      ? bodySpans.filter((_, index) => bodyStatementIds[index] === statementIndex)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? bodySpans
        : [];

  return {
    ...node,
    statementIds: [statementIndex],
    span: scopedOccurrenceSpans[0] ?? node.span,
    nameSpans: scopedOccurrenceSpans.length > 0 ? scopedOccurrenceSpans : node.nameSpans,
    bodySpan: scopedBodySpans[0],
    aggregation: getAggregationForStatement(node, statementIndex),
    filters: getFiltersForStatement(node, statementIndex),
  };
}

export function scopeNodesToStatement(result: AnalyzeResult, statementIndex: number): Node[] {
  return nodesInStatement(result, statementIndex).map((node) =>
    scopeNodeToStatement(node, statementIndex)
  );
}
