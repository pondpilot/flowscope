import type { AnalyzeResult, Node, Span, StatementLineage } from '@pondpilot/flowscope-core';

const OCCURRENCE_SOURCE_NAMES_METADATA_KEY = 'occurrenceSourceNames';
const BODY_SPANS_METADATA_KEY = 'bodySpans';
const BODY_SOURCE_NAMES_METADATA_KEY = 'bodySourceNames';

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

function getFallbackSourceName(node: Node, sourceName?: string): string | null {
  if (typeof node.metadata?.sourceName === 'string') {
    return node.metadata.sourceName;
  }
  return sourceName ?? null;
}

function buildOccurrenceSourceNames(node: Node, sourceName?: string): Array<string | null> {
  const spanCount = node.nameSpans?.length ?? 0;
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

export function mergeNodesForNavigation(
  existing: Node | null,
  incoming: Node,
  sourceName?: string
): Node {
  const nextOccurrenceSourceNames = buildOccurrenceSourceNames(incoming, sourceName);
  const nextBodySpans = buildBodySpans(incoming);
  const nextBodySourceNames = buildBodySourceNames(incoming, sourceName);

  if (existing === null) {
    return {
      ...incoming,
      metadata: {
        ...(incoming.metadata || {}),
        ...(getFallbackSourceName(incoming, sourceName)
          ? { sourceName: getFallbackSourceName(incoming, sourceName) }
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

  const mergedNameSpans = [...(existing.nameSpans ?? []), ...(incoming.nameSpans ?? [])];
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
    nameSpans: mergedNameSpans.length > 0 ? mergedNameSpans : existing.nameSpans,
    bodySpan: existing.bodySpan ?? incoming.bodySpan,
    metadata: {
      ...(existing.metadata || {}),
      ...(!existing.metadata?.sourceName && getFallbackSourceName(incoming, sourceName)
        ? { sourceName: getFallbackSourceName(incoming, sourceName) }
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

export function findMergedNodeById(
  result: Pick<AnalyzeResult, 'statements'>,
  nodeId: string
): Node | null {
  let mergedNode: Node | null = null;

  for (const statement of result.statements) {
    for (const node of statement.nodes) {
      if (node.id === nodeId) {
        mergedNode = mergeNodesForNavigation(mergedNode, node, statement.sourceName);
      }
    }
  }

  return mergedNode;
}

export function mergeStatementNodesForNavigation(statements: StatementLineage[]): Node[] {
  const mergedNodes = new Map<string, Node>();

  statements.forEach((statement) => {
    statement.nodes.forEach((node) => {
      const mergedNode = mergeNodesForNavigation(
        mergedNodes.get(node.id) ?? null,
        node,
        statement.sourceName
      );
      mergedNodes.set(node.id, mergedNode);
    });
  });

  return Array.from(mergedNodes.values());
}
