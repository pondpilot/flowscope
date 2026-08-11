import { describe, expect, it } from 'vitest';
import type { Edge as FlowEdge, Node as FlowNode } from '@xyflow/react';

import { GRAPH_CONFIG } from '../src/constants';
import type { TableNodeData } from '../src/types';
import {
  applyRenderDataToEdges,
  applyRenderDataToNodes,
  createRenderGraphIndex,
  enhanceGraphWithHighlights,
  getGraphSearchableTypes,
  getNodeCollapseStates,
  hasNodeCollapseChanged,
  prepareProgressiveLayoutNodes,
  shouldShowMiniMap,
} from '../src/utils/graphViewModel';

function makeNode(id: string, data: Record<string, unknown> = {}): FlowNode {
  return { id, position: { x: 0, y: 0 }, data };
}

function makeTableNode(id: string, isCollapsed = false): FlowNode {
  return {
    id,
    type: 'tableNode',
    position: { x: 0, y: 0 },
    data: {
      label: id,
      nodeType: 'table',
      columns: [
        { id: `${id}:highlighted`, name: 'highlighted' },
        { id: `${id}:plain`, name: 'plain' },
      ],
      isSelected: false,
      isCollapsed,
      isHighlighted: false,
    } satisfies TableNodeData,
  };
}

describe('enhanceGraphWithHighlights', () => {
  it('projects highlighted graph ids into table columns, nodes, and edges', () => {
    const tableNode = makeTableNode('table:orders');
    const scriptNode = makeNode('script:load', { label: 'load', isSelected: true });
    const edge: FlowEdge = {
      id: 'edge:orders-load',
      source: tableNode.id,
      target: scriptNode.id,
      data: { relation: 'read' },
    };

    const result = enhanceGraphWithHighlights(
      { nodes: [tableNode, scriptNode], edges: [edge] },
      new Set([tableNode.id, 'table:orders:highlighted', edge.id])
    );

    const tableData = result.nodes[0].data as TableNodeData;
    expect(tableData.isSelected).toBe(true);
    expect(tableData.columns.map((column) => column.isHighlighted)).toEqual([true, false]);
    expect(result.nodes[1].data.isSelected).toBe(true);
    expect(result.edges[0]).toMatchObject({
      animated: true,
      zIndex: GRAPH_CONFIG.HIGHLIGHTED_EDGE_Z_INDEX,
      data: { relation: 'read', isHighlighted: true },
    });
  });

  it('clears transient edge highlights without clearing existing node selection', () => {
    const selectedNode = makeNode('script:selected', { isSelected: true });
    const edge: FlowEdge = { id: 'edge', source: 'a', target: 'b', animated: true, zIndex: 10 };

    const result = enhanceGraphWithHighlights({ nodes: [selectedNode], edges: [edge] }, new Set());

    expect(result.nodes[0].data.isSelected).toBe(true);
    expect(result.edges[0]).toMatchObject({
      animated: false,
      zIndex: 0,
      data: { isHighlighted: false },
    });
  });
});

describe('prepareProgressiveLayoutNodes', () => {
  it('preserves every position when the graph ids are unchanged', () => {
    const current = [
      { ...makeNode('a'), position: { x: 100, y: 200 } },
      { ...makeNode('b'), position: { x: 300, y: 400 } },
    ];
    const render = [makeNode('a', { version: 2 }), makeNode('b', { version: 2 })];

    const result = prepareProgressiveLayoutNodes(current, render, 'LR');

    expect(result.map((node) => node.position)).toEqual(current.map((node) => node.position));
    expect(result.map((node) => node.data)).toEqual(render.map((node) => node.data));
  });

  it('uses a fresh fast layout when fewer than half of the node ids overlap', () => {
    const current = [{ ...makeNode('a'), position: { x: 999, y: 999 } }];
    const render = [makeNode('a'), makeNode('b'), makeNode('c')];

    const result = prepareProgressiveLayoutNodes(current, render, 'LR');

    expect(result[0].position).not.toEqual(current[0].position);
  });

  it('preserves matched positions and lays out new nodes at the overlap threshold', () => {
    const current = [{ ...makeNode('a'), position: { x: 999, y: 999 } }];
    const render = [makeNode('a'), makeNode('b')];

    const result = prepareProgressiveLayoutNodes(current, render, 'LR');

    expect(result[0].position).toEqual(current[0].position);
    expect(result[1].position).not.toEqual(current[0].position);
  });
});

describe('render graph reconciliation', () => {
  it('applies current render data while preserving user-adjusted positions', () => {
    const layoutNode = makeNode('a', { version: 'layout' });
    const renderNode = makeNode('a', { version: 'render' });
    const currentNode = { ...makeNode('a'), position: { x: 25, y: 50 } };
    const layoutEdge: FlowEdge = { id: 'edge', source: 'a', target: 'b', animated: false };
    const renderEdge: FlowEdge = {
      ...layoutEdge,
      animated: true,
      zIndex: 100,
      data: { isHighlighted: true },
    };
    const index = createRenderGraphIndex({ nodes: [renderNode], edges: [renderEdge] });

    expect(applyRenderDataToNodes([layoutNode], index.nodeDataById, [currentNode])).toEqual([
      { ...layoutNode, position: currentNode.position, data: renderNode.data },
    ]);
    expect(applyRenderDataToEdges([layoutEdge], index.edgeById)).toEqual([renderEdge]);
  });

  it('tracks table collapse state changes without including other node types', () => {
    const collapsed = makeTableNode('table:orders', true);
    const script = makeNode('script:load', { isCollapsed: true });

    const collapseStates = getNodeCollapseStates([collapsed, script]);

    expect(collapseStates).toEqual(new Map([[collapsed.id, true]]));
    expect(hasNodeCollapseChanged([collapsed], new Map([[collapsed.id, false]]))).toBe(true);
    expect(hasNodeCollapseChanged([collapsed], collapseStates)).toBe(false);
  });
});

describe('graph control derivation', () => {
  it('derives searchable types from view and column settings', () => {
    expect(getGraphSearchableTypes('script', true)).toEqual(['script', 'table', 'view', 'cte']);
    expect(getGraphSearchableTypes('table', false)).toEqual(['table', 'view', 'cte', 'script']);
    expect(getGraphSearchableTypes('table', true)).toContain('column');
  });

  it('shows the minimap only for non-empty graphs within the supported size', () => {
    expect(shouldShowMiniMap(0)).toBe(false);
    expect(shouldShowMiniMap(1)).toBe(true);
    expect(shouldShowMiniMap(2000)).toBe(true);
    expect(shouldShowMiniMap(2001)).toBe(false);
  });
});
