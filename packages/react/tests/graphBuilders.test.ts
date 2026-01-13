import { describe, it, expect } from 'vitest';
import type { StatementLineage } from '@pondpilot/flowscope-core';
import { buildFlowEdges, buildFlowNodes, mergeStatements, computeIsCollapsed } from '../src/utils/graphBuilders';
import { GRAPH_CONFIG } from '../src/constants';

describe('computeIsCollapsed', () => {
  it('returns true when defaultCollapsed is true and node is not in overrides', () => {
    const overrides = new Set<string>();
    expect(computeIsCollapsed('node-1', true, overrides)).toBe(true);
  });

  it('returns false when defaultCollapsed is true and node is in overrides (expanded)', () => {
    const overrides = new Set(['node-1']);
    expect(computeIsCollapsed('node-1', true, overrides)).toBe(false);
  });

  it('returns false when defaultCollapsed is false and node is not in overrides', () => {
    const overrides = new Set<string>();
    expect(computeIsCollapsed('node-1', false, overrides)).toBe(false);
  });

  it('returns true when defaultCollapsed is false and node is in overrides (collapsed)', () => {
    const overrides = new Set(['node-1']);
    expect(computeIsCollapsed('node-1', false, overrides)).toBe(true);
  });
});

const createInsertLineage = (): StatementLineage => ({
  statementIndex: 0,
  statementType: 'INSERT',
  nodes: [
    {
      id: 'table:staging.orders',
      type: 'table',
      label: 'staging.orders',
      qualifiedName: 'staging.orders',
    },
    {
      id: 'table:analytics.tgt_orders',
      type: 'table',
      label: 'analytics.tgt_orders',
      qualifiedName: 'analytics.tgt_orders',
    },
    {
      id: 'column:staging.orders.order_id',
      type: 'column',
      label: 'order_id',
      qualifiedName: 'staging.orders.order_id',
    },
    {
      id: 'column:analytics.tgt_orders.order_id',
      type: 'column',
      label: 'order_id',
      qualifiedName: 'analytics.tgt_orders.order_id',
    },
    {
      // Simulates SELECT projection feeding the INSERT target (no qualified name)
      id: 'column:projection.order_id',
      type: 'column',
      label: 'order_id',
    },
  ],
  edges: [
    {
      id: 'edge:own:src',
      from: 'table:staging.orders',
      to: 'column:staging.orders.order_id',
      type: 'ownership',
    },
    {
      id: 'edge:own:tgt',
      from: 'table:analytics.tgt_orders',
      to: 'column:analytics.tgt_orders.order_id',
      type: 'ownership',
    },
    {
      id: 'edge:data:src_to_tgt',
      from: 'column:staging.orders.order_id',
      to: 'column:analytics.tgt_orders.order_id',
      type: 'data_flow',
    },
    {
      id: 'edge:der:projection',
      from: 'column:staging.orders.order_id',
      to: 'column:projection.order_id',
      type: 'derivation',
    },
  ],
});

describe('mergeStatements', () => {
  it('preserves join metadata from later statements referencing the same table', () => {
    const firstStmt: StatementLineage = {
      statementIndex: 0,
      statementType: 'SELECT',
      nodes: [
        { id: 'table:users', type: 'table', label: 'users', qualifiedName: 'users' },
      ],
      edges: [],
      joinCount: 0,
      complexityScore: 1,
    };

    const secondStmt: StatementLineage = {
      statementIndex: 1,
      statementType: 'SELECT',
      nodes: [
        {
          id: 'table:users',
          type: 'table',
          label: 'users',
          qualifiedName: 'users',
          joinType: 'LEFT',
          joinCondition: 'u.id = o.user_id',
        },
      ],
      edges: [],
      joinCount: 1,
      complexityScore: 1,
    };

    const merged = mergeStatements([firstStmt, secondStmt]);
    const usersNode = merged.nodes.find((n) => n.id === 'table:users');
    expect(usersNode?.joinType).toBe('LEFT');
    expect(usersNode?.joinCondition).toBe('u.id = o.user_id');
  });
});

describe('graphBuilders DML handling', () => {
  it('renders INSERT lineage into the real target even with unqualified columns present', () => {
    const statement = createInsertLineage();

    const flowEdges = buildFlowEdges(statement);
    expect(flowEdges).toHaveLength(1);
    expect(flowEdges[0]).toMatchObject({
      source: 'table:staging.orders',
      target: 'table:analytics.tgt_orders',
    });

    const flowNodes = buildFlowNodes(
      statement,
      null,
      '',
      new Set<string>(),
      new Set<string>()
    );
    const outputNode = flowNodes.find((node) => node.id === GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID);
    expect(outputNode).toBeUndefined();
  });

  it('keeps SELECT-style output edges even when other statements introduce table-level edges', () => {
    const statement: StatementLineage = {
      statementIndex: 0,
      statementType: 'SELECT',
      nodes: [
        { id: 'table:source', type: 'table', label: 'source', qualifiedName: 'source' },
        { id: 'table:target', type: 'table', label: 'target', qualifiedName: 'target' },
        { id: 'column:source.id', type: 'column', label: 'id', qualifiedName: 'source.id' },
        { id: 'column:target.id', type: 'column', label: 'id', qualifiedName: 'target.id' },
        { id: 'column:output.total', type: 'column', label: 'total' },
      ],
      edges: [
        {
          id: 'own:source',
          from: 'table:source',
          to: 'column:source.id',
          type: 'ownership',
        },
        {
          id: 'own:target',
          from: 'table:target',
          to: 'column:target.id',
          type: 'ownership',
        },
        {
          id: 'flow:source_to_target',
          from: 'column:source.id',
          to: 'column:target.id',
          type: 'data_flow',
        },
        {
          id: 'flow:source_to_output',
          from: 'column:source.id',
          to: 'column:output.total',
          type: 'derivation',
        },
      ],
    };

    const edges = buildFlowEdges(statement);
    const dmlEdge = edges.find(
      (edge) => edge.source === 'table:source' && edge.target === 'table:target'
    );
    expect(dmlEdge, 'should keep DML-style edge').toBeDefined();

    const selectEdge = edges.find(
      (edge) => edge.target === GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID && edge.source === 'table:source'
    );
    expect(selectEdge, 'should add SELECT output edge').toBeDefined();

    const nodes = buildFlowNodes(
      statement,
      null,
      '',
      new Set<string>(),
      new Set<string>()
    );
    const outputNode = nodes.find((node) => node.id === GRAPH_CONFIG.VIRTUAL_OUTPUT_NODE_ID);
    expect(outputNode, 'virtual Output node should exist for SELECT projections').toBeDefined();
  });

  it('only marks physical tables as base tables when joins exist', () => {
    const statement: StatementLineage = {
      statementIndex: 0,
      statementType: 'SELECT',
      nodes: [
        { id: 'table:users', type: 'table', label: 'users', qualifiedName: 'users' },
        { id: 'cte:recent_orders', type: 'cte', label: 'recent_orders' },
        { id: 'view:active_users', type: 'view', label: 'active_users' },
        {
          id: 'table:orders',
          type: 'table',
          label: 'orders',
          qualifiedName: 'orders',
          joinType: 'INNER',
        },
      ],
      edges: [],
      joinCount: 1,
      complexityScore: 1,
    };

    const nodes = buildFlowNodes(statement, null, '', new Set<string>(), new Set<string>());
    const usersNode = nodes.find((node) => node.id === 'table:users');
    const recentOrdersNode = nodes.find((node) => node.id === 'cte:recent_orders');
    const viewNode = nodes.find((node) => node.id === 'view:active_users');

    expect(usersNode?.data.isBaseTable).toBe(true);
    expect(recentOrdersNode?.data.isBaseTable).toBeFalsy();
    expect(viewNode?.data.isBaseTable).toBeFalsy();
  });
});
