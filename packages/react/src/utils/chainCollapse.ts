import type { Edge, Node } from '@pondpilot/flowscope-core';

/**
 * Chain-collapse: fold a single-parent downstream node into its parent.
 *
 * A "chain-collapsible" edge A→B is one whose target B has exactly one
 * incoming edge (from A), so folding B into A is semantically unambiguous.
 * The user toggles folding per node; the graph builder then hides folded
 * nodes, transitively hides descendants that only reach the visible graph
 * through folded ancestors, and annotates the surviving inbound edge with
 * a rollup count.
 *
 * This module is pure — it works off plain lineage arrays so it can be
 * unit-tested without spinning up a worker or a layout engine.
 */

/** Minimal view of the lineage graph this module needs. */
export interface ChainLineageView {
  nodes: ReadonlyArray<Pick<Node, 'id' | 'type'>>;
  edges: ReadonlyArray<Pick<Edge, 'from' | 'to' | 'type'>>;
}

export interface ChainCollapseResult {
  /**
   * Fold-root nodes — the user-selected (and still-eligible) nodes that
   * mark where a chain has been folded. These stay in the graph but are
   * rendered compactly; the edge into them becomes a "+N" badge.
   */
  foldRootIds: ReadonlySet<string>;
  /**
   * Orphaned descendants of fold roots that lose their only connection
   * to the visible graph when the fold happens. These are dropped from
   * the flow graph entirely (both nodes and their edges).
   */
  hiddenNodeIds: ReadonlySet<string>;
  /**
   * For each fold root, the total count of nodes rolled up under it
   * (self + hidden descendants). Used to display the "+N" badge on the
   * incoming edge and optionally on the fold-root node itself.
   */
  rollupCountByFoldRootId: ReadonlyMap<string, number>;
  /**
   * Node ids whose incoming edges should be offered a collapse toggle
   * (i.e. they pass `isChainCollapsible` in the current graph and are
   * not already folded).
   */
  collapsibleTargetIds: ReadonlySet<string>;
}

/**
 * Eligibility predicate for chain-collapse — a node can be folded into
 * its parent when it has exactly one incoming relational edge AND its
 * kind represents a derived step in the pipeline (CTE, view, output).
 *
 * Why not base tables: they represent real external data, so hiding one
 * would hide a source the user almost certainly needs to see. Why not
 * columns: chain collapse operates on the relational graph only.
 *
 * `outDegree` is accepted for symmetry and future tightening (e.g. a
 * stricter "middle-link only" rule), but is not used by this predicate.
 */
export function isChainCollapsible(
  node: Pick<Node, 'id' | 'type'>,
  inDegree: number,
  outDegree: number
): boolean {
  void outDegree;
  if (inDegree !== 1) return false;
  return node.type === 'cte' || node.type === 'view' || node.type === 'output';
}

/**
 * Compute the set of hidden nodes and the rollup counts, given the user's
 * chain-collapse selections and the current (non-column) lineage.
 *
 * The algorithm:
 *  1. Discover the eligible set via `isChainCollapsible` over every node.
 *  2. Starting from each node in `collapsedChainNodeIds` that is eligible,
 *     hide it. Then walk its descendants and hide any whose every path
 *     to a root passes through an already-hidden node — those have no
 *     visible parent anymore and would dangle.
 *  3. For each hidden node, find its surviving visible ancestor (the
 *     first ancestor that is NOT hidden). Bump the rollup count on the
 *     edge `ancestor → firstHiddenChildOnPath`.
 *
 * Column nodes are ignored by this algorithm — pass the relational
 * slice of the graph in.
 */
export function computeChainCollapse(
  view: ChainLineageView,
  collapsedChainNodeIds: ReadonlySet<string>
): ChainCollapseResult {
  const nodes = view.nodes.filter((n) => n.type !== 'column');
  const tableNodeIds = new Set(nodes.map((n) => n.id));

  // The analyzer emits most parent/child flow as column-level edges
  // (data_flow/derivation between column nodes). To derive the
  // table-level graph the UI shows, we first map columns to their owning
  // tables via `ownership` edges, then resolve every flow edge to a
  // table-level pair. This mirrors `buildFlowEdges` so chain eligibility
  // matches what the user sees on screen.
  const columnToTable = new Map<string, string>();
  for (const edge of view.edges) {
    if (edge.type !== 'ownership') continue;
    if (tableNodeIds.has(edge.from)) {
      columnToTable.set(edge.to, edge.from);
    }
  }

  const inDegree = new Map<string, number>();
  const outDegree = new Map<string, number>();
  const parentsOf = new Map<string, string[]>();
  const childrenOf = new Map<string, string[]>();
  for (const node of nodes) {
    inDegree.set(node.id, 0);
    outDegree.set(node.id, 0);
    parentsOf.set(node.id, []);
    childrenOf.set(node.id, []);
  }

  const resolveToTable = (endpointId: string): string | undefined => {
    if (tableNodeIds.has(endpointId)) return endpointId;
    return columnToTable.get(endpointId);
  };

  // Dedupe pairs — multiple column-level edges between the same two
  // tables (a common case when every projected column has its own edge)
  // should count as one parent/child relation, not N.
  const seenPair = new Set<string>();
  const flowEdgeTypes: ReadonlySet<string> = new Set([
    'data_flow',
    'derivation',
    'cross_statement',
  ]);
  for (const edge of view.edges) {
    if (!flowEdgeTypes.has(edge.type)) continue;
    const fromTable = resolveToTable(edge.from);
    const toTable = resolveToTable(edge.to);
    if (!fromTable || !toTable) continue;
    if (fromTable === toTable) continue; // self-loop (recursive CTE)
    const key = `${fromTable}\u0000${toTable}`;
    if (seenPair.has(key)) continue;
    seenPair.add(key);
    inDegree.set(toTable, (inDegree.get(toTable) ?? 0) + 1);
    outDegree.set(fromTable, (outDegree.get(fromTable) ?? 0) + 1);
    parentsOf.get(toTable)!.push(fromTable);
    childrenOf.get(fromTable)!.push(toTable);
  }

  const collapsibleTargetIds = new Set<string>();
  for (const node of nodes) {
    if (isChainCollapsible(node, inDegree.get(node.id) ?? 0, outDegree.get(node.id) ?? 0)) {
      collapsibleTargetIds.add(node.id);
    }
  }

  // Only selections that are still eligible become fold roots. This
  // guards against stale selections surviving across analysis runs where
  // topology changed (e.g. a CTE gained a second parent and is no longer
  // a valid single-parent chain node).
  const foldRootIds = new Set<string>(
    [...collapsedChainNodeIds].filter((id) => collapsibleTargetIds.has(id))
  );

  // Fold roots stay visible in the graph (rendered compactly, with a
  // rollup badge). The "hidden" set holds only their orphaned descendants:
  // nodes whose every path to a root passes through a fold root.
  const hiddenNodeIds = new Set<string>();
  // `absorbed` tracks "already accounted for under some fold" during BFS —
  // union of foldRootIds + hiddenNodeIds.
  const absorbed = new Set<string>(foldRootIds);
  const queue = [...foldRootIds];
  while (queue.length > 0) {
    const id = queue.shift()!;
    for (const childId of childrenOf.get(id) ?? []) {
      if (absorbed.has(childId)) continue;
      const parents = parentsOf.get(childId) ?? [];
      if (parents.length === 0) continue;
      const allParentsAbsorbed = parents.every((p) => absorbed.has(p));
      if (allParentsAbsorbed) {
        absorbed.add(childId);
        hiddenNodeIds.add(childId);
        queue.push(childId);
      }
    }
  }

  // Rollup counts: each fold root counts itself (1) plus the hidden
  // descendants reachable from it through absorbed nodes only. A hidden
  // node may be descended from multiple fold roots (in diamond-shaped
  // graphs); we charge it to the nearest one along a deterministic walk.
  const rollupCountByFoldRootId = new Map<string, number>();
  for (const rootId of foldRootIds) rollupCountByFoldRootId.set(rootId, 1);

  for (const hiddenId of hiddenNodeIds) {
    // Walk up through absorbed parents until we hit a fold root.
    let cursor = hiddenId;
    // Cycle protection for self-loops surviving absorption checks.
    const visited = new Set<string>([cursor]);
    while (true) {
      if (foldRootIds.has(cursor)) {
        rollupCountByFoldRootId.set(cursor, (rollupCountByFoldRootId.get(cursor) ?? 0) + 1);
        break;
      }
      const parents = parentsOf.get(cursor) ?? [];
      const absorbedParent = parents.find((p) => absorbed.has(p) && !visited.has(p));
      if (absorbedParent === undefined) break;
      visited.add(absorbedParent);
      cursor = absorbedParent;
    }
  }

  return { foldRootIds, hiddenNodeIds, rollupCountByFoldRootId, collapsibleTargetIds };
}
