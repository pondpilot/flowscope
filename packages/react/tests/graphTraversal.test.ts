import { describe, it, expect } from 'vitest';
import {
  findConnectedElements,
  pruneDanglingEdges,
  filterByNamespace,
} from '../src/utils/graphTraversal';
import type { Edge as FlowEdge, Node as FlowNode } from '@xyflow/react';
import type { TableNodeData, NamespaceFilter } from '../src/types';

const makeTableNode = (
  id: string,
  columns: Array<{ id: string; name: string }> = []
): FlowNode => ({
  id,
  type: 'tableNode',
  position: { x: 0, y: 0 },
  data: {
    label: id,
    nodeType: 'table',
    columns,
    isSelected: false,
    isCollapsed: false,
    isHighlighted: false,
  } satisfies TableNodeData,
});

describe('findConnectedElements', () => {
  describe('linear graph traversal', () => {
    it('should find all connected elements in a simple linear graph', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node1',
          target: 'node2',
        },
        {
          id: 'edge2',
          source: 'node2',
          target: 'node3',
        },
      ];

      const result = findConnectedElements('node1', edges);

      expect(result).toContain('node1');
      expect(result).toContain('edge1');
      expect(result).toContain('node2');
      expect(result).toContain('edge2');
      expect(result).toContain('node3');
      expect(result.size).toBe(5);
    });

    it('should find connected elements starting from middle node', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node1',
          target: 'node2',
        },
        {
          id: 'edge2',
          source: 'node2',
          target: 'node3',
        },
      ];

      const result = findConnectedElements('node2', edges);

      // Should find both upstream (node1) and downstream (node3)
      expect(result).toContain('node1');
      expect(result).toContain('edge1');
      expect(result).toContain('node2');
      expect(result).toContain('edge2');
      expect(result).toContain('node3');
      expect(result.size).toBe(5);
    });
  });

  describe('branching graph traversal', () => {
    it('should find all connected elements in a branching graph', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node1',
          target: 'node2',
        },
        {
          id: 'edge2',
          source: 'node1',
          target: 'node3',
        },
        {
          id: 'edge3',
          source: 'node2',
          target: 'node4',
        },
        {
          id: 'edge4',
          source: 'node3',
          target: 'node4',
        },
      ];

      const result = findConnectedElements('node1', edges);

      // Should find all nodes and edges in the graph
      expect(result).toContain('node1');
      expect(result).toContain('edge1');
      expect(result).toContain('node2');
      expect(result).toContain('edge2');
      expect(result).toContain('node3');
      expect(result).toContain('edge3');
      expect(result).toContain('node4');
      expect(result).toContain('edge4');
      expect(result.size).toBe(8);
    });

    it('should find all upstream and downstream elements from merge point', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node1',
          target: 'node3',
        },
        {
          id: 'edge2',
          source: 'node2',
          target: 'node3',
        },
        {
          id: 'edge3',
          source: 'node3',
          target: 'node4',
        },
      ];

      const result = findConnectedElements('node3', edges);

      // Should find all connected nodes
      expect(result).toContain('node1');
      expect(result).toContain('node2');
      expect(result).toContain('node3');
      expect(result).toContain('node4');
      expect(result).toContain('edge1');
      expect(result).toContain('edge2');
      expect(result).toContain('edge3');
      expect(result.size).toBe(7);
    });
  });

  describe('column-level graph with handles', () => {
    it('should handle edges with sourceHandle and targetHandle', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'table1',
          target: 'table2',
          sourceHandle: 'col1',
          targetHandle: 'col2',
        },
        {
          id: 'edge2',
          source: 'table2',
          target: 'table3',
          sourceHandle: 'col2',
          targetHandle: 'col3',
        },
      ];

      const result = findConnectedElements('col1', edges);

      // Should use handles instead of source/target
      expect(result).toContain('col1');
      expect(result).toContain('edge1');
      expect(result).toContain('col2');
      expect(result).toContain('edge2');
      expect(result).toContain('col3');
      expect(result.size).toBe(5);
    });

    it('should handle mixed edges with and without handles', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'table1',
          target: 'table2',
          sourceHandle: 'col1',
          targetHandle: 'col2',
        },
        {
          id: 'edge2',
          source: 'table2',
          target: 'table3',
        },
      ];

      const result = findConnectedElements('col1', edges);

      // Should find connected elements using handles where available
      expect(result).toContain('col1');
      expect(result).toContain('edge1');
      expect(result).toContain('col2');
      expect(result.size).toBe(3);
    });
  });

  describe('disconnected graphs', () => {
    it('should only find elements in connected component', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node1',
          target: 'node2',
        },
        {
          id: 'edge2',
          source: 'node3',
          target: 'node4',
        },
      ];

      const result = findConnectedElements('node1', edges);

      // Should only find node1, node2, and edge1
      expect(result).toContain('node1');
      expect(result).toContain('edge1');
      expect(result).toContain('node2');
      expect(result).not.toContain('node3');
      expect(result).not.toContain('node4');
      expect(result).not.toContain('edge2');
      expect(result.size).toBe(3);
    });
  });

  describe('edge cases', () => {
    it('should handle empty edge list', () => {
      const edges: FlowEdge[] = [];
      const result = findConnectedElements('node1', edges);

      expect(result).toContain('node1');
      expect(result.size).toBe(1);
    });

    it('should handle single node with no connections', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node2',
          target: 'node3',
        },
      ];

      const result = findConnectedElements('node1', edges);

      expect(result).toContain('node1');
      expect(result.size).toBe(1);
    });

    it('should handle cyclic graphs', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'node1',
          target: 'node2',
        },
        {
          id: 'edge2',
          source: 'node2',
          target: 'node3',
        },
        {
          id: 'edge3',
          source: 'node3',
          target: 'node1',
        },
      ];

      const result = findConnectedElements('node1', edges);

      // Should handle cycles without infinite loop
      expect(result).toContain('node1');
      expect(result).toContain('edge1');
      expect(result).toContain('node2');
      expect(result).toContain('edge2');
      expect(result).toContain('node3');
      expect(result).toContain('edge3');
      expect(result.size).toBe(6);
    });
  });

  describe('complex data lineage scenarios', () => {
    it('should handle diamond-shaped dependency graph', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'source',
          target: 'transform1',
        },
        {
          id: 'edge2',
          source: 'source',
          target: 'transform2',
        },
        {
          id: 'edge3',
          source: 'transform1',
          target: 'output',
        },
        {
          id: 'edge4',
          source: 'transform2',
          target: 'output',
        },
      ];

      const result = findConnectedElements('source', edges);

      // Should find: source, edge1, transform1, edge2, transform2, edge3, output, edge4
      expect(result.size).toBe(8);
      expect(result).toContain('source');
      expect(result).toContain('transform1');
      expect(result).toContain('transform2');
      expect(result).toContain('output');
      expect(result).toContain('edge1');
      expect(result).toContain('edge2');
      expect(result).toContain('edge3');
      expect(result).toContain('edge4');
    });

    it('should handle multi-level column lineage', () => {
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'table1',
          target: 'table2',
          sourceHandle: 'table1.col_a',
          targetHandle: 'table2.col_x',
        },
        {
          id: 'edge2',
          source: 'table1',
          target: 'table2',
          sourceHandle: 'table1.col_b',
          targetHandle: 'table2.col_y',
        },
        {
          id: 'edge3',
          source: 'table2',
          target: 'table3',
          sourceHandle: 'table2.col_x',
          targetHandle: 'table3.col_final',
        },
      ];

      const result = findConnectedElements('table1.col_a', edges);

      // Should trace through column lineage
      expect(result).toContain('table1.col_a');
      expect(result).toContain('edge1');
      expect(result).toContain('table2.col_x');
      expect(result).toContain('edge3');
      expect(result).toContain('table3.col_final');
      expect(result.size).toBe(5);

      // Should NOT include the separate lineage path
      expect(result).not.toContain('table1.col_b');
      expect(result).not.toContain('table2.col_y');
      expect(result).not.toContain('edge2');
    });
  });
});

describe('pruneDanglingEdges', () => {
  it('drops edges that reference missing nodes', () => {
    const nodes: FlowNode[] = [makeTableNode('table1')];
    const edges: FlowEdge[] = [
      {
        id: 'edge1',
        source: 'table1',
        target: 'table2',
      },
    ];

    const result = pruneDanglingEdges({ nodes, edges });

    expect(result.edges).toHaveLength(0);
  });

  it('drops edges that reference missing handles', () => {
    const nodes: FlowNode[] = [
      makeTableNode('table1', [{ id: 'col1', name: 'col1' }]),
      makeTableNode('table2', [{ id: 'col2', name: 'col2' }]),
    ];
    const edges: FlowEdge[] = [
      {
        id: 'edge1',
        source: 'table1',
        target: 'table2',
        sourceHandle: 'col1',
        targetHandle: 'col2',
      },
      {
        id: 'edge2',
        source: 'table1',
        target: 'table2',
        sourceHandle: 'col1',
        targetHandle: 'missing',
      },
      {
        id: 'edge3',
        source: 'table1',
        target: 'table2',
        sourceHandle: 'missing',
        targetHandle: 'col2',
      },
    ];

    const result = pruneDanglingEdges({ nodes, edges });

    expect(result.edges).toHaveLength(1);
    expect(result.edges[0].id).toBe('edge1');
  });

  it('keeps edges without handles when nodes exist', () => {
    const nodes: FlowNode[] = [makeTableNode('table1'), makeTableNode('table2')];
    const edges: FlowEdge[] = [
      {
        id: 'edge1',
        source: 'table1',
        target: 'table2',
      },
    ];

    const result = pruneDanglingEdges({ nodes, edges });

    expect(result.edges).toEqual(edges);
  });
});

const makeTableNodeWithNamespace = (
  id: string,
  options: {
    schema?: string;
    database?: string;
    columns?: Array<{ id: string; name: string }>;
  } = {}
): FlowNode => ({
  id,
  type: 'tableNode',
  position: { x: 0, y: 0 },
  data: {
    label: id,
    nodeType: 'table',
    columns: options.columns || [],
    isSelected: false,
    isCollapsed: false,
    isHighlighted: false,
    schema: options.schema,
    database: options.database,
  } satisfies TableNodeData,
});

const makeScriptNode = (id: string): FlowNode => ({
  id,
  type: 'scriptNode',
  position: { x: 0, y: 0 },
  data: {
    label: id,
    sourceName: id,
    tablesRead: [],
    tablesWritten: [],
    statementCount: 1,
    isSelected: false,
    isHighlighted: false,
  },
});

describe('filterByNamespace', () => {
  describe('no filter (show all)', () => {
    it('returns graph unchanged when namespaceFilter is undefined', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: 'private' }),
      ];
      const edges: FlowEdge[] = [{ id: 'edge1', source: 'table1', target: 'table2' }];

      const result = filterByNamespace({ nodes, edges }, undefined);

      expect(result.nodes).toHaveLength(2);
      expect(result.edges).toHaveLength(1);
    });

    it('returns graph unchanged when both filter arrays are empty', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: 'private' }),
      ];
      const edges: FlowEdge[] = [{ id: 'edge1', source: 'table1', target: 'table2' }];
      const filter: NamespaceFilter = { schemas: [], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.edges).toHaveLength(1);
    });
  });

  describe('schema filtering', () => {
    it('filters nodes by schema', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: 'private' }),
        makeTableNodeWithNamespace('table3', { schema: 'public' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: ['public'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.nodes.map((n) => n.id)).toEqual(['table1', 'table3']);
    });

    it('allows multiple schemas', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: 'private' }),
        makeTableNodeWithNamespace('table3', { schema: 'analytics' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: ['public', 'private'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.nodes.map((n) => n.id)).toEqual(['table1', 'table2']);
    });
  });

  describe('database filtering', () => {
    it('filters nodes by database', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { database: 'prod' }),
        makeTableNodeWithNamespace('table2', { database: 'staging' }),
        makeTableNodeWithNamespace('table3', { database: 'prod' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: [], databases: ['prod'] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.nodes.map((n) => n.id)).toEqual(['table1', 'table3']);
    });
  });

  describe('combined schema and database filtering', () => {
    it('filters by both schema AND database', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public', database: 'prod' }),
        makeTableNodeWithNamespace('table2', { schema: 'public', database: 'staging' }),
        makeTableNodeWithNamespace('table3', { schema: 'private', database: 'prod' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: ['public'], databases: ['prod'] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(1);
      expect(result.nodes[0].id).toBe('table1');
    });
  });

  describe('unscoped nodes behavior', () => {
    it('keeps nodes without schema when schema filter is active', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: undefined }), // No schema
        makeTableNodeWithNamespace('table3', { schema: 'private' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: ['public'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.nodes.map((n) => n.id)).toEqual(['table1', 'table2']);
    });

    it('keeps nodes without database when database filter is active', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { database: 'prod' }),
        makeTableNodeWithNamespace('table2', { database: undefined }), // No database
        makeTableNodeWithNamespace('table3', { database: 'staging' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: [], databases: ['prod'] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.nodes.map((n) => n.id)).toEqual(['table1', 'table2']);
    });
  });

  describe('non-table nodes', () => {
    it('always preserves non-table nodes (like script nodes)', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeScriptNode('script1'),
        makeTableNodeWithNamespace('table2', { schema: 'private' }),
      ];
      const edges: FlowEdge[] = [];
      const filter: NamespaceFilter = { schemas: ['public'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.nodes.map((n) => n.id)).toEqual(['table1', 'script1']);
    });
  });

  describe('edge filtering', () => {
    it('removes edges to filtered-out nodes', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: 'private' }),
      ];
      const edges: FlowEdge[] = [{ id: 'edge1', source: 'table1', target: 'table2' }];
      const filter: NamespaceFilter = { schemas: ['public'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(1);
      expect(result.edges).toHaveLength(0);
    });

    it('keeps edges between preserved nodes', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', { schema: 'public' }),
        makeTableNodeWithNamespace('table2', { schema: 'public' }),
      ];
      const edges: FlowEdge[] = [{ id: 'edge1', source: 'table1', target: 'table2' }];
      const filter: NamespaceFilter = { schemas: ['public'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.edges).toHaveLength(1);
    });

    it('handles column-level edges correctly', () => {
      const nodes: FlowNode[] = [
        makeTableNodeWithNamespace('table1', {
          schema: 'public',
          columns: [{ id: 'col1', name: 'col1' }],
        }),
        makeTableNodeWithNamespace('table2', {
          schema: 'public',
          columns: [{ id: 'col2', name: 'col2' }],
        }),
        makeTableNodeWithNamespace('table3', {
          schema: 'private',
          columns: [{ id: 'col3', name: 'col3' }],
        }),
      ];
      const edges: FlowEdge[] = [
        {
          id: 'edge1',
          source: 'table1',
          target: 'table2',
          sourceHandle: 'col1',
          targetHandle: 'col2',
        },
        {
          id: 'edge2',
          source: 'table2',
          target: 'table3',
          sourceHandle: 'col2',
          targetHandle: 'col3',
        },
      ];
      const filter: NamespaceFilter = { schemas: ['public'], databases: [] };

      const result = filterByNamespace({ nodes, edges }, filter);

      expect(result.nodes).toHaveLength(2);
      expect(result.edges).toHaveLength(1);
      expect(result.edges[0].id).toBe('edge1');
    });
  });
});
