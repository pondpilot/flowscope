import { nodesInStatement, type AnalyzeResult, type FilterPredicate, type Node } from './types';

const STATEMENT_FILTERS_METADATA_KEY = 'statementFilters';
const OCCURRENCE_SPANS_METADATA_KEY = 'occurrenceSpans';
const OCCURRENCE_STATEMENT_IDS_METADATA_KEY = 'occurrenceStatementIds';
const OCCURRENCE_SOURCE_NAMES_METADATA_KEY = 'occurrenceSourceNames';
const BODY_SPANS_METADATA_KEY = 'bodySpans';
const BODY_STATEMENT_IDS_METADATA_KEY = 'bodyStatementIds';
const BODY_SOURCE_NAMES_METADATA_KEY = 'bodySourceNames';
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

function readSourceNameArray(value: unknown): Array<string | null> {
  return Array.isArray(value)
    ? value.map((entry) => (typeof entry === 'string' ? entry : null))
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

function getFallbackSourceName(node: Node, sourceName?: string): string | null {
  if (typeof node.metadata?.sourceName === 'string') {
    return node.metadata.sourceName;
  }

  return sourceName ?? null;
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

function buildOccurrenceSourceNames(node: Node, sourceName?: string): Array<string | null> {
  const spanCount = buildOccurrenceSpans(node).length;
  if (spanCount === 0) {
    return [];
  }

  const explicit = readSourceNameArray(node.metadata?.[OCCURRENCE_SOURCE_NAMES_METADATA_KEY]);
  const fallback = getFallbackSourceName(node, sourceName);
  return Array.from({ length: spanCount }, (_, index) => explicit[index] ?? fallback);
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

function buildBodySourceNames(node: Node, sourceName?: string): Array<string | null> {
  const bodySpanCount = buildBodySpans(node).length;
  if (bodySpanCount === 0) {
    return [];
  }

  const explicit = readSourceNameArray(node.metadata?.[BODY_SOURCE_NAMES_METADATA_KEY]);
  const fallback = getFallbackSourceName(node, sourceName);
  return Array.from({ length: bodySpanCount }, (_, index) => explicit[index] ?? fallback);
}

function getOccurrenceIndexesForStatement(node: Node, statementIndex: number): number[] {
  const statementIds = buildOccurrenceStatementIds(node);
  if (statementIds.length === 0) {
    return [];
  }

  return statementIds.flatMap((value, index) => (value === statementIndex ? [index] : []));
}

function getBodyIndexesForStatement(node: Node, statementIndex: number): number[] {
  const statementIds = buildBodyStatementIds(node);
  if (statementIds.length === 0) {
    return [];
  }

  return statementIds.flatMap((value, index) => (value === statementIndex ? [index] : []));
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

export function scopeNodeToStatement(
  node: Node,
  statementIndex: number,
  sourceName?: string
): Node {
  const occurrenceSpans = buildOccurrenceSpans(node);
  const occurrenceSourceNames = buildOccurrenceSourceNames(node, sourceName);
  const occurrenceIndexes = getOccurrenceIndexesForStatement(node, statementIndex);
  const scopedOccurrenceSpans =
    occurrenceIndexes.length > 0
      ? occurrenceIndexes
          .map((index) => occurrenceSpans[index])
          .filter((span): span is NonNullable<Node['span']> => !!span)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? occurrenceSpans
        : [];
  const scopedOccurrenceSourceNames =
    occurrenceIndexes.length > 0
      ? occurrenceIndexes.map((index) => occurrenceSourceNames[index] ?? null)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? occurrenceSourceNames
        : [];

  const bodySpans = buildBodySpans(node);
  const bodySourceNames = buildBodySourceNames(node, sourceName);
  const bodyIndexes = getBodyIndexesForStatement(node, statementIndex);
  const scopedBodySpans =
    bodyIndexes.length > 0
      ? bodyIndexes
          .map((index) => bodySpans[index])
          .filter((span): span is NonNullable<Node['span']> => !!span)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? bodySpans
        : [];
  const scopedBodySourceNames =
    bodyIndexes.length > 0
      ? bodyIndexes.map((index) => bodySourceNames[index] ?? null)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? bodySourceNames
        : [];

  return {
    ...node,
    statementIds: [statementIndex],
    span: scopedOccurrenceSpans[0] ?? node.span,
    nameSpans: scopedOccurrenceSpans.length > 0 ? scopedOccurrenceSpans : node.nameSpans,
    bodySpan: scopedBodySpans[0],
    aggregation: getAggregationForStatement(node, statementIndex),
    filters: getFiltersForStatement(node, statementIndex),
    metadata: {
      ...(node.metadata || {}),
      ...(sourceName ? { sourceName } : {}),
      ...(scopedOccurrenceSpans.length > 0
        ? {
            [OCCURRENCE_SPANS_METADATA_KEY]: scopedOccurrenceSpans,
            [OCCURRENCE_STATEMENT_IDS_METADATA_KEY]: Array.from(
              { length: scopedOccurrenceSpans.length },
              () => statementIndex
            ),
            [OCCURRENCE_SOURCE_NAMES_METADATA_KEY]: scopedOccurrenceSourceNames.map(
              (value) => value ?? sourceName ?? null
            ),
          }
        : {}),
      ...(scopedBodySpans.length > 0
        ? {
            [BODY_SPANS_METADATA_KEY]: scopedBodySpans,
            [BODY_STATEMENT_IDS_METADATA_KEY]: Array.from(
              { length: scopedBodySpans.length },
              () => statementIndex
            ),
            [BODY_SOURCE_NAMES_METADATA_KEY]: scopedBodySourceNames.map(
              (value) => value ?? sourceName ?? null
            ),
          }
        : {}),
    },
  };
}

export function scopeNodesToStatement(
  result: AnalyzeResult,
  statementIndex: number,
  sourceName?: string
): Node[] {
  return nodesInStatement(result, statementIndex).map((node) =>
    scopeNodeToStatement(node, statementIndex, sourceName)
  );
}
