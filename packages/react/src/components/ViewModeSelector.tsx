import { useLineage } from '../context';
import type { LineageViewMode } from '../types';

const VIEW_MODES: Array<{ value: LineageViewMode; label: string; description: string }> = [
  {
    value: 'script',
    label: 'Script',
    description: 'Show relationships between scripts through shared tables',
  },
  {
    value: 'table',
    label: 'Table',
    description: 'Show tables with relationships (default view)',
  },
  {
    value: 'column',
    label: 'Column',
    description: 'Show individual columns with full lineage paths',
  },
];

/**
 * Segmented control for switching between different lineage view modes.
 * Displays three options: Script, Table, and Column views.
 */
export function ViewModeSelector(): JSX.Element {
  const { state, actions } = useLineage();
  const { viewMode } = state;
  const { setViewMode } = actions;

  return (
    <div className="inline-flex rounded-md border border-border" role="group">
      {VIEW_MODES.map((mode, index) => {
        const isActive = viewMode === mode.value;
        const isFirst = index === 0;
        const isLast = index === VIEW_MODES.length - 1;

        return (
          <button
            key={mode.value}
            type="button"
            onClick={() => setViewMode(mode.value)}
            title={mode.description}
            className={`
              px-3 py-1 text-xs font-medium
              ${
                isActive
                  ? 'bg-primary text-primary-foreground'
                  : 'bg-background text-foreground hover:bg-muted'
              }
              ${isFirst ? 'rounded-l-sm' : ''}
              ${isLast ? 'rounded-r-sm' : ''}
              ${!isFirst ? 'border-l border-border' : ''}
              focus:z-10 focus:outline-none focus:ring-2 focus:ring-ring
              transition-colors duration-150
            `}
          >
            {mode.label}
          </button>
        );
      })}
    </div>
  );
}
