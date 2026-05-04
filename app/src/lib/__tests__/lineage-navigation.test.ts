import { describe, expect, it, vi } from 'vitest';

import { applyLineageNavigation, type LineageNavigationDeps } from '../lineage-navigation';
import type { NavigationTarget } from '../navigation-context';

function makeDeps(overrides: Partial<LineageNavigationDeps> = {}): LineageNavigationDeps & {
  selectNode: ReturnType<typeof vi.fn>;
  toggleTableExpansion: ReturnType<typeof vi.fn>;
  setFocusNodeId: ReturnType<typeof vi.fn>;
  triggerFitView: ReturnType<typeof vi.fn>;
  revealNodeInGraph: ReturnType<typeof vi.fn>;
} {
  return {
    expandedTableIds: new Set<string>(),
    selectNode: vi.fn(),
    toggleTableExpansion: vi.fn(),
    setFocusNodeId: vi.fn(),
    triggerFitView: vi.fn(),
    revealNodeInGraph: vi.fn(),
    ...overrides,
  } as LineageNavigationDeps & {
    selectNode: ReturnType<typeof vi.fn>;
    toggleTableExpansion: ReturnType<typeof vi.fn>;
    setFocusNodeId: ReturnType<typeof vi.fn>;
    triggerFitView: ReturnType<typeof vi.fn>;
    revealNodeInGraph: ReturnType<typeof vi.fn>;
  };
}

describe('applyLineageNavigation', () => {
  it('expands missing parent tables, selects the first node, and reveals the primary focus', () => {
    const deps = makeDeps({
      expandedTableIds: new Set<string>(['t:already-open']),
    });
    const target: NavigationTarget = {
      highlightNodeIds: ['c:bkpf.mandt', 'c:bseg.mandt'],
      tablesToExpand: ['t:bkpf', 't:bseg', 't:already-open'],
      primaryFocusId: 't:bkpf',
    };

    applyLineageNavigation(target, deps);

    // Tables not yet expanded get toggled, the already-expanded one is skipped.
    expect(deps.toggleTableExpansion).toHaveBeenCalledWith('t:bkpf');
    expect(deps.toggleTableExpansion).toHaveBeenCalledWith('t:bseg');
    expect(deps.toggleTableExpansion).not.toHaveBeenCalledWith('t:already-open');
    expect(deps.toggleTableExpansion).toHaveBeenCalledTimes(2);

    // First node is selected for highlight styling. revealNodeInGraph drives
    // the gentle pan/zoom + pulse on the parent table; setFocusNodeId stays
    // unused because its useNodeFocus zoom is too aggressive.
    expect(deps.selectNode).toHaveBeenCalledWith('c:bkpf.mandt');
    expect(deps.revealNodeInGraph).toHaveBeenCalledWith('t:bkpf');
    expect(deps.setFocusNodeId).not.toHaveBeenCalled();
    expect(deps.triggerFitView).not.toHaveBeenCalled();
  });

  it('reveals the first highlight id when no primaryFocusId is provided', () => {
    const deps = makeDeps();
    const target: NavigationTarget = {
      highlightNodeIds: ['t:mara'],
    };

    applyLineageNavigation(target, deps);

    expect(deps.selectNode).toHaveBeenCalledWith('t:mara');
    expect(deps.revealNodeInGraph).toHaveBeenCalledWith('t:mara');
    expect(deps.setFocusNodeId).not.toHaveBeenCalled();
  });

  it('deduplicates entries in tablesToExpand', () => {
    const deps = makeDeps();
    const target: NavigationTarget = {
      highlightNodeIds: ['n:1'],
      tablesToExpand: ['t:bkpf', 't:bkpf', 't:bkpf'],
    };

    applyLineageNavigation(target, deps);

    expect(deps.toggleTableExpansion).toHaveBeenCalledTimes(1);
    expect(deps.toggleTableExpansion).toHaveBeenCalledWith('t:bkpf');
  });

  it('handles highlightNodeIds without tablesToExpand', () => {
    const deps = makeDeps();
    const target: NavigationTarget = {
      highlightNodeIds: ['t:mara'],
    };

    applyLineageNavigation(target, deps);

    expect(deps.toggleTableExpansion).not.toHaveBeenCalled();
    expect(deps.selectNode).toHaveBeenCalledWith('t:mara');
    expect(deps.revealNodeInGraph).toHaveBeenCalledWith('t:mara');
    expect(deps.setFocusNodeId).not.toHaveBeenCalled();
  });

  it('falls back to tableId selection when highlightNodeIds is empty', () => {
    const deps = makeDeps();
    const target: NavigationTarget = {
      highlightNodeIds: [],
      tableId: 't:legacy',
    };

    applyLineageNavigation(target, deps);

    expect(deps.selectNode).toHaveBeenCalledWith('t:legacy');
    expect(deps.setFocusNodeId).toHaveBeenCalledWith('t:legacy');
    expect(deps.toggleTableExpansion).not.toHaveBeenCalled();
    expect(deps.triggerFitView).not.toHaveBeenCalled();
  });

  it('handles legacy tableId navigation (HierarchyView path)', () => {
    const deps = makeDeps();
    const target: NavigationTarget = { tableId: 't:mara' };

    applyLineageNavigation(target, deps);

    expect(deps.selectNode).toHaveBeenCalledWith('t:mara');
    expect(deps.setFocusNodeId).toHaveBeenCalledWith('t:mara');
    expect(deps.toggleTableExpansion).not.toHaveBeenCalled();
    expect(deps.triggerFitView).not.toHaveBeenCalled();
  });

  it('triggers fit-to-view for fitView-only navigation (Issues panel path)', () => {
    const deps = makeDeps();
    const target: NavigationTarget = { fitView: true };

    applyLineageNavigation(target, deps);

    expect(deps.triggerFitView).toHaveBeenCalledTimes(1);
    expect(deps.selectNode).not.toHaveBeenCalled();
    expect(deps.setFocusNodeId).not.toHaveBeenCalled();
    expect(deps.toggleTableExpansion).not.toHaveBeenCalled();
  });

  it('does nothing for an empty navigation target', () => {
    const deps = makeDeps();

    applyLineageNavigation({}, deps);

    expect(deps.selectNode).not.toHaveBeenCalled();
    expect(deps.setFocusNodeId).not.toHaveBeenCalled();
    expect(deps.toggleTableExpansion).not.toHaveBeenCalled();
    expect(deps.triggerFitView).not.toHaveBeenCalled();
  });

  it('prefers highlightNodeIds over tableId when both are present', () => {
    const deps = makeDeps();
    const target: NavigationTarget = {
      highlightNodeIds: ['n:highlight'],
      tableId: 't:other',
    };

    applyLineageNavigation(target, deps);

    expect(deps.selectNode).toHaveBeenCalledWith('n:highlight');
    expect(deps.selectNode).not.toHaveBeenCalledWith('t:other');
    expect(deps.revealNodeInGraph).toHaveBeenCalledWith('n:highlight');
    expect(deps.setFocusNodeId).not.toHaveBeenCalled();
  });

  it('does not touch schema-related state for librarian-originated navigation', () => {
    // Librarian never produces tableName-only targets; this guard ensures the
    // helper itself does not synthesize one. Any tableName field is ignored
    // here — the schema useEffect (HierarchyView path) is the only consumer.
    const deps = makeDeps();
    const target: NavigationTarget = {
      highlightNodeIds: ['c:bkpf.mandt'],
      tablesToExpand: ['t:bkpf'],
      tableName: 'BKPF',
    };

    applyLineageNavigation(target, deps);

    expect(deps.selectNode).toHaveBeenCalledWith('c:bkpf.mandt');
    expect(deps.toggleTableExpansion).toHaveBeenCalledWith('t:bkpf');
    // No fit-view churn, no extra side effects.
    expect(deps.triggerFitView).not.toHaveBeenCalled();
  });
});
