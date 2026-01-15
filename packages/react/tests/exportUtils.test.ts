import { describe, it, expect } from 'vitest';
import {
  extractScriptInfo,
  extractTableInfo,
  extractColumnMappings,
  extractTableDependencies,
  generateMermaidScriptView,
  generateMermaidTableView,
  generateMermaidColumnView,
  generateMermaidHybridView,
  generateStructuredJson,
  generateXlsxWorkbook,
  generateDependencyMatrixSheet,
  sanitizeXlsxValue,
} from '../src/utils/exportUtils';
import type { AnalyzeResult, StatementLineage } from '@pondpilot/flowscope-core';

const createMockResult = (): AnalyzeResult => ({
  statements: [
    {
      statementIndex: 0,
      statementType: 'SELECT',
      sourceName: 'script1.sql',
      joinCount: 0,
      complexityScore: 1,
      nodes: [
        { id: 'table1', type: 'table', label: 'users', qualifiedName: 'public.users' },
        { id: 'table2', type: 'table', label: 'orders', qualifiedName: 'public.orders' },
        { id: 'col1', type: 'column', label: 'user_id' },
        { id: 'col2', type: 'column', label: 'order_id' },
        { id: 'col3', type: 'column', label: 'total' },
      ],
      edges: [
        { id: 'e1', from: 'table1', to: 'col1', type: 'ownership' },
        { id: 'e2', from: 'table2', to: 'col2', type: 'ownership' },
        { id: 'e3', from: 'table2', to: 'col3', type: 'ownership' },
        { id: 'e4', from: 'table1', to: 'table2', type: 'data_flow' },
        { id: 'e5', from: 'col1', to: 'col2', type: 'derivation', expression: 'u.user_id' },
      ],
    },
    {
      statementIndex: 1,
      statementType: 'INSERT',
      sourceName: 'script2.sql',
      joinCount: 0,
      complexityScore: 1,
      nodes: [
        { id: 'table3', type: 'table', label: 'summary', qualifiedName: 'public.summary' },
        { id: 'table2_ref', type: 'table', label: 'orders', qualifiedName: 'public.orders' },
      ],
      edges: [
        { id: 'e6', from: 'table2_ref', to: 'table3', type: 'data_flow' },
      ],
    },
  ],
  globalLineage: { nodes: [], edges: [] },
  issues: [],
  summary: {
    statementCount: 2,
    tableCount: 3,
    columnCount: 3,
    joinCount: 1,
    complexityScore: 25,
    issueCount: { errors: 0, warnings: 1, infos: 0 },
    hasErrors: false,
  },
});

describe('extractScriptInfo', () => {
  it('extracts script information from statements', () => {
    const result = createMockResult();
    const scripts = extractScriptInfo(result.statements);

    expect(scripts).toHaveLength(2);

    const script1 = scripts.find(s => s.sourceName === 'script1.sql');
    expect(script1).toBeDefined();
    expect(script1!.statementCount).toBe(1);
    expect(script1!.tablesRead).toContain('public.users');

    const script2 = scripts.find(s => s.sourceName === 'script2.sql');
    expect(script2).toBeDefined();
    expect(script2!.statementCount).toBe(1);
    expect(script2!.tablesWritten).toContain('public.summary');
  });

  it('correctly distinguishes source and target tables in INSERT statements', () => {
    const result = createMockResult();
    const scripts = extractScriptInfo(result.statements);

    // script2.sql has: INSERT INTO summary SELECT * FROM orders
    // orders should be READ (source), summary should be WRITTEN (target)
    const script2 = scripts.find(s => s.sourceName === 'script2.sql');
    expect(script2).toBeDefined();
    expect(script2!.tablesWritten).toContain('public.summary');
    expect(script2!.tablesWritten).not.toContain('public.orders');
    expect(script2!.tablesRead).toContain('public.orders');
    expect(script2!.tablesRead).not.toContain('public.summary');
  });

  it('handles statements without sourceName', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 1,
        nodes: [{ id: 't1', type: 'table', label: 'test' }],
        edges: [],
      },
    ];

    const scripts = extractScriptInfo(statements);
    expect(scripts).toHaveLength(1);
    expect(scripts[0].sourceName).toBe('default');
  });
});

describe('extractTableInfo', () => {
  it('extracts table information including columns', () => {
    const result = createMockResult();
    const tables = extractTableInfo(result.statements);

    expect(tables.length).toBeGreaterThan(0);

    const usersTable = tables.find(t => t.name === 'users');
    expect(usersTable).toBeDefined();
    expect(usersTable!.qualifiedName).toBe('public.users');
    expect(usersTable!.type).toBe('table');
    expect(usersTable!.columns).toContain('user_id');
  });

  it('identifies CTEs correctly', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 1,
        nodes: [{ id: 'cte1', type: 'cte', label: 'temp_data' }],
        edges: [],
      },
    ];

    const tables = extractTableInfo(statements);
    expect(tables).toHaveLength(1);
    expect(tables[0].type).toBe('cte');
  });
});

describe('extractColumnMappings', () => {
  it('extracts column-to-column lineage mappings', () => {
    const result = createMockResult();
    const mappings = extractColumnMappings(result.statements);

    expect(mappings.length).toBeGreaterThan(0);

    const mapping = mappings.find(m => m.sourceColumn === 'user_id');
    expect(mapping).toBeDefined();
    expect(mapping!.targetColumn).toBe('order_id');
    expect(mapping!.expression).toBe('u.user_id');
    expect(mapping!.edgeType).toBe('derivation');
  });
});

describe('generateMermaidScriptView', () => {
  it('generates valid mermaid flowchart for scripts', () => {
    const result = createMockResult();
    const mermaid = generateMermaidScriptView(result);

    expect(mermaid).toContain('flowchart LR');
    expect(mermaid).toContain('script1_sql');
    expect(mermaid).toContain('script2_sql');
  });

  it('includes edges between scripts with shared tables', () => {
    const result = createMockResult();
    const mermaid = generateMermaidScriptView(result);

    // script1 writes orders, script2 reads orders -> should have edge
    // The mock data has script1 reading users and orders, script2 reading orders and writing summary
    expect(mermaid).toContain('flowchart LR');
  });
});

describe('generateMermaidTableView', () => {
  it('generates valid mermaid flowchart for tables', () => {
    const result = createMockResult();
    const mermaid = generateMermaidTableView(result);

    expect(mermaid).toContain('flowchart LR');
    expect(mermaid).toContain('users');
    expect(mermaid).toContain('orders');
  });
});

describe('generateMermaidColumnView', () => {
  it('generates valid mermaid flowchart for columns', () => {
    const result = createMockResult();
    const mermaid = generateMermaidColumnView(result);

    expect(mermaid).toContain('flowchart LR');
  });

  it('uses dashed arrows for derivations', () => {
    const result = createMockResult();
    const mermaid = generateMermaidColumnView(result);

    // derivation edges should use -.->
    expect(mermaid).toContain('-.->');
  });
});

describe('generateMermaidHybridView', () => {
  it('generates valid mermaid flowchart with scripts and tables', () => {
    const result = createMockResult();
    const mermaid = generateMermaidHybridView(result);

    expect(mermaid).toContain('flowchart LR');
    // Should have both script nodes (with double braces) and table nodes
    expect(mermaid).toContain('{{');
    expect(mermaid).toContain('}}');
  });
});

describe('generateStructuredJson', () => {
  it('generates structured JSON with all sections', () => {
    const result = createMockResult();
    const json = generateStructuredJson(result);

    expect(json.version).toBe('1.0');
    expect(json.exportedAt).toBeDefined();
    expect(json.summary).toEqual(result.summary);
    expect(json.scripts.length).toBeGreaterThan(0);
    expect(json.tables.length).toBeGreaterThan(0);
    expect(json.columnMappings).toBeDefined();
    expect(json.rawResult).toEqual(result);
  });
});

describe('sanitizeXlsxValue', () => {
  it('returns normal strings unchanged', () => {
    expect(sanitizeXlsxValue('hello')).toBe('hello');
    expect(sanitizeXlsxValue('public.users')).toBe('public.users');
    expect(sanitizeXlsxValue('123')).toBe('123');
  });

  it('prefixes values starting with = to prevent formula injection', () => {
    expect(sanitizeXlsxValue('=SUM(A1:A10)')).toBe("'=SUM(A1:A10)");
    expect(sanitizeXlsxValue('=cmd|...')).toBe("'=cmd|...");
  });

  it('prefixes values starting with + to prevent formula injection', () => {
    expect(sanitizeXlsxValue('+1234567890')).toBe("'+1234567890");
  });

  it('prefixes values starting with - to prevent formula injection', () => {
    expect(sanitizeXlsxValue('-1+1')).toBe("'-1+1");
  });

  it('prefixes values starting with @ to prevent formula injection', () => {
    expect(sanitizeXlsxValue('@SUM(A1)')).toBe("'@SUM(A1)");
  });

  it('handles empty strings', () => {
    expect(sanitizeXlsxValue('')).toBe('');
  });
});

describe('extractTableDependencies', () => {
  it('extracts table-to-table dependencies from data_flow edges', () => {
    const result = createMockResult();
    const deps = extractTableDependencies(result.statements);

    // From mock: table1 -> table2 (e4), table2_ref -> table3 (e6)
    expect(deps).toHaveLength(2);

    const dep1 = deps.find(d => d.sourceTable === 'public.users' && d.targetTable === 'public.orders');
    expect(dep1).toBeDefined();

    const dep2 = deps.find(d => d.sourceTable === 'public.orders' && d.targetTable === 'public.summary');
    expect(dep2).toBeDefined();
  });

  it('handles empty statements', () => {
    const deps = extractTableDependencies([]);
    expect(deps).toHaveLength(0);
  });

  it('includes join-only dependencies to output', () => {
    const outputNodeType = 'output' as StatementLineage['nodes'][number]['type'];
    const joinDependencyType = 'join_dependency' as StatementLineage['edges'][number]['type'];
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 1,
        complexityScore: 1,
        nodes: [
          { id: 't1', type: 'table', label: 'source', qualifiedName: 'db.source' },
          { id: 'out1', type: outputNodeType, label: 'Output' },
        ],
        edges: [
          { id: 'e1', from: 't1', to: 'out1', type: joinDependencyType },
        ],
      },
    ];

    const deps = extractTableDependencies(statements);
    expect(deps).toHaveLength(1);
    expect(deps[0]).toEqual({ sourceTable: 'db.source', targetTable: 'Output' });
  });

  it('deduplicates identical dependencies', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 1,
        nodes: [
          { id: 't1', type: 'table', label: 'source', qualifiedName: 'db.source' },
          { id: 't2', type: 'table', label: 'target', qualifiedName: 'db.target' },
        ],
        edges: [
          { id: 'e1', from: 't1', to: 't2', type: 'data_flow' },
          { id: 'e2', from: 't1', to: 't2', type: 'data_flow' },
        ],
      },
    ];

    const deps = extractTableDependencies(statements);
    expect(deps).toHaveLength(1);
  });
});

describe('generateDependencyMatrixSheet', () => {
  it('generates a matrix sheet with tables as rows and columns', () => {
    const deps = [
      { sourceTable: 'A', targetTable: 'B' },
      { sourceTable: 'B', targetTable: 'C' },
    ];

    const sheet = generateDependencyMatrixSheet(deps);

    // Header row: ['', 'A', 'B', 'C']
    expect(sheet['A1']).toEqual({ t: 's', v: '' });
    expect(sheet['B1']).toEqual({ t: 's', v: 'A' });
    expect(sheet['C1']).toEqual({ t: 's', v: 'B' });
    expect(sheet['D1']).toEqual({ t: 's', v: 'C' });

    // Row A: ['A', '-', 'w', '']
    // A writes to B, no relation with C
    expect(sheet['A2']).toEqual({ t: 's', v: 'A' });
    expect(sheet['B2']).toEqual({ t: 's', v: '-' }); // self
    expect(sheet['C2']).toEqual({ t: 's', v: 'w' }); // A writes to B
    expect(sheet['D2']).toEqual({ t: 's', v: '' }); // no A -> C

    // Row B: ['B', 'r', '-', 'w']
    // B reads from A, B writes to C
    expect(sheet['A3']).toEqual({ t: 's', v: 'B' });
    expect(sheet['B3']).toEqual({ t: 's', v: 'r' }); // B reads from A
    expect(sheet['C3']).toEqual({ t: 's', v: '-' }); // self
    expect(sheet['D3']).toEqual({ t: 's', v: 'w' }); // B writes to C

    // Row C: ['C', '', 'r', '-']
    // C reads from B
    expect(sheet['A4']).toEqual({ t: 's', v: 'C' });
    expect(sheet['B4']).toEqual({ t: 's', v: '' }); // no relation
    expect(sheet['C4']).toEqual({ t: 's', v: 'r' }); // C reads from B
    expect(sheet['D4']).toEqual({ t: 's', v: '-' }); // self
  });

  it('handles empty dependencies', () => {
    const sheet = generateDependencyMatrixSheet([]);

    // Should have header + legend rows
    expect(sheet['A1']).toEqual({ t: 's', v: '' }); // empty header corner
    expect(sheet['A3']).toEqual({ t: 's', v: 'Legend:' }); // legend starts after empty row
  });

  it('includes legend at the bottom', () => {
    const deps = [{ sourceTable: 'A', targetTable: 'B' }];
    const sheet = generateDependencyMatrixSheet(deps);

    // Legend should be after the matrix (2 tables = 3 rows: header + 2 data rows)
    // Row 4 is empty, Row 5 is "Legend:"
    expect(sheet['A5']).toEqual({ t: 's', v: 'Legend:' });
    expect(sheet['A6']).toEqual({ t: 's', v: 'w' });
    expect(sheet['A7']).toEqual({ t: 's', v: 'r' });
    expect(sheet['A8']).toEqual({ t: 's', v: '-' });
  });
});

describe('generateXlsxWorkbook', () => {
  it('generates workbook with all required sheets', () => {
    const result = createMockResult();
    const workbook = generateXlsxWorkbook(result);

    expect(workbook.SheetNames).toContain('Scripts');
    expect(workbook.SheetNames).toContain('Tables');
    expect(workbook.SheetNames).toContain('Column Mappings');
    expect(workbook.SheetNames).toContain('Summary');
    expect(workbook.SheetNames).toContain('Dependency Matrix');
  });

  it('includes joinCount and complexityScore in Summary sheet', () => {
    const result = createMockResult();
    const workbook = generateXlsxWorkbook(result);
    const summarySheet = workbook.Sheets['Summary'];

    // Check that joinCount and complexityScore are present
    // The sheet format is: A=Metric, B=Value
    const metrics: string[] = [];
    let row = 2;
    while (summarySheet[`A${row}`]) {
      metrics.push(summarySheet[`A${row}`].v);
      row++;
    }

    expect(metrics).toContain('Total Joins');
    expect(metrics).toContain('Complexity Score');
  });
});
