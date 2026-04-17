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

  it('derives table-level parents via column ownership when flow edges are column-level', () => {
    // Mimics the real analyzer shape: a/b tables own columns, data_flow
    // runs column→column. The util must still surface a → b as a
    // relational edge via ownership mapping.
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'b', type: 'cte' },
      { id: 'a.col', type: 'column' },
      { id: 'b.col', type: 'column' },
    ];
    const edges: MinimalEdge[] = [
      { from: 'a', to: 'a.col', type: 'ownership' },
      { from: 'b', to: 'b.col', type: 'ownership' },
      { from: 'a.col', to: 'b.col', type: 'data_flow' },
    ];
    const result = computeChainCollapse(view(nodes, edges), new Set());
    expect(result.collapsibleTargetIds.has('b')).toBe(true);
  });

  it('deduplicates multiple column-level edges between the same two tables', () => {
    // Three column-level data_flow edges a→b — a's inDegree from b's
    // perspective should still be 1, not 3, so b remains eligible.
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'b', type: 'cte' },
      { id: 'a.x', type: 'column' },
      { id: 'a.y', type: 'column' },
      { id: 'b.x', type: 'column' },
      { id: 'b.y', type: 'column' },
    ];
    const edges: MinimalEdge[] = [
      { from: 'a', to: 'a.x', type: 'ownership' },
      { from: 'a', to: 'a.y', type: 'ownership' },
      { from: 'b', to: 'b.x', type: 'ownership' },
      { from: 'b', to: 'b.y', type: 'ownership' },
      { from: 'a.x', to: 'b.x', type: 'data_flow' },
      { from: 'a.y', to: 'b.y', type: 'data_flow' },
      { from: 'a.x', to: 'b.y', type: 'derivation' },
    ];
    const result = computeChainCollapse(view(nodes, edges), new Set());
    expect(result.collapsibleTargetIds.has('b')).toBe(true);
  });

  it('correctly counts rollup across a chain resolved through column ownership', () => {
    // a → b → c → d all connected via column-level data_flow. Folding b
    // must orphan c and d for a total rollup of 3 under b.
    const nodes: MinimalNode[] = [
      { id: 'a', type: 'table' },
      { id: 'b', type: 'cte' },
      { id: 'c', type: 'cte' },
      { id: 'd', type: 'cte' },
      { id: 'a.c', type: 'column' },
      { id: 'b.c', type: 'column' },
      { id: 'c.c', type: 'column' },
      { id: 'd.c', type: 'column' },
    ];
    const edges: MinimalEdge[] = [
      { from: 'a', to: 'a.c', type: 'ownership' },
      { from: 'b', to: 'b.c', type: 'ownership' },
      { from: 'c', to: 'c.c', type: 'ownership' },
      { from: 'd', to: 'd.c', type: 'ownership' },
      { from: 'a.c', to: 'b.c', type: 'data_flow' },
      { from: 'b.c', to: 'c.c', type: 'data_flow' },
      { from: 'c.c', to: 'd.c', type: 'data_flow' },
    ];
    const result = computeChainCollapse(view(nodes, edges), new Set(['b']));
    expect(result.foldRootIds).toEqual(new Set(['b']));
    expect(result.hiddenNodeIds).toEqual(new Set(['c', 'd']));
    // b (self) + c + d = 3.
    expect(result.rollupCountByFoldRootId.get('b')).toBe(3);
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
