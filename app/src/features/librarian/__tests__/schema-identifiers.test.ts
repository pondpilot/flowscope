import { describe, expect, it } from 'vitest';

import {
  buildSchemaIdentifiers,
  detectIdentifiers,
  extractSummary,
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

  it('matches a column name (case-insensitive, word-bounded)', () => {
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

  it('matches lower-case variants and normalizes to canonical casing', () => {
    const schema = makeSchema([], ['MANDT']);
    const segments = detectIdentifiers('This mandt is lowercase.', schema);
    expect(segments).toEqual([
      { type: 'text', value: 'This ' },
      { type: 'identifier', value: 'MANDT', kind: 'column' },
      { type: 'text', value: ' is lowercase.' },
    ]);
  });

  it('matches mixed-case variants and normalizes to canonical casing', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    const segments = detectIdentifiers('See Bkpf.MaNdT now.', schema);
    const ids = segments.filter((s) => s.type === 'identifier');
    expect(ids).toEqual([
      { type: 'identifier', value: 'BKPF', kind: 'table' },
      { type: 'identifier', value: 'MANDT', kind: 'column' },
    ]);
  });

  it('does not match a non-identifier word that contains an identifier as a prefix', () => {
    const schema = makeSchema([], ['MANDT']);
    const segments = detectIdentifiers('The mandate is renewed.', schema);
    expect(segments.every((s) => s.type === 'text')).toBe(true);
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

  it('handles missing resolvedSchema and nodes gracefully', () => {
    const result = {} as unknown as Parameters<typeof buildSchemaIdentifiers>[0];
    const ids = buildSchemaIdentifiers(result);
    expect(ids.tables.size).toBe(0);
    expect(ids.columns.size).toBe(0);
  });

  it('falls back to lineage nodes when resolvedSchema is absent', () => {
    const result = {
      nodes: [
        {
          id: 'table-1',
          type: 'table',
          label: 'orders_alias',
          canonicalName: { schema: 'public', name: 'orders' },
        },
        {
          id: 'view-1',
          type: 'view',
          label: 'invoice_view',
          canonicalName: { name: 'invoice_view' },
        },
        {
          id: 'column-1',
          type: 'column',
          label: 'order_id',
          canonicalName: { schema: 'public', name: 'orders', column: 'ORDER_ID' },
        },
      ],
    } as unknown as Parameters<typeof buildSchemaIdentifiers>[0];

    const ids = buildSchemaIdentifiers(result);

    expect([...ids.tables].sort()).toEqual(['invoice_view', 'orders']);
    expect([...ids.columns]).toEqual(['ORDER_ID']);
    expect(ids.columnOwners.get('ORDER_ID')).toEqual(['orders']);
  });

  it('maps column owners correctly for analyzer-shaped canonical names', () => {
    // Matches what `parse_canonical_name` in flowscope-core actually emits:
    // 2-part `orders.total_amount` becomes `{ schema: 'orders', name: 'total_amount' }`,
    // with no `column` field. Without the schema-as-owner fallback this
    // assertion would record `total_amount -> ['total_amount']`.
    const result = {
      nodes: [
        {
          id: 'col-1',
          type: 'column',
          label: 'total_amount',
          canonicalName: { schema: 'orders', name: 'total_amount' },
        },
        {
          id: 'col-2',
          type: 'column',
          label: 'customer_id',
          canonicalName: { catalog: 'main', schema: 'orders', name: 'customer_id' },
        },
      ],
    } as unknown as Parameters<typeof buildSchemaIdentifiers>[0];

    const ids = buildSchemaIdentifiers(result);

    expect([...ids.columns].sort()).toEqual(['customer_id', 'total_amount']);
    expect(ids.columnOwners.get('total_amount')).toEqual(['orders']);
    expect(ids.columnOwners.get('customer_id')).toEqual(['orders']);
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

  it('resolves all four case variants of a qualified reference to the same canonical ref', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    const expected = [{ tableName: 'BKPF', columnName: 'MANDT' }];
    expect(resolveAllReferences('BKPF.MANDT', schema)).toEqual(expected);
    expect(resolveAllReferences('bkpf.MANDT', schema)).toEqual(expected);
    expect(resolveAllReferences('BKPF.mandt', schema)).toEqual(expected);
    expect(resolveAllReferences('bkpf.mandt', schema)).toEqual(expected);
  });

  it('resolves a bare lowercase column to a bare-column reference with canonical casing', () => {
    const schema = makeSchema(['BKPF'], ['MANDT']);
    expect(resolveAllReferences('the mandt column appears everywhere.', schema)).toEqual([
      { columnName: 'MANDT', bareColumn: true },
    ]);
  });
});

describe('extractSummary', () => {
  it('returns empty string for empty input', () => {
    expect(extractSummary('')).toBe('');
  });

  it('returns the original text when no Summary marker is present', () => {
    const text = 'Just a plain answer with no sections.';
    expect(extractSummary(text)).toBe(text);
  });

  it('extracts content between Summary: and Data Lineage:', () => {
    const text =
      'Summary: MANDT is a technical key in BKPF and BSEG.\n' +
      'Data Lineage: bkpf.MANDT = bseg.MANDT.\n' +
      'Documentation: No information.';
    expect(extractSummary(text)).toBe('MANDT is a technical key in BKPF and BSEG.');
  });

  it('extracts content between Summary: and Documentation: when Data Lineage is absent', () => {
    const text = 'Summary: Vendor country lives in LFA1.LAND1.\nDocumentation: see PDF foo.pdf.';
    expect(extractSummary(text)).toBe('Vendor country lives in LFA1.LAND1.');
  });

  it('handles markdown-bold Summary header', () => {
    const text = '**Summary:** Payment block is in RBKP.ZLSPR.\n**Data Lineage:** rbkp.ZLSPR.';
    expect(extractSummary(text)).toBe('Payment block is in RBKP.ZLSPR.');
  });

  it('extracts everything after Summary when no terminating section follows', () => {
    const text = 'Summary: Just one section here, no other markers.';
    expect(extractSummary(text)).toBe('Just one section here, no other markers.');
  });

  it('is case-insensitive on the Summary marker', () => {
    const text = 'summary: lowercase header.\nData Lineage: details.';
    expect(extractSummary(text)).toBe('lowercase header.');
  });
});
