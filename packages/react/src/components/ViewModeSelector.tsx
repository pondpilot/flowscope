import { FileCode, Table2, Columns3 } from 'lucide-react';
import { useLineage } from '../store';
import type { LineageViewMode } from '../types';
import { PANEL_STYLES } from '../constants';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';

const VIEW_MODES: Array<{
  value: LineageViewMode;
  label: string;
  description: string;
  icon: React.ElementType;
}> = [
  {
    value: 'script',
    label: 'Script',
    description: 'Show relationships between scripts through shared tables',
    icon: FileCode,
  },
  {
    value: 'table',
    label: 'Table',
    description: 'Show tables with relationships (default view)',
    icon: Table2,
  },
  {
    value: 'column',
    label: 'Column',
    description: 'Show individual columns with full lineage paths',
    icon: Columns3,
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
    <GraphTooltipProvider>
      <div
        className={PANEL_STYLES.selector}
        role="radiogroup"
        aria-label="Select lineage view mode"
        data-graph-panel
      >
        {VIEW_MODES.map((mode) => {
          const isActive = viewMode === mode.value;
          const Icon = mode.icon;

          return (
            <GraphTooltip key={mode.value} delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  type="button"
                  role="radio"
                  aria-checked={isActive}
                  aria-label={mode.label}
                  onClick={() => setViewMode(mode.value)}
                  className={`
                    inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-all duration-200
                    ${
                      isActive
                        ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                        : 'text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'
                    }
                    focus-visible:outline-none
                  `}
                >
                  <Icon className="size-4" strokeWidth={isActive ? 2.5 : 1.5} />
                </button>
              </GraphTooltipTrigger>
              <GraphTooltipPortal>
                <GraphTooltipContent side="bottom">
                  <p>{mode.description}</p>
                  <GraphTooltipArrow />
                </GraphTooltipContent>
              </GraphTooltipPortal>
            </GraphTooltip>
          );
        })}
      </div>
    </GraphTooltipProvider>
  );
}
