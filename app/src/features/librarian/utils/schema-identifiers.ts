import type { AnalyzeResult } from '@pondpilot/flowscope-core';

export type IdentifierKind = 'table' | 'column';

export interface SchemaIdentifiers {
  tables: Set<string>;
  columns: Set<string>;
  /** Map from column name -> tables that contain that column. */
  columnOwners: Map<string, string[]>;
}

export interface IdentifierSegment {
  type: 'text' | 'identifier';
  value: string;
  kind?: IdentifierKind;
}

export interface ChatReference {
  tableName?: string;
  columnName?: string;
  /** True when the reference came from a column identifier with no preceding table qualifier. */
  bareColumn?: boolean;
}

export const EMPTY_SCHEMA_IDENTIFIERS: SchemaIdentifiers = {
  tables: new Set(),
  columns: new Set(),
  columnOwners: new Map(),
};

function isTableLikeNode(type: string | undefined): boolean {
  return type === 'table' || type === 'view' || type === 'cte';
}

function addColumnOwner(
  columnOwners: Map<string, string[]>,
  columnName: string,
  tableName: string
): void {
  const owners = columnOwners.get(columnName) ?? [];
  if (!owners.includes(tableName)) owners.push(tableName);
  columnOwners.set(columnName, owners);
}

export function buildSchemaIdentifiers(
  result: AnalyzeResult | null | undefined
): SchemaIdentifiers {
  const tables = new Set<string>();
  const columns = new Set<string>();
  const columnOwners = new Map<string, string[]>();

  const tablesArr = result?.resolvedSchema?.tables ?? [];
  for (const table of tablesArr) {
    if (table.name) tables.add(table.name);
    for (const col of table.columns) {
      if (!col.name) continue;
      columns.add(col.name);
      addColumnOwner(columnOwners, col.name, table.name);
    }
  }

  // `resolvedSchema` is the source of truth when present, even if it only
  // contributed tables or only columns — falling through to the lineage-node
  // fallback could then mix in stale or duplicated entries. Only fall back
  // when the resolved schema produced nothing at all.
  if (tables.size > 0 || columns.size > 0) {
    return { tables, columns, columnOwners };
  }

  for (const node of result?.nodes ?? []) {
    if (isTableLikeNode(node.type)) {
      const tableName = node.canonicalName?.name ?? node.label;
      if (tableName) tables.add(tableName);
      continue;
    }

    if (node.type === 'column') {
      // Column-node canonical names can arrive in two shapes:
      //   1. Structured: `{ name: <table>, column: <col> }` (when a builder
      //      emits an explicit column canonical name).
      //   2. Parsed qualified name: `{ schema: <table>, name: <col> }` (what
      //      the analyzer's `parse_canonical_name` produces by splitting
      //      `table.col` on the dot — it can't distinguish tables from
      //      columns, so the last part lands in `name`).
      // Prefer the structured shape when `column` is set; otherwise treat
      // `name` as the column and `schema` as the owning table.
      const cn = node.canonicalName;
      const columnName = cn?.column ?? cn?.name ?? node.label;
      if (!columnName) continue;
      columns.add(columnName);
      const tableName = cn?.column ? cn?.name : cn?.schema;
      if (tableName) addColumnOwner(columnOwners, columnName, tableName);
    }
  }

  return { tables, columns, columnOwners };
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Tokenize `text` into text/identifier segments.
 * Matches are case-insensitive and bounded by word boundaries so embedded
 * substrings (e.g. "MANDT" inside "MANDT_X") are not matched. Matched values
 * are normalized to their canonical schema casing.
 */
export function detectIdentifiers(text: string, schema: SchemaIdentifiers): IdentifierSegment[] {
  if (!text) return [];

  if (schema.tables.size === 0 && schema.columns.size === 0) {
    return [{ type: 'text', value: text }];
  }

  // Build lower-cased lookup so we can normalize matches back to canonical
  // names. Tables win over columns when a name appears in both, matching the
  // downstream kind-resolution rule.
  const lookup = new Map<string, string>();
  for (const c of schema.columns) lookup.set(c.toLowerCase(), c);
  for (const t of schema.tables) lookup.set(t.toLowerCase(), t);

  const names = [...lookup.keys()].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`\\b(?:${names.map(escapeRegex).join('|')})\\b`, 'gi');

  const segments: IdentifierSegment[] = [];
  let last = 0;
  let m: RegExpExecArray | null;

  while ((m = pattern.exec(text)) !== null) {
    if (m.index > last) {
      segments.push({ type: 'text', value: text.slice(last, m.index) });
    }
    const canonical = lookup.get(m[0].toLowerCase()) ?? m[0];
    const kind: IdentifierKind = schema.tables.has(canonical) ? 'table' : 'column';
    segments.push({ type: 'identifier', value: canonical, kind });
    last = m.index + m[0].length;
  }

  if (last < text.length) {
    segments.push({ type: 'text', value: text.slice(last) });
  }

  return segments;
}

/**
 * Resolve every schema identifier in `text` to a chat reference.
 *
 * - Table identifiers become `{ tableName }` references.
 * - Column identifiers immediately preceded by a table identifier (separated
 *   only by whitespace and/or a single dot, e.g. `BKPF.MANDT` or `BKPF MANDT`)
 *   become qualified `{ tableName, columnName }` references.
 * - Column identifiers without such a qualifier become bare-column references
 *   `{ columnName, bareColumn: true }` so the caller can decide how to expand
 *   them (e.g. highlight every owning table).
 *
 * Results are deduplicated by `(tableName, columnName)` while preserving the
 * order of first occurrence.
 */
export function resolveAllReferences(text: string, schema: SchemaIdentifiers): ChatReference[] {
  const segments = detectIdentifiers(text, schema);
  const refs: ChatReference[] = [];
  const seen = new Set<string>();

  // Gap between a table identifier and a column identifier: at most one dot,
  // surrounded only by horizontal whitespace. Newlines or anything else break
  // the qualification — `BKPF.\n\nMANDT` and `BKPF..MANDT` are NOT qualified.
  const QUALIFIER_GAP = /^[ \t]*\.?[ \t]*$/;

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i];
    if (seg.type !== 'identifier') continue;

    let ref: ChatReference | null = null;

    if (seg.kind === 'table') {
      // Skip emitting a standalone table reference when the next identifier is
      // a column separated only by whitespace and at most one dot — that column
      // will produce a qualified `{ tableName, columnName }` reference instead.
      let consumedByColumn = false;
      for (let j = i + 1; j < segments.length; j++) {
        const next = segments[j];
        if (next.type === 'text') {
          if (!QUALIFIER_GAP.test(next.value)) break;
          continue;
        }
        if (next.kind === 'column') consumedByColumn = true;
        break;
      }
      if (!consumedByColumn) ref = { tableName: seg.value };
    } else if (seg.kind === 'column') {
      let qualifier: string | null = null;
      for (let j = i - 1; j >= 0; j--) {
        const prev = segments[j];
        if (prev.type === 'text') {
          if (!QUALIFIER_GAP.test(prev.value)) break;
          continue;
        }
        if (prev.kind === 'table') qualifier = prev.value;
        break;
      }
      ref = qualifier
        ? { tableName: qualifier, columnName: seg.value }
        : { columnName: seg.value, bareColumn: true };
    }

    if (!ref) continue;
    const key = `${ref.tableName ?? ''}${ref.columnName ?? ''}`;
    if (seen.has(key)) continue;
    seen.add(key);
    refs.push(ref);
  }

  return refs;
}

/**
 * Extract the Summary block from a structured assistant answer.
 *
 * The Librarian system prompt formats answers in three labelled sections —
 * `Summary`, `Data Lineage`, `Documentation`. Lineage navigation should
 * react only to identifiers in the Summary so a click reflects the
 * answer's main claim and ignores incidental tables named only in the
 * supporting sections. Falls back to the full text when no Summary marker
 * is present.
 */
export function extractSummary(text: string): string {
  if (!text) return '';
  // The Summary marker may appear bold-wrapped in markdown — the closing `**`
  // can sit either before or after the colon (e.g. `**Summary:**` or
  // `**Summary**:`), so allow an optional `**` on each side of `:`.
  const re =
    /(?:\*\*)?\s*Summary\s*(?:\*\*)?\s*:?\s*(?:\*\*)?\s*([\s\S]*?)(?=(?:\*\*)?\s*(?:Data\s*Lineage|Documentation)\s*(?:\*\*)?\s*:|$)/i;
  const m = text.match(re);
  return m ? m[1].trim() : text;
}
