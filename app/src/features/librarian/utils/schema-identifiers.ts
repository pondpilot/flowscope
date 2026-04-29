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

export interface SchemaReference {
  tableName: string;
  /** Present when the source identifier was a column; set to that column name. */
  columnName?: string;
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
      const owners = columnOwners.get(col.name) ?? [];
      if (!owners.includes(table.name)) owners.push(table.name);
      columnOwners.set(col.name, owners);
    }
  }

  return { tables, columns, columnOwners };
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Tokenize `text` into text/identifier segments.
 * Matches are case-sensitive and bounded by word boundaries so embedded
 * substrings (e.g. "MANDT" inside "MANDT_X") are not matched.
 */
export function detectIdentifiers(
  text: string,
  schema: SchemaIdentifiers
): IdentifierSegment[] {
  if (!text) return [];

  if (schema.tables.size === 0 && schema.columns.size === 0) {
    return [{ type: 'text', value: text }];
  }

  const names = new Set<string>([...schema.tables, ...schema.columns]);
  const sorted = [...names].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`\\b(?:${sorted.map(escapeRegex).join('|')})\\b`, 'g');

  const segments: IdentifierSegment[] = [];
  let last = 0;
  let m: RegExpExecArray | null;

  while ((m = pattern.exec(text)) !== null) {
    if (m.index > last) {
      segments.push({ type: 'text', value: text.slice(last, m.index) });
    }
    const value = m[0];
    const kind: IdentifierKind = schema.tables.has(value) ? 'table' : 'column';
    segments.push({ type: 'identifier', value, kind });
    last = m.index + value.length;
  }

  if (last < text.length) {
    segments.push({ type: 'text', value: text.slice(last) });
  }

  return segments;
}

/**
 * Resolve the first schema identifier in `text` to a table reference.
 * Table identifiers resolve to themselves; column identifiers resolve to
 * their first known owning table. Returns null when no resolvable
 * identifier is found.
 */
export function resolveFirstTableReference(
  text: string,
  schema: SchemaIdentifiers
): SchemaReference | null {
  const segments = detectIdentifiers(text, schema);
  for (const seg of segments) {
    if (seg.type !== 'identifier') continue;
    if (seg.kind === 'table') {
      return { tableName: seg.value };
    }
    if (seg.kind === 'column') {
      const owners = schema.columnOwners.get(seg.value);
      if (owners && owners.length > 0) {
        return { tableName: owners[0], columnName: seg.value };
      }
    }
  }
  return null;
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
export function resolveAllReferences(
  text: string,
  schema: SchemaIdentifiers
): ChatReference[] {
  const segments = detectIdentifiers(text, schema);
  const refs: ChatReference[] = [];
  const seen = new Set<string>();

  for (let i = 0; i < segments.length; i++) {
    const seg = segments[i];
    if (seg.type !== 'identifier') continue;

    let ref: ChatReference | null = null;

    if (seg.kind === 'table') {
      // Skip emitting a standalone table reference when the next identifier is
      // a column separated only by whitespace and/or a dot — that column will
      // produce a qualified `{ tableName, columnName }` reference instead.
      let consumedByColumn = false;
      for (let j = i + 1; j < segments.length; j++) {
        const next = segments[j];
        if (next.type === 'text') {
          if (!/^[.\s]*$/.test(next.value)) break;
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
          if (!/^[.\s]*$/.test(prev.value)) break;
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
