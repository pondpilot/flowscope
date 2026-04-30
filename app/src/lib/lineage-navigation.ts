import type { NavigationTarget } from './navigation-context';

/**
 * Pure consumer of a {@link NavigationTarget} for the lineage tab.
 *
 * The `@pondpilot/flowscope-react` public API does not expose an action that
 * highlights an arbitrary set of node IDs. When a Librarian chat answer
 * resolves to multiple references, we fall back to selecting the first node
 * and recentering on it; the rest of the references are still represented in
 * the schema (parent tables get auto-expanded so the columns are visible).
 */
export interface LineageNavigationDeps {
  expandedTableIds: Set<string>;
  selectNode: (id: string | null) => void;
  toggleTableExpansion: (id: string) => void;
  setFocusNodeId: (id: string | undefined) => void;
  triggerFitView: () => void;
  /**
   * Reveal a node in the graph with a gentle pan/zoom and a transient pulse
   * animation (added in flowscope-react v0.7.0). Used for chat-click
   * navigation instead of `setFocusNodeId`, which triggers the much more
   * aggressive `useNodeFocus` zoom.
   */
  revealNodeInGraph: (nodeId: string) => void;
}

export function applyLineageNavigation(
  target: NavigationTarget,
  deps: LineageNavigationDeps
): void {
  if (target.highlightNodeIds && target.highlightNodeIds.length > 0) {
    const tablesToExpand = target.tablesToExpand ?? [];
    const seen = new Set<string>();
    for (const tableId of tablesToExpand) {
      if (seen.has(tableId)) continue;
      seen.add(tableId);
      if (!deps.expandedTableIds.has(tableId)) {
        deps.toggleTableExpansion(tableId);
      }
    }
    const firstId = target.highlightNodeIds[0];
    deps.selectNode(firstId);
    // Reveal the primary node (parent table for column refs, or the first
    // referenced top-level node otherwise). Columns are not top-level
    // ReactFlow nodes, so reveal would be a no-op for column ids — the
    // resolver hands us a parent-table id via primaryFocusId for that case.
    deps.revealNodeInGraph(target.primaryFocusId ?? firstId);
    return;
  }

  if (target.tableId) {
    deps.selectNode(target.tableId);
    deps.setFocusNodeId(target.tableId);
    return;
  }

  if (target.fitView) {
    deps.triggerFitView();
  }
}
