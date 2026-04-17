import { describe, it, expect } from 'vitest';
import type { Edge, Node } from '@pondpilot/flowscope-core';

import {
  computeChainCollapse,
  isChainCollapsible,
  type ChainLineageView,
} from '../src/utils/chainCollapse';

type MinimalNode = Pick<Node, 'id' | 'type'>;
type MinimalEdge = Pick<Edge, 'from' | 'to' | 'type'>;

function view(nodes: MinimalNode[], edges: MinimalEdge[]): ChainLineageView {
  return { nodes, edges };
}

function dataFlow(from: string, to: string): MinimalEdge {
  return { from, to, type: 'data_flow' };
}

describe('isChainCollapsible', () => {
  it('accepts single-parent CTEs, views, and outputs', () => {
    expect(isChainCollapsible({ id: 'a', type: 'cte' }, 1, 1)).toBe(true);
    expect(isChainCollapsible({ id: 'a', type: 'view' }, 1, 0)).toBe(true);
    expect(isChainCollapsible({ id: 'a', type: 'output' }, 1, 3)).toBe(true);
  });

  it('rejects base tables even when single-parent', () => {
    expect(isChainCollapsible({ id: 'a', type: 'table' }, 1, 1)).toBe(false);
  });

  it('rejects nodes with zero or multiple parents', () => {
    expect(isChainCollapsible({ id: 'a', type: 'cte' }, 0, 1)).toBe(false);
    expect(isChainCollapsible({ id: 'a', type: 'cte' }, 2, 1)).toBe(false);
  });

  it('rejects columns unconditionally', () => {
    expect(isChainCollapsible({ id: 'a', type: 'column' }, 1, 1)).toBe(false);
  });
});

describe('computeChainCollapse', () => {
  it('returns empty sets when nothing is selected', () => {
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'cte' },
      { id: 'b', type: 'cte' },
    ];
    const result = computeChainCollapse(view(nodes, [dataFlow('a', 'b')]), new Set());
    expect(result.foldRootIds.size).toBe(0);
    expect(result.hiddenNodeIds.size).toBe(0);
    expect(result.rollupCountByFoldRootId.size).toBe(0);
    expect(result.collapsibleTargetIds.has('b')).toBe(true);
  });

  it('folds a single CTE chain link and rolls up orphaned descendants', () => {
    // a → b → c (b and c both single-parent CTEs).
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'b', type: 'cte' },
      { id: 'c', type: 'cte' },
    ];
    const edges: MinimalEdge[] = [dataFlow('a', 'b'), dataFlow('b', 'c')];
    const result = computeChainCollapse(view(nodes, edges), new Set(['b']));

    expect(result.foldRootIds).toEqual(new Set(['b']));
    expect(result.hiddenNodeIds).toEqual(new Set(['c']));
    // b itself (1) + orphaned c (1) = 2 nodes folded under b.
    expect(result.rollupCountByFoldRootId.get('b')).toBe(2);
  });

  it('keeps descendants visible when they still have a non-folded parent', () => {
    // a → c, b → c, fold a. c still reachable through b → stays visible.
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'cte' },
      { id: 'b', type: 'cte' },
      { id: 'c', type: 'cte' },
      { id: 'root', type: 'table' },
    ];
    const edges: MinimalEdge[] = [
      dataFlow('root', 'a'),
      dataFlow('root', 'b'),
      dataFlow('a', 'c'),
      dataFlow('b', 'c'),
    ];
    const result = computeChainCollapse(view(nodes, edges), new Set(['a']));

    expect(result.foldRootIds).toEqual(new Set(['a']));
    expect(result.hiddenNodeIds.size).toBe(0);
    expect(result.rollupCountByFoldRootId.get('a')).toBe(1);
  });

  it('ignores stale selections that are no longer eligible', () => {
    // b has two parents now — the old selection becomes invalid.
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'a2', type: 'table' },
      { id: 'b', type: 'cte' },
    ];
    const edges: MinimalEdge[] = [dataFlow('a', 'b'), dataFlow('a2', 'b')];
    const result = computeChainCollapse(view(nodes, edges), new Set(['b']));

    expect(result.foldRootIds.size).toBe(0);
    expect(result.hiddenNodeIds.size).toBe(0);
    expect(result.collapsibleTargetIds.has('b')).toBe(false);
  });

  it('expanding a previously folded node restores the original state', () => {
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'b', type: 'cte' },
      { id: 'c', type: 'cte' },
    ];
    const edges: MinimalEdge[] = [dataFlow('a', 'b'), dataFlow('b', 'c')];

    const collapsed = computeChainCollapse(view(nodes, edges), new Set(['b']));
    expect(collapsed.hiddenNodeIds.has('c')).toBe(true);

    const expanded = computeChainCollapse(view(nodes, edges), new Set());
    expect(expanded.foldRootIds.size).toBe(0);
    expect(expanded.hiddenNodeIds.size).toBe(0);
  });

  it('ignores non-relational edges (column ownership, derivation, join deps)', () => {
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'cte' },
      { id: 'b', type: 'cte' },
    ];
    // Only derivation: no data_flow relation between a and b, so b has
    // no chain-collapsible in-degree from the util's perspective.
    const edges: MinimalEdge[] = [{ from: 'a', to: 'b', type: 'derivation' }];
    const result = computeChainCollapse(view(nodes, edges), new Set());
    expect(result.collapsibleTargetIds.has('b')).toBe(false);
  });

  it('ignores self-loops (recursive CTEs) when counting degrees', () => {
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'b', type: 'cte' },
    ];
    const edges: MinimalEdge[] = [dataFlow('a', 'b'), dataFlow('b', 'b')];
    const result = computeChainCollapse(view(nodes, edges), new Set());
    // b has one real parent (a) and a self-loop; predicate should still
    // see inDegree === 1 from the parent edge.
    expect(result.collapsibleTargetIds.has('b')).toBe(true);
  });
});
