import { describe, expect, it } from 'vitest';

import {
  buildSchemaIdentifiers,
  detectIdentifiers,
  resolveAllReferences,
  type SchemaIdentifiers,
} from '../utils/schema-identifiers';

function makeSchema(
  tables: string[],
  columns: string[],
  columnOwners: Record<string, string[]> = {}
): SchemaIdentifiers {
  return {
    tables: new Set(tables),
    columns: new Set(columns),
    columnOwners: new Map(Object.entries(columnOwners)),
  };
}

describe('detectIdentifiers', () => {
  it('returns a single text segment when schema is empty', () => {
    const schema = makeSchema([], []);
    const segments = detectIdentifiers('Hello MANDT world', schema);
    expect(segments).toEqual([{ type: 'text', value: 'Hello MANDT world' }]);
  });

  it('returns empty array for empty text', () => {
    const schema = makeSchema(['MANDT'], []);
    expect(detectIdentifiers('', schema)).toEqual([]);
  });

  it('matches a column name exactly (case-sensitive, word-bounded)', () => {
    const schema = makeSchema([], ['MANDT']);
    const segments = detectIdentifiers('Client is MANDT.', schema);
    expect(segments).toEqual([
      { type: 'text', value: 'Client is ' },
      { type: 'identifier', value: 'MANDT', kind: 'column' },
      { type: 'text', value: '.' },
    ]);
  });

  it('matches a table name exactly', () => {
    const schema = makeSchema(['ekko'], []);
    const segments = detectIdentifiers('See ekko for details.', schema);
    expect(segments[1]).toEqual({ type: 'identifier', value: 'ekko', kind: 'table' });
  });

  it('does not match lower-case variants of an upper-case identifier', () => {
    const schema = makeSchema([], ['MANDT']);
    const segments = detectIdentifiers('This mandt is lowercase.', schema);
    expect(segments).toEqual([{ type: 'text', value: 'This mandt is lowercase.' }]);
  });

  it('does not match embedded substrings (word boundary)', () => {
    const schema = makeSchema([], ['MANDT']);
    const segments = detectIdentifiers('MANDT_X and xMANDT and MANDTy', schema);
    expect(segments.every((s) => s.type === 'text')).toBe(true);
  });

  it('matches multiple identifiers on a single line', () => {
    const schema = makeSchema(['ekko', 'ekpo'], ['EBELN', 'EBELP']);
    const segments = detectIdentifiers('Join ekko.EBELN = ekpo.EBELP today.', schema);
    const identifiers = segments.filter((s) => s.type === 'identifier').map((s) => s.value);
    expect(identifiers).toEqual(['ekko', 'EBELN', 'ekpo', 'EBELP']);
  });

  it('marks identifiers present in both tables and columns as tables', () => {
    const schema = makeSchema(['shared'], ['shared']);
    const segments = detectIdentifiers('the shared name', schema);
    const id = segments.find((s) => s.type === 'identifier');
    expect(id?.kind).toBe('table');
  });

  it('handles identifiers at start and end of text', () => {
    const schema = makeSchema([], ['MANDT', 'BUKRS']);
    const segments = detectIdentifiers('MANDT is here BUKRS', schema);
    expect(segments[0]).toEqual({ type: 'identifier', value: 'MANDT', kind: 'column' });
    expect(segments[segments.length - 1]).toEqual({
      type: 'identifier',
      value: 'BUKRS',
      kind: 'column',
    });
  });

  it('matches identifiers surrounded by punctuation (backticks, parens)', () => {
    const schema = makeSchema([], ['MANDT']);
    const segments = detectIdentifiers('Use `MANDT` and (MANDT).', schema);
    const matches = segments.filter((s) => s.type === 'identifier');
    expect(matches).toHaveLength(2);
    expect(matches.every((m) => m.value === 'MANDT')).toBe(true);
  });
});

describe('buildSchemaIdentifiers', () => {
  it('returns an empty set when result is null/undefined', () => {
    const fromNull = buildSchemaIdentifiers(null);
    const fromUndef = buildSchemaIdentifiers(undefined);
    expect(fromNull.tables.size).toBe(0);
    expect(fromNull.columns.size).toBe(0);
    expect(fromUndef.tables.size).toBe(0);
  });

  it('collects table and column names from resolvedSchema', () => {
    const result = {
      resolvedSchema: {
        tables: [
          {
            name: 'ekko',
            columns: [{ name: 'EBELN' }, { name: 'MANDT' }],
          },
          {
            name: 'ekpo',
            columns: [{ name: 'EBELN' }, { name: 'EBELP' }],
          },
        ],
      },
    } as unknown as Parameters<typeof buildSchemaIdentifiers>[0];

    const ids = buildSchemaIdentifiers(result);
    expect([...ids.tables].sort()).toEqual(['ekko', 'ekpo']);
    expect([...ids.columns].sort()).toEqual(['EBELN', 'EBELP', 'MANDT']);
    expect(ids.columnOwners.get('EBELN')?.sort()).toEqual(['ekko', 'ekpo']);
    expect(ids.columnOwners.get('MANDT')).toEqual(['ekko']);
  });

  it('handles missing resolvedSchema gracefully', () => {
    const result = {} as unknown as Parameters<typeof buildSchemaIdentifiers>[0];
    const ids = buildSchemaIdentifiers(result);
    expect(ids.tables.size).toBe(0);
    expect(ids.columns.size).toBe(0);
  });
});

describe('resolveAllReferences', () => {
  it('returns an empty list when text has no identifiers', () => {
    const schema = makeSchema(['MARA'], ['MANDT']);
    expect(resolveAllReferences('Nothing to resolve here.', schema)).toEqual([]);
  });

  it('returns an empty list for unknown identifiers (skipped silently)', () => {
    const schema = makeSchema(['MARA'], ['MANDT']);
    expect(resolveAllReferences('UNKNOWN_TABLE and OTHER_COL', schema)).toEqual([]);
  });

  it('resolves a dotted column as a qualified reference', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('Look at BKPF.MANDT now.', schema)).toEqual([
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
  });

  it('resolves a space-separated table+column as a qualified reference', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('Look at BKPF MANDT now.', schema)).toEqual([
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
  });

  it('resolves a standalone table as a table reference', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('Look at BKPF now.', schema)).toEqual([{ tableName: 'BKPF' }]);
  });

  it('resolves a standalone column as a bare-column reference', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('The MANDT column appears everywhere.', schema)).toEqual([
      { columnName: 'MANDT', bareColumn: true },
    ]);
  });

  it('does not qualify a column when the preceding text is more than a separator', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('BKPF and then MANDT.', schema)).toEqual([
      { tableName: 'BKPF' },
      { columnName: 'MANDT', bareColumn: true },
    ]);
  });

  it('does not qualify a column when the preceding identifier is another column', () => {
    const schema = makeSchema([], ['MANDT', 'BUKRS']);
    expect(resolveAllReferences('MANDT BUKRS pair', schema)).toEqual([
      { columnName: 'MANDT', bareColumn: true },
      { columnName: 'BUKRS', bareColumn: true },
    ]);
  });

  it('returns mixed references in order, deduplicated', () => {
    const schema = makeSchema(['BKPF', 'BSEG'], ['MANDT', 'BUKRS']);
    const refs = resolveAllReferences(
      'BKPF.MANDT joins BSEG.MANDT; mention MANDT alone, then BKPF.MANDT again, and BSEG too.',
      schema
    );
    expect(refs).toEqual([
      { tableName: 'BKPF', columnName: 'MANDT' },
      { tableName: 'BSEG', columnName: 'MANDT' },
      { columnName: 'MANDT', bareColumn: true },
      { tableName: 'BSEG' },
    ]);
  });

  it('treats names that are both tables and columns as table references', () => {
    const schema = makeSchema(['shared'], ['shared']);
    expect(resolveAllReferences('the shared name', schema)).toEqual([{ tableName: 'shared' }]);
  });

  it('does not qualify across a newline gap', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('Tables: BKPF.\n\nKey columns: MANDT.', schema)).toEqual([
      { tableName: 'BKPF' },
      { columnName: 'MANDT', bareColumn: true },
    ]);
  });

  it('does not qualify when the gap contains more than one dot', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('BKPF..MANDT', schema)).toEqual([
      { tableName: 'BKPF' },
      { columnName: 'MANDT', bareColumn: true },
    ]);
  });

  it('qualifies through a single dot surrounded by horizontal whitespace', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('BKPF . MANDT is the key', schema)).toEqual([
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
  });
});
