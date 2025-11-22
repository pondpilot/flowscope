import { describe, it, expect } from 'vitest';
import { findConnectedElements } from '../src/utils/graphTraversal';
import type { Edge as FlowEdge } from '@xyflow/react';

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
