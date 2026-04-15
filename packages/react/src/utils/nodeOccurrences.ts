import type { AnalyzeResult, Node, Span, StatementMeta } from '@pondpilot/flowscope-core';

const OCCURRENCE_SPANS_METADATA_KEY = 'occurrenceSpans';
const OCCURRENCE_STATEMENT_IDS_METADATA_KEY = 'occurrenceStatementIds';
const OCCURRENCE_SOURCE_NAMES_METADATA_KEY = 'occurrenceSourceNames';
const BODY_SPANS_METADATA_KEY = 'bodySpans';
const BODY_STATEMENT_IDS_METADATA_KEY = 'bodyStatementIds';
const BODY_SOURCE_NAMES_METADATA_KEY = 'bodySourceNames';
const STATEMENT_AGGREGATIONS_METADATA_KEY = 'statementAggregations';

function isSpan(value: unknown): value is Span {
  return (
    typeof value === 'object' &&
    value !== null &&
    'start' in value &&
    'end' in value &&
    typeof value.start === 'number' &&
    typeof value.end === 'number'
  );
}

function readSpanArray(value: unknown): Span[] {
  return Array.isArray(value) ? value.filter(isSpan) : [];
}

function readSourceNameArray(value: unknown): Array<string | null> {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.map((entry) => (typeof entry === 'string' ? entry : null));
}

function readNumberArray(value: unknown): number[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.filter((entry): entry is number => typeof entry === 'number');
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

function buildOccurrenceSpans(node: Node): Span[] {
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

function buildOccurrenceSourceNames(node: Node, sourceName?: string): Array<string | null> {
  const spanCount = buildOccurrenceSpans(node).length;
  if (spanCount === 0) {
    return [];
  }

  const explicit = readSourceNameArray(node.metadata?.[OCCURRENCE_SOURCE_NAMES_METADATA_KEY]);
  const fallback = getFallbackSourceName(node, sourceName);
  return Array.from({ length: spanCount }, (_, index) => explicit[index] ?? fallback);
}

function buildBodySpans(node: Node): Span[] {
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

function buildBodySourceNames(node: Node, sourceName?: string): Array<string | null> {
  const bodySpans = buildBodySpans(node);
  if (bodySpans.length === 0) {
    return [];
  }

  const explicit = readSourceNameArray(node.metadata?.[BODY_SOURCE_NAMES_METADATA_KEY]);
  const fallback = getFallbackSourceName(node, sourceName);
  return Array.from({ length: bodySpans.length }, (_, index) => explicit[index] ?? fallback);
}

export function getOccurrenceSourceName(node: Node, index: number): string | undefined {
  const sourceName = buildOccurrenceSourceNames(node)[index];
  return typeof sourceName === 'string' ? sourceName : undefined;
}

export function getOccurrenceSpan(node: Node, index: number): Span | undefined {
  return buildOccurrenceSpans(node)[index];
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

export function getBodySpanForSourceName(node: Node, sourceName?: string): Span | undefined {
  const bodySpans = buildBodySpans(node);
  if (bodySpans.length === 0) {
    return undefined;
  }
  if (!sourceName) {
    return bodySpans[0];
  }

  const bodySourceNames = buildBodySourceNames(node);
  const matchingIndex = bodySourceNames.findIndex((entry) => entry === sourceName);
  return matchingIndex >= 0 ? bodySpans[matchingIndex] : bodySpans[0];
}

export function getOccurrenceForStatement(
  node: Node,
  statementIndex: number
): { spans: Span[]; sourceNames: Array<string | null> } {
  const occurrenceSpans = buildOccurrenceSpans(node);
  const occurrenceSourceNames = buildOccurrenceSourceNames(node);
  const indexes = getOccurrenceIndexesForStatement(node, statementIndex);

  if (indexes.length === 0) {
    if (node.statementIds.length === 1 && node.statementIds[0] === statementIndex) {
      return { spans: occurrenceSpans, sourceNames: occurrenceSourceNames };
    }
    return { spans: [], sourceNames: [] };
  }

  return {
    spans: indexes.map((index) => occurrenceSpans[index]).filter((span): span is Span => !!span),
    sourceNames: indexes.map((index) => occurrenceSourceNames[index] ?? null),
  };
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
  const scopedFilters =
    typeof statementIndex === 'number'
      ? (() => {
          const explicit = node.metadata?.statementFilters;
          if (explicit && typeof explicit === 'object' && !Array.isArray(explicit)) {
            const value = (explicit as Record<string, unknown>)[String(statementIndex)];
            if (Array.isArray(value)) {
              return value as NonNullable<Node['filters']>;
            }
          }
          return node.filters;
        })()
      : node.filters;

  const scopedOccurrences = getOccurrenceForStatement(node, statementIndex);
  const bodySpans = buildBodySpans(node);
  const bodySourceNames = buildBodySourceNames(node, sourceName);
  const bodyIndexes = getBodyIndexesForStatement(node, statementIndex);
  const scopedBodySpans =
    bodyIndexes.length > 0
      ? bodyIndexes.map((index) => bodySpans[index]).filter((span): span is Span => !!span)
      : node.statementIds.length === 1 && node.statementIds[0] === statementIndex
        ? bodySpans
        : [];
  const scopedBodySourceNames =
    bodyIndexes.length > 0
      ? bodyIndexes.map((index) => bodySourceNames[index] ?? null)
      : bodySpans.length > 0 &&
          node.statementIds.length === 1 &&
          node.statementIds[0] === statementIndex
        ? bodySourceNames
        : [];

  return {
    ...node,
    statementIds: [statementIndex],
    span: scopedOccurrences.spans[0] ?? node.span,
    nameSpans: scopedOccurrences.spans.length > 0 ? scopedOccurrences.spans : node.nameSpans,
    bodySpan: scopedBodySpans[0] ?? undefined,
    aggregation: getAggregationForStatement(node, statementIndex),
    filters: scopedFilters,
    metadata: {
      ...(node.metadata || {}),
      ...(sourceName ? { sourceName } : {}),
      ...(scopedOccurrences.spans.length > 0
        ? {
            [OCCURRENCE_SPANS_METADATA_KEY]: scopedOccurrences.spans,
            [OCCURRENCE_STATEMENT_IDS_METADATA_KEY]: Array.from(
              { length: scopedOccurrences.spans.length },
              () => statementIndex
            ),
            [OCCURRENCE_SOURCE_NAMES_METADATA_KEY]: scopedOccurrences.sourceNames.map(
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

export function mergeNodesForNavigation(
  existing: Node | null,
  incoming: Node,
  sourceName?: string
): Node {
  const nextOccurrenceSpans = buildOccurrenceSpans(incoming);
  const nextOccurrenceSourceNames = buildOccurrenceSourceNames(incoming, sourceName);
  const nextBodySpans = buildBodySpans(incoming);
  const nextBodySourceNames = buildBodySourceNames(incoming, sourceName);

  if (existing === null) {
    return {
      ...incoming,
      nameSpans: nextOccurrenceSpans.length > 0 ? nextOccurrenceSpans : incoming.nameSpans,
      metadata: {
        ...(incoming.metadata || {}),
        ...(getFallbackSourceName(incoming, sourceName)
          ? { sourceName: getFallbackSourceName(incoming, sourceName) }
          : {}),
        ...(nextOccurrenceSpans.length > 0
          ? { [OCCURRENCE_SPANS_METADATA_KEY]: nextOccurrenceSpans }
          : {}),
        ...(nextOccurrenceSourceNames.length > 0
          ? { [OCCURRENCE_SOURCE_NAMES_METADATA_KEY]: nextOccurrenceSourceNames }
          : {}),
        ...(nextBodySpans.length > 0
          ? {
              [BODY_SPANS_METADATA_KEY]: nextBodySpans,
              [BODY_SOURCE_NAMES_METADATA_KEY]: nextBodySourceNames,
            }
          : {}),
      },
    };
  }

  const mergedOccurrenceSpans = [...buildOccurrenceSpans(existing), ...nextOccurrenceSpans];
  const mergedOccurrenceSourceNames = [
    ...buildOccurrenceSourceNames(existing),
    ...nextOccurrenceSourceNames,
  ];
  const mergedBodySpans = [...buildBodySpans(existing), ...nextBodySpans];
  const mergedBodySourceNames = [...buildBodySourceNames(existing), ...nextBodySourceNames];

  return {
    ...existing,
    filters:
      incoming.filters && incoming.filters.length > 0
        ? [...(existing.filters || []), ...incoming.filters]
        : existing.filters,
    nameSpans: mergedOccurrenceSpans.length > 0 ? mergedOccurrenceSpans : existing.nameSpans,
    bodySpan: existing.bodySpan ?? incoming.bodySpan,
    metadata: {
      ...(existing.metadata || {}),
      ...(!existing.metadata?.sourceName && getFallbackSourceName(incoming, sourceName)
        ? { sourceName: getFallbackSourceName(incoming, sourceName) }
        : {}),
      ...(mergedOccurrenceSpans.length > 0
        ? { [OCCURRENCE_SPANS_METADATA_KEY]: mergedOccurrenceSpans }
        : {}),
      ...(mergedOccurrenceSourceNames.length > 0
        ? { [OCCURRENCE_SOURCE_NAMES_METADATA_KEY]: mergedOccurrenceSourceNames }
        : {}),
      ...(mergedBodySpans.length > 0
        ? {
            [BODY_SPANS_METADATA_KEY]: mergedBodySpans,
            [BODY_SOURCE_NAMES_METADATA_KEY]: mergedBodySourceNames,
          }
        : {}),
    },
  };
}

export function resolveNodeSourceName(
  node: Pick<Node, 'metadata' | 'statementIds'>,
  statementById: ReadonlyMap<number, Pick<StatementMeta, 'sourceName'>>
): string | undefined {
  if (typeof node.metadata?.sourceName === 'string') {
    return node.metadata.sourceName;
  }

  const sourceNames = new Set<string>();
  for (const stmtIdx of node.statementIds) {
    const sourceName = statementById.get(stmtIdx)?.sourceName;
    if (sourceName) {
      sourceNames.add(sourceName);
    }
  }

  if (sourceNames.size === 1) {
    return sourceNames.values().next().value;
  }

  return undefined;
}

export function findMergedNodeById(
  result: Pick<AnalyzeResult, 'statements' | 'nodes'>,
  nodeId: string
): Node | null {
  let mergedNode: Node | null = null;

  // Find all occurrences of this node id in the flat graph, then replay the
  // per-node merge. Shared nodes already carry merged spans/filters across
  // their `statementIds`, so replaying once per statement would duplicate the
  // same occurrences.
  const matches = result.nodes.filter((n) => n.id === nodeId);
  if (matches.length === 0) return null;

  const statementById = new Map(result.statements.map((s) => [s.statementIndex, s]));
  for (const node of matches) {
    mergedNode = mergeNodesForNavigation(
      mergedNode,
      node,
      resolveNodeSourceName(node, statementById)
    );
  }

  return mergedNode;
}
