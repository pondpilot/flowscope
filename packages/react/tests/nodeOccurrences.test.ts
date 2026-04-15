import { describe, expect, it } from 'vitest';
import type { Node } from '@pondpilot/flowscope-core';
import { getAggregationForStatement, scopeNodeToStatement } from '../src/utils/nodeOccurrences';

describe('statement-scoped aggregations', () => {
  it('clears aggregation metadata for statements explicitly marked as non-aggregated', () => {
    const node: Node = {
      id: 'column:analytics.metrics.c',
      type: 'column',
      label: 'c',
      qualifiedName: 'analytics.metrics.c',
      statementIds: [0, 1],
      aggregation: {
        isGroupingKey: false,
        function: 'COUNT',
      },
      metadata: {
        statementAggregations: {
          '0': {
            isGroupingKey: false,
            function: 'COUNT',
          },
          '1': null,
        },
      },
    };

    expect(getAggregationForStatement(node, 0)?.function).toBe('COUNT');
    expect(getAggregationForStatement(node, 1)).toBeUndefined();
    expect(scopeNodeToStatement(node, 1).aggregation).toBeUndefined();
  });
});
