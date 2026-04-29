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
    deps.setFocusNodeId(firstId);
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
