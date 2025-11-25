import { describe, it, expect } from 'vitest';
import type { StatementLineage } from '@pondpilot/flowscope-core';
import { buildFlowEdges, buildFlowNodes } from '../src/utils/graphBuilders';
import { GRAPH_CONFIG } from '../src/constants';

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
});
