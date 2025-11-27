import { describe, it, expect } from 'vitest';
import type { ResolvedSchemaTable, SchemaTable } from '@pondpilot/flowscope-core';
import { buildSchemaFlowEdges, buildSchemaFlowNodes } from '../src/components/SchemaView';

describe('buildSchemaFlowNodes', () => {
  it('creates nodes for each table in schema', () => {
    const schema: SchemaTable[] = [
      { name: 'users', columns: [{ name: 'id', dataType: 'INT' }] },
      { name: 'orders', columns: [{ name: 'id', dataType: 'INT' }] },
    ];

    const nodes = buildSchemaFlowNodes(schema);

    expect(nodes).toHaveLength(2);
    expect(nodes[0].id).toBe('users');
    expect(nodes[0].type).toBe('schemaTableNode');
    expect(nodes[0].data.label).toBe('users');
    expect(nodes[0].data.columns).toHaveLength(1);
    expect(nodes[1].id).toBe('orders');
  });

  it('handles empty schema', () => {
    const nodes = buildSchemaFlowNodes([]);
    expect(nodes).toHaveLength(0);
  });

  it('handles tables with no columns', () => {
    const schema: SchemaTable[] = [{ name: 'empty_table' }];

    const nodes = buildSchemaFlowNodes(schema);

    expect(nodes).toHaveLength(1);
    expect(nodes[0].data.columns).toEqual([]);
  });

  it('preserves origin for ResolvedSchemaTable', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'users',
        columns: [{ name: 'id' }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'derived_table',
        columns: [{ name: 'id' }],
        origin: 'implied',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const nodes = buildSchemaFlowNodes(schema);

    expect(nodes[0].data.origin).toBe('imported');
    expect(nodes[1].data.origin).toBe('implied');
  });

  it('handles columns with constraint metadata', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'orders',
        columns: [
          { name: 'id', dataType: 'INT', isPrimaryKey: true },
          {
            name: 'user_id',
            dataType: 'INT',
            foreignKey: { table: 'users', column: 'id' },
          },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const nodes = buildSchemaFlowNodes(schema);

    expect(nodes[0].data.columns).toHaveLength(2);
    expect(nodes[0].data.columns[0].isPrimaryKey).toBe(true);
    expect(nodes[0].data.columns[1].foreignKey).toEqual({ table: 'users', column: 'id' });
  });

  it('derives foreign key badges from table-level constraints', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'orders',
        columns: [{ name: 'id', dataType: 'INT', isPrimaryKey: true }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'order_items',
        columns: [
          { name: 'id', dataType: 'INT', isPrimaryKey: true },
          { name: 'order_id', dataType: 'INT' },
        ],
        constraints: [
          {
            constraintType: 'foreign_key',
            columns: ['order_id'],
            referencedTable: 'orders',
            referencedColumns: ['id'],
          },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const nodes = buildSchemaFlowNodes(schema);

    const orderItemsColumns = nodes.find((node) => node.id === 'order_items')?.data.columns;
    expect(orderItemsColumns?.[1].foreignKey).toEqual({ table: 'orders', column: 'id' });
  });
});

describe('buildSchemaFlowEdges', () => {
  it('creates edges for foreign key relationships', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'users',
        columns: [{ name: 'id', dataType: 'INT', isPrimaryKey: true }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'orders',
        columns: [
          { name: 'id', dataType: 'INT', isPrimaryKey: true },
          {
            name: 'user_id',
            dataType: 'INT',
            foreignKey: { table: 'users', column: 'id' },
          },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const edges = buildSchemaFlowEdges(schema);

    expect(edges).toHaveLength(1);
    expect(edges[0].source).toBe('orders');
    expect(edges[0].target).toBe('users');
    expect(edges[0].label).toBe('user_id → id');
    expect(edges[0].id).toBe('fk-orders-user_id-users-id');
  });

  it('returns empty array when no foreign keys exist', () => {
    const schema: SchemaTable[] = [
      { name: 'users', columns: [{ name: 'id', dataType: 'INT' }] },
      { name: 'orders', columns: [{ name: 'id', dataType: 'INT' }] },
    ];

    const edges = buildSchemaFlowEdges(schema);

    expect(edges).toHaveLength(0);
  });

  it('handles empty schema', () => {
    const edges = buildSchemaFlowEdges([]);
    expect(edges).toHaveLength(0);
  });

  it('ignores foreign keys to tables not in schema', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'orders',
        columns: [
          {
            name: 'user_id',
            dataType: 'INT',
            foreignKey: { table: 'users', column: 'id' },
          },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const edges = buildSchemaFlowEdges(schema);

    expect(edges).toHaveLength(0);
  });

  it('creates multiple edges for multiple foreign keys', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'users',
        columns: [{ name: 'id', isPrimaryKey: true }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'products',
        columns: [{ name: 'id', isPrimaryKey: true }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'order_items',
        columns: [
          { name: 'id', isPrimaryKey: true },
          { name: 'user_id', foreignKey: { table: 'users', column: 'id' } },
          { name: 'product_id', foreignKey: { table: 'products', column: 'id' } },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const edges = buildSchemaFlowEdges(schema);

    expect(edges).toHaveLength(2);
    expect(edges.map((e) => e.target).sort()).toEqual(['products', 'users']);
  });

  it('sets correct edge styling properties', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'users',
        schema: 'public',
        columns: [{ name: 'id' }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'orders',
        schema: 'public',
        columns: [{ name: 'user_id', foreignKey: { table: 'users', column: 'id' } }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const edges = buildSchemaFlowEdges(schema);

    expect(edges[0].type).toBe('smoothstep');
    expect(edges[0].animated).toBe(false);
    expect(edges[0].style).toEqual({ stroke: '#6366f1', strokeWidth: 2 });
  });

  it('resolves schema-qualified foreign key targets', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'users',
        schema: 'public',
        columns: [{ name: 'id', isPrimaryKey: true }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'orders',
        schema: 'public',
        columns: [
          { name: 'id', isPrimaryKey: true },
          { name: 'user_id', foreignKey: { table: 'public.users', column: 'id' } },
          { name: 'user_id_quoted', foreignKey: { table: '"public"."users"', column: 'id' } },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const edges = buildSchemaFlowEdges(schema);
    expect(edges).toHaveLength(2);
    edges.forEach((edge) => {
      expect(edge.target).toBe('users');
      expect(edge.source).toBe('orders');
    });
  });

  it('creates edges from table-level foreign key constraints', () => {
    const schema: ResolvedSchemaTable[] = [
      {
        name: 'orders',
        columns: [{ name: 'id', isPrimaryKey: true }],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
      {
        name: 'order_items',
        columns: [
          { name: 'id', isPrimaryKey: true },
          { name: 'order_id' },
        ],
        constraints: [
          {
            constraintType: 'foreign_key',
            columns: ['order_id'],
            referencedTable: 'orders',
            referencedColumns: ['id'],
          },
        ],
        origin: 'imported',
        updatedAt: '2025-01-01T00:00:00Z',
      },
    ];

    const edges = buildSchemaFlowEdges(schema);

    expect(edges).toHaveLength(1);
    expect(edges[0]).toMatchObject({
      source: 'order_items',
      target: 'orders',
      label: 'order_id → id',
      id: 'fk-order_items-order_id-orders-id',
    });
  });
});
