import { describe, expect, it } from 'vitest';

import {
  buildSchemaIdentifiers,
  detectIdentifiers,
  resolveFirstTableReference,
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

describe('resolveFirstTableReference', () => {
  it('returns null when text has no identifiers', () => {
    const schema = makeSchema(['MARA'], []);
    expect(resolveFirstTableReference('no match here', schema)).toBeNull();
  });

  it('returns null when schema is empty', () => {
    const schema = makeSchema([], []);
    expect(resolveFirstTableReference('MARA is a table', schema)).toBeNull();
  });

  it('resolves a direct table reference', () => {
    const schema = makeSchema(['MARA', 'EKKO'], []);
    expect(resolveFirstTableReference('Look at MARA.', schema)).toEqual({
      tableName: 'MARA',
    });
  });

  it('resolves a column reference to its first owning table', () => {
    const schema = makeSchema([], ['MANDT'], { MANDT: ['MARA', 'EKKO'] });
    expect(resolveFirstTableReference('The MANDT column exists.', schema)).toEqual({
      tableName: 'MARA',
      columnName: 'MANDT',
    });
  });

  it('picks the first resolvable identifier in order', () => {
    const schema = makeSchema(['EKKO'], ['MANDT'], { MANDT: ['MARA'] });
    expect(resolveFirstTableReference('First EKKO, then MANDT.', schema)).toEqual({
      tableName: 'EKKO',
    });
  });

  it('skips unresolvable column identifiers with no known owner', () => {
    const schema = makeSchema(['EKKO'], ['MANDT']);
    expect(resolveFirstTableReference('MANDT only here, then EKKO.', schema)).toEqual({
      tableName: 'EKKO',
    });
  });
});
