import type { JSX } from 'react';
import { Network, Workflow } from 'lucide-react';
import { useLineage } from '../store';
import type { LayoutAlgorithm } from '../types';
import { PANEL_STYLES } from '../constants';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';

const LAYOUT_OPTIONS: Array<{
  value: LayoutAlgorithm;
  label: string;
  description: string;
  icon: React.ElementType;
}> = [
  {
    value: 'dagre',
    label: 'Dagre',
    description: 'Fast hierarchical layout',
    icon: Network,
  },
  {
    value: 'elk',
    label: 'ELK',
    description: 'Advanced layout with edge crossing minimization',
    icon: Workflow,
  },
];

/**
 * Segmented control for switching between different layout algorithms.
 */
export function LayoutSelector(): JSX.Element {
  const { state, actions } = useLineage();
  const { layoutAlgorithm } = state;
  const { setLayoutAlgorithm } = actions;

  return (
    <GraphTooltipProvider>
      <div
        className={PANEL_STYLES.selector}
        role="radiogroup"
        aria-label="Select layout algorithm"
        data-graph-panel
      >
        {LAYOUT_OPTIONS.map((option) => {
          const isActive = layoutAlgorithm === option.value;
          const Icon = option.icon;

          return (
            <GraphTooltip key={option.value} delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  type="button"
                  role="radio"
                  aria-checked={isActive}
                  aria-label={option.label}
                  onClick={() => setLayoutAlgorithm(option.value)}
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
                  <p>{option.description}</p>
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
