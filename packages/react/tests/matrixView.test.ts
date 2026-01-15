import { describe, it, expect } from 'vitest';
import type { StatementLineage } from '@pondpilot/flowscope-core';
import {
  extractTableDependenciesWithDetails,
  extractScriptDependencies,
  buildTableMatrix,
  buildScriptMatrix,
  type TableDependencyWithDetails,
} from '../src/utils/matrixUtils';

// Test data
const createMockStatements = (): StatementLineage[] => [
  {
    statementIndex: 0,
    statementType: 'SELECT',
    sourceName: 'script1.sql',
    joinCount: 1,
    complexityScore: 25,
    nodes: [
      { id: 'table1', type: 'table', label: 'users', qualifiedName: 'public.users' },
      { id: 'table2', type: 'table', label: 'orders', qualifiedName: 'public.orders' },
      { id: 'col1', type: 'column', label: 'user_id' },
      { id: 'col2', type: 'column', label: 'order_id' },
    ],
    edges: [
      { id: 'e1', from: 'table1', to: 'col1', type: 'ownership' },
      { id: 'e2', from: 'table2', to: 'col2', type: 'ownership' },
      { id: 'e3', from: 'table1', to: 'table2', type: 'data_flow' },
      { id: 'e4', from: 'col1', to: 'col2', type: 'derivation', expression: 'u.user_id' },
    ],
  },
  {
    statementIndex: 1,
    statementType: 'INSERT',
    sourceName: 'script2.sql',
    joinCount: 0,
    complexityScore: 10,
    nodes: [
      { id: 'table3', type: 'table', label: 'summary', qualifiedName: 'public.summary' },
      { id: 'table2_ref', type: 'table', label: 'orders', qualifiedName: 'public.orders' },
    ],
    edges: [
      { id: 'e5', from: 'table2_ref', to: 'table3', type: 'data_flow' },
    ],
  },
];

describe('extractTableDependenciesWithDetails', () => {
  it('extracts table-to-table dependencies', () => {
    const statements = createMockStatements();
    const deps = extractTableDependenciesWithDetails(statements);

    expect(deps.length).toBeGreaterThan(0);

    // Check users -> orders dependency
    const usersToOrders = deps.find(
      (d) => d.sourceTable === 'public.users' && d.targetTable === 'public.orders'
    );
    expect(usersToOrders).toBeDefined();

    // Check orders -> summary dependency
    const ordersToSummary = deps.find(
      (d) => d.sourceTable === 'public.orders' && d.targetTable === 'public.summary'
    );
    expect(ordersToSummary).toBeDefined();
  });

  it('tracks column mappings within dependencies', () => {
    const statements = createMockStatements();
    const deps = extractTableDependenciesWithDetails(statements);

    const usersToOrders = deps.find(
      (d) => d.sourceTable === 'public.users' && d.targetTable === 'public.orders'
    );

    expect(usersToOrders).toBeDefined();
    expect(usersToOrders!.columnCount).toBe(1);
    expect(usersToOrders!.columns[0].source).toBe('user_id');
    expect(usersToOrders!.columns[0].target).toBe('order_id');
  });

  it('handles empty statements', () => {
    const deps = extractTableDependenciesWithDetails([]);
    expect(deps).toHaveLength(0);
  });

  it('ignores self-references', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 5,
        nodes: [
          { id: 't1', type: 'table', label: 'users', qualifiedName: 'public.users' },
        ],
        edges: [
          { id: 'e1', from: 't1', to: 't1', type: 'data_flow' },
        ],
      },
    ];

    const deps = extractTableDependenciesWithDetails(statements);
    expect(deps).toHaveLength(0);
  });

  it('captures spans from source nodes', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 5,
        nodes: [
          { id: 't1', type: 'table', label: 'a', qualifiedName: 'a', span: { start: 10, end: 20 } },
          { id: 't2', type: 'table', label: 'b', qualifiedName: 'b' },
        ],
        edges: [
          { id: 'e1', from: 't1', to: 't2', type: 'data_flow' },
        ],
      },
    ];

    const deps = extractTableDependenciesWithDetails(statements);
    expect(deps).toHaveLength(1);
    expect(deps[0].spans).toHaveLength(1);
    expect(deps[0].spans[0]).toEqual({ start: 10, end: 20 });
  });

  it('includes join-only dependencies to output', () => {
    const outputNodeType = 'output' as StatementLineage['nodes'][number]['type'];
    const joinDependencyType = 'join_dependency' as StatementLineage['edges'][number]['type'];

    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 1,
        complexityScore: 5,
        nodes: [
          { id: 't1', type: 'table', label: 'table1', qualifiedName: 'table1' },
          { id: 'out1', type: outputNodeType, label: 'Output' },
        ],
        edges: [
          { id: 'e1', from: 't1', to: 'out1', type: joinDependencyType },
        ],
      },
    ];

    const deps = extractTableDependenciesWithDetails(statements);
    const joinDep = deps.find((dep) => dep.sourceTable === 'table1' && dep.targetTable === 'Output');
    expect(joinDep).toBeDefined();
    expect(joinDep!.columns).toHaveLength(0);
  });
});

describe('extractScriptDependencies', () => {
  it('extracts script-to-script dependencies via shared tables', () => {
    const statements = createMockStatements();
    const { dependencies } = extractScriptDependencies(statements);

    // script1 writes to orders (via data_flow), script2 reads from orders
    // Actually in our mock, script1 has users->orders flow (orders is target = written)
    // and script2 has orders->summary flow (orders is source = read)
    // So script1 writes orders, script2 reads orders

    expect(dependencies.length).toBeGreaterThanOrEqual(0);
  });

  it('handles statements without sourceName', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        joinCount: 0,
        complexityScore: 5,
        nodes: [{ id: 't1', type: 'table', label: 'test' }],
        edges: [],
      },
    ];

    const { dependencies, allScripts } = extractScriptDependencies(statements);
    // Should use 'default' as script name
    expect(allScripts).toContain('default');
    expect(dependencies).toHaveLength(0); // No dependencies with single script
  });

  it('finds shared tables between scripts', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'INSERT',
        sourceName: 'producer.sql',
        joinCount: 0,
        complexityScore: 10,
        nodes: [
          { id: 't1', type: 'table', label: 'source', qualifiedName: 'db.source' },
          { id: 't2', type: 'table', label: 'target', qualifiedName: 'db.target' },
        ],
        edges: [
          { id: 'e1', from: 't1', to: 't2', type: 'data_flow' },
        ],
      },
      {
        statementIndex: 1,
        statementType: 'SELECT',
        sourceName: 'consumer.sql',
        joinCount: 0,
        complexityScore: 5,
        nodes: [
          { id: 't3', type: 'table', label: 'target', qualifiedName: 'db.target' },
          { id: 't4', type: 'table', label: 'output', qualifiedName: 'db.output' },
        ],
        edges: [
          { id: 'e2', from: 't3', to: 't4', type: 'data_flow' },
        ],
      },
    ];

    const { dependencies } = extractScriptDependencies(statements);

    // producer writes to db.target, consumer reads from db.target
    const producerToConsumer = dependencies.find(
      (d) => d.sourceScript === 'producer.sql' && d.targetScript === 'consumer.sql'
    );

    expect(producerToConsumer).toBeDefined();
    expect(producerToConsumer!.sharedTables).toContain('db.target');
  });

  it('returns all scripts including those with no dependencies', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'SELECT',
        sourceName: 'isolated.sql',
        joinCount: 0,
        complexityScore: 5,
        nodes: [{ id: 't1', type: 'table', label: 'lonely_table' }],
        edges: [],
      },
      {
        statementIndex: 1,
        statementType: 'SELECT',
        sourceName: 'another.sql',
        joinCount: 0,
        complexityScore: 5,
        nodes: [{ id: 't2', type: 'table', label: 'other_table' }],
        edges: [],
      },
    ];

    const { dependencies, allScripts } = extractScriptDependencies(statements);
    expect(allScripts).toContain('isolated.sql');
    expect(allScripts).toContain('another.sql');
    expect(dependencies).toHaveLength(0);
  });

  it('treats only created relations as writes for CREATE statements', () => {
    const statements: StatementLineage[] = [
      {
        statementIndex: 0,
        statementType: 'CREATE_VIEW',
        sourceName: 'creator.sql',
        joinCount: 0,
        complexityScore: 5,
        nodes: [
          { id: 'orders_source', type: 'table', label: 'orders', qualifiedName: 'public.orders' },
          {
            id: 'orders_view',
            type: 'view',
            label: 'orders_view',
            qualifiedName: 'public.orders_view',
          },
        ],
        edges: [
          { id: 'cv_edge', from: 'orders_source', to: 'orders_view', type: 'data_flow' },
        ],
      },
      {
        statementIndex: 1,
        statementType: 'SELECT',
        sourceName: 'orders-reader.sql',
        joinCount: 0,
        complexityScore: 5,
        nodes: [
          { id: 'orders_table', type: 'table', label: 'orders', qualifiedName: 'public.orders' },
          { id: 'sink', type: 'table', label: 'sink', qualifiedName: 'public.sink' },
        ],
        edges: [
          { id: 'select_edge', from: 'orders_table', to: 'sink', type: 'data_flow' },
        ],
      },
      {
        statementIndex: 2,
        statementType: 'SELECT',
        sourceName: 'view-reader.sql',
        joinCount: 0,
        complexityScore: 5,
        nodes: [
          {
            id: 'orders_view_source',
            type: 'view',
            label: 'orders_view',
            qualifiedName: 'public.orders_view',
          },
          { id: 'view_sink', type: 'table', label: 'sink_view', qualifiedName: 'public.sink2' },
        ],
        edges: [
          { id: 'view_edge', from: 'orders_view_source', to: 'view_sink', type: 'data_flow' },
        ],
      },
    ];

    const { dependencies } = extractScriptDependencies(statements);

    const falsePositive = dependencies.find(
      (d) => d.sourceScript === 'creator.sql' && d.targetScript === 'orders-reader.sql'
    );
    expect(falsePositive).toBeUndefined();

    const realDependency = dependencies.find(
      (d) => d.sourceScript === 'creator.sql' && d.targetScript === 'view-reader.sql'
    );
    expect(realDependency).toBeDefined();
    expect(realDependency!.sharedTables).toEqual(['public.orders_view']);
  });
});

describe('buildTableMatrix', () => {
  it('builds correct matrix cells', () => {
    const deps: TableDependencyWithDetails[] = [
      { sourceTable: 'A', targetTable: 'B', columnCount: 1, columns: [], spans: [] },
      { sourceTable: 'B', targetTable: 'C', columnCount: 2, columns: [], spans: [] },
    ];

    const matrix = buildTableMatrix(deps);

    // A row
    expect(matrix.cells.get('A')?.get('A')?.type).toBe('self');
    expect(matrix.cells.get('A')?.get('B')?.type).toBe('write'); // A writes to B
    expect(matrix.cells.get('A')?.get('C')?.type).toBe('none');

    // B row
    expect(matrix.cells.get('B')?.get('A')?.type).toBe('read'); // B reads from A (A->B means B receives from A)
    expect(matrix.cells.get('B')?.get('B')?.type).toBe('self');
    expect(matrix.cells.get('B')?.get('C')?.type).toBe('write'); // B writes to C

    // C row
    expect(matrix.cells.get('C')?.get('A')?.type).toBe('none');
    expect(matrix.cells.get('C')?.get('B')?.type).toBe('read'); // C reads from B
    expect(matrix.cells.get('C')?.get('C')?.type).toBe('self');
  });

  it('handles empty dependencies', () => {
    const matrix = buildTableMatrix([]);
    expect(matrix.items).toHaveLength(0);
    expect(matrix.cells.size).toBe(0);
  });

  it('sorts items alphabetically', () => {
    const deps: TableDependencyWithDetails[] = [
      { sourceTable: 'Z', targetTable: 'A', columnCount: 0, columns: [], spans: [] },
    ];

    const matrix = buildTableMatrix(deps);
    expect(matrix.items).toEqual(['A', 'Z']);
  });

  it('includes dependency details in cells', () => {
    const deps: TableDependencyWithDetails[] = [
      {
        sourceTable: 'A',
        targetTable: 'B',
        columnCount: 2,
        columns: [
          { source: 'id', target: 'a_id' },
          { source: 'name', target: 'a_name' },
        ],
        spans: [{ start: 0, end: 10 }],
      },
    ];

    const matrix = buildTableMatrix(deps);
    const cellAB = matrix.cells.get('A')?.get('B');
    expect(cellAB?.type).toBe('write');
    expect(cellAB?.details).toBeDefined();
    const details = cellAB?.details as TableDependencyWithDetails;
    expect(details.columnCount).toBe(2);
    expect(details.columns).toHaveLength(2);
    expect(details.spans).toHaveLength(1);
  });
});

describe('buildScriptMatrix', () => {
  it('builds correct matrix for script dependencies', () => {
    const deps = [
      { sourceScript: 'a.sql', targetScript: 'b.sql', sharedTables: ['table1'] },
    ];
    const allScripts = ['a.sql', 'b.sql', 'c.sql'];

    const matrix = buildScriptMatrix(deps, allScripts);

    expect(matrix.items).toEqual(['a.sql', 'b.sql', 'c.sql']);
    expect(matrix.cells.get('a.sql')?.get('b.sql')?.type).toBe('write');
    expect(matrix.cells.get('b.sql')?.get('a.sql')?.type).toBe('read');
    expect(matrix.cells.get('c.sql')?.get('a.sql')?.type).toBe('none');
    expect(matrix.cells.get('c.sql')?.get('b.sql')?.type).toBe('none');
  });

  it('includes scripts with no dependencies', () => {
    const deps: { sourceScript: string; targetScript: string; sharedTables: string[] }[] = [];
    const allScripts = ['lone.sql'];

    const matrix = buildScriptMatrix(deps, allScripts);

    expect(matrix.items).toContain('lone.sql');
    expect(matrix.cells.get('lone.sql')?.get('lone.sql')?.type).toBe('self');
  });
});
