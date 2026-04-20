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
