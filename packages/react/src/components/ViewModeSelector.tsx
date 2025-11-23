import { FileCode, Table2, Columns3 } from 'lucide-react';
import { useLineage } from '../context';
import type { LineageViewMode } from '../types';
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
      <div className="flex items-center" role="radiogroup" aria-label="Select lineage view mode">
        {VIEW_MODES.map((mode, index) => {
          const isActive = viewMode === mode.value;
          const Icon = mode.icon;

          return (
            <div key={mode.value} className="flex items-center">
              <GraphTooltip delayDuration={300}>
                <GraphTooltipTrigger asChild>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={isActive}
                    aria-label={mode.label}
                    onClick={() => setViewMode(mode.value)}
                    className={`
                      flex h-8 w-8 shrink-0 items-center justify-center rounded transition-colors
                      ${
                        isActive
                          ? 'bg-slate-200 text-slate-900'
                          : 'text-slate-500 hover:bg-slate-50 hover:text-slate-900'
                      }
                      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40
                    `}
                  >
                    <Icon className="h-4 w-4" strokeWidth={isActive ? 2.5 : 1.5} />
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipPortal>
                  <GraphTooltipContent side="bottom">
                    <p>{mode.description}</p>
                    <GraphTooltipArrow />
                  </GraphTooltipContent>
                </GraphTooltipPortal>
              </GraphTooltip>
              {index < VIEW_MODES.length - 1 && (
                <div className="h-5 w-px bg-slate-300 mx-0.5" />
              )}
            </div>
          );
        })}
      </div>
    </GraphTooltipProvider>
  );
}
