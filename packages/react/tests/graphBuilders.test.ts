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
  joinCount: 0,
  complexityScore: 1,
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

/**
 * Creates a statement lineage resembling the customer_360 view:
 * - CTEs: user_ltv (from orders), user_engagement (from session_summary)
 * - Final SELECT joins users with both CTEs
 * - Represents: CREATE VIEW customer_360 AS WITH ... SELECT ... FROM users LEFT JOIN user_ltv LEFT JOIN user_engagement
 */
const createCustomer360Lineage = (): StatementLineage => ({
  statementIndex: 0,
  statementType: 'CREATE_VIEW',
  joinCount: 2,
  complexityScore: 50,
  nodes: [
    // Tables
    { id: 'table:orders', type: 'table', label: 'orders', qualifiedName: 'orders' },
    { id: 'table:session_summary', type: 'table', label: 'session_summary', qualifiedName: 'session_summary' },
    { id: 'table:users', type: 'table', label: 'users', qualifiedName: 'users' },
    // CTEs
    { id: 'cte:user_ltv', type: 'cte', label: 'user_ltv', joinType: 'LEFT', joinCondition: 'u.user_id = ltv.user_id' },
    { id: 'cte:user_engagement', type: 'cte', label: 'user_engagement', joinType: 'LEFT', joinCondition: 'u.user_id = eng.user_id' },
    // View
    { id: 'view:customer_360', type: 'view', label: 'customer_360', qualifiedName: 'customer_360' },
    // Columns from orders
    { id: 'column:orders.user_id', type: 'column', label: 'user_id', qualifiedName: 'orders.user_id' },
    { id: 'column:orders.total_amount', type: 'column', label: 'total_amount', qualifiedName: 'orders.total_amount' },
    // Columns from session_summary
    { id: 'column:session_summary.user_id', type: 'column', label: 'user_id', qualifiedName: 'session_summary.user_id' },
    { id: 'column:session_summary.session_id', type: 'column', label: 'session_id', qualifiedName: 'session_summary.session_id' },
    // Columns from users
    { id: 'column:users.user_id', type: 'column', label: 'user_id', qualifiedName: 'users.user_id' },
    { id: 'column:users.email', type: 'column', label: 'email', qualifiedName: 'users.email' },
    // Columns from user_ltv CTE
    { id: 'column:user_ltv.user_id', type: 'column', label: 'user_id', qualifiedName: 'user_ltv.user_id' },
    { id: 'column:user_ltv.lifetime_value', type: 'column', label: 'lifetime_value', qualifiedName: 'user_ltv.lifetime_value' },
    // Columns from user_engagement CTE
    { id: 'column:user_engagement.user_id', type: 'column', label: 'user_id', qualifiedName: 'user_engagement.user_id' },
    { id: 'column:user_engagement.total_sessions', type: 'column', label: 'total_sessions', qualifiedName: 'user_engagement.total_sessions' },
    // Columns from customer_360 view (output)
    { id: 'column:customer_360.user_id', type: 'column', label: 'user_id', qualifiedName: 'customer_360.user_id' },
    { id: 'column:customer_360.email', type: 'column', label: 'email', qualifiedName: 'customer_360.email' },
    { id: 'column:customer_360.lifetime_value', type: 'column', label: 'lifetime_value', qualifiedName: 'customer_360.lifetime_value' },
    { id: 'column:customer_360.total_sessions', type: 'column', label: 'total_sessions', qualifiedName: 'customer_360.total_sessions' },
  ],
  edges: [
    // Ownership edges: table -> column
    { id: 'own:orders.user_id', from: 'table:orders', to: 'column:orders.user_id', type: 'ownership' },
    { id: 'own:orders.total_amount', from: 'table:orders', to: 'column:orders.total_amount', type: 'ownership' },
    { id: 'own:session_summary.user_id', from: 'table:session_summary', to: 'column:session_summary.user_id', type: 'ownership' },
    { id: 'own:session_summary.session_id', from: 'table:session_summary', to: 'column:session_summary.session_id', type: 'ownership' },
    { id: 'own:users.user_id', from: 'table:users', to: 'column:users.user_id', type: 'ownership' },
    { id: 'own:users.email', from: 'table:users', to: 'column:users.email', type: 'ownership' },
    { id: 'own:user_ltv.user_id', from: 'cte:user_ltv', to: 'column:user_ltv.user_id', type: 'ownership' },
    { id: 'own:user_ltv.lifetime_value', from: 'cte:user_ltv', to: 'column:user_ltv.lifetime_value', type: 'ownership' },
    { id: 'own:user_engagement.user_id', from: 'cte:user_engagement', to: 'column:user_engagement.user_id', type: 'ownership' },
    { id: 'own:user_engagement.total_sessions', from: 'cte:user_engagement', to: 'column:user_engagement.total_sessions', type: 'ownership' },
    { id: 'own:customer_360.user_id', from: 'view:customer_360', to: 'column:customer_360.user_id', type: 'ownership' },
    { id: 'own:customer_360.email', from: 'view:customer_360', to: 'column:customer_360.email', type: 'ownership' },
    { id: 'own:customer_360.lifetime_value', from: 'view:customer_360', to: 'column:customer_360.lifetime_value', type: 'ownership' },
    { id: 'own:customer_360.total_sessions', from: 'view:customer_360', to: 'column:customer_360.total_sessions', type: 'ownership' },
    // Data flow edges: orders -> user_ltv CTE
    { id: 'flow:orders.user_id->user_ltv.user_id', from: 'column:orders.user_id', to: 'column:user_ltv.user_id', type: 'derivation' },
    { id: 'flow:orders.total_amount->user_ltv.lifetime_value', from: 'column:orders.total_amount', to: 'column:user_ltv.lifetime_value', type: 'derivation' },
    // Data flow edges: session_summary -> user_engagement CTE
    { id: 'flow:session_summary.user_id->user_engagement.user_id', from: 'column:session_summary.user_id', to: 'column:user_engagement.user_id', type: 'derivation' },
    { id: 'flow:session_summary.session_id->user_engagement.total_sessions', from: 'column:session_summary.session_id', to: 'column:user_engagement.total_sessions', type: 'derivation' },
    // Data flow edges: users -> customer_360
    { id: 'flow:users.user_id->customer_360.user_id', from: 'column:users.user_id', to: 'column:customer_360.user_id', type: 'data_flow' },
    { id: 'flow:users.email->customer_360.email', from: 'column:users.email', to: 'column:customer_360.email', type: 'data_flow' },
    // Data flow edges: user_ltv -> customer_360
    { id: 'flow:user_ltv.lifetime_value->customer_360.lifetime_value', from: 'column:user_ltv.lifetime_value', to: 'column:customer_360.lifetime_value', type: 'data_flow' },
    // Data flow edges: user_engagement -> customer_360
    { id: 'flow:user_engagement.total_sessions->customer_360.total_sessions', from: 'column:user_engagement.total_sessions', to: 'column:customer_360.total_sessions', type: 'data_flow' },
  ],
});

describe('buildFlowEdges table consistency', () => {
  it('should produce same table-to-table pairs regardless of showColumnEdges', () => {
    const statement = createCustomer360Lineage();

    // Build edges in both modes
    const tableEdges = buildFlowEdges(statement, false);
    const columnEdges = buildFlowEdges(statement, true);

    // Extract unique table pairs from each (source->target)
    const tableModePairs = new Set(tableEdges.map((e) => `${e.source}->${e.target}`));
    const columnModePairs = new Set(columnEdges.map((e) => `${e.source}->${e.target}`));

    // Verify consistent table pairs
    expect(tableModePairs).toEqual(columnModePairs);

    // Also verify edge counts make sense
    // Table mode: deduplicated (1 edge per table pair)
    // Column mode: one edge per column connection
    expect(tableEdges.length).toBeLessThanOrEqual(columnEdges.length);

    // With 8 column-level data flows, column mode should have more edges
    // Table mode should have exactly 5 unique table pairs
    expect(tableEdges.length).toBe(5);
    expect(columnEdges.length).toBe(8);
  });

  it('should include expected table relationships for customer_360', () => {
    const statement = createCustomer360Lineage();
    const edges = buildFlowEdges(statement, false);

    const tablePairs = edges.map((e) => `${e.source}->${e.target}`);

    // Expected relationships based on SQL structure
    expect(tablePairs).toContain('table:orders->cte:user_ltv');
    expect(tablePairs).toContain('table:session_summary->cte:user_engagement');
    expect(tablePairs).toContain('table:users->view:customer_360');
    expect(tablePairs).toContain('cte:user_ltv->view:customer_360');
    expect(tablePairs).toContain('cte:user_engagement->view:customer_360');
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
      joinCount: 0,
      complexityScore: 1,
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
