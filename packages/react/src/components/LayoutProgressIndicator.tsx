import { Loader2 } from 'lucide-react';
import type { JSX } from 'react';
import { useLineageStore } from '../store';
import { PANEL_STYLES } from '../constants';

/**
 * Loading indicator shown during graph building and layout computation.
 * Appears in the bottom-left corner of the graph while:
 * - The graph builder worker constructs nodes and edges from lineage data
 * - The layout worker computes node positions
 */
export function LayoutProgressIndicator(): JSX.Element | null {
  const isLayouting = useLineageStore((state) => state.isLayouting);
  const isBuilding = useLineageStore((state) => state.isBuilding);

  const isLoading = isBuilding || isLayouting;

  if (!isLoading) return null;

  // Show appropriate message based on what's happening
  const message = isBuilding ? 'Building graph...' : 'Computing layout...';

  return (
    <div className={PANEL_STYLES.container} data-graph-panel>
      <Loader2 className="size-4 animate-spin text-slate-500 dark:text-slate-400 ml-2" />
      <span className="text-sm text-slate-600 dark:text-slate-300 px-2">{message}</span>
    </div>
  );
}
