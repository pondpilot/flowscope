import { Network, Workflow } from 'lucide-react';
import { useLineage } from '../store';
import type { LayoutAlgorithm } from '../types';
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
      <div className="flex items-center" role="radiogroup" aria-label="Select layout algorithm">
        {LAYOUT_OPTIONS.map((option, index) => {
          const isActive = layoutAlgorithm === option.value;
          const Icon = option.icon;

          return (
            <div key={option.value} className="flex items-center">
              <GraphTooltip delayDuration={300}>
                <GraphTooltipTrigger asChild>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={isActive}
                    aria-label={option.label}
                    onClick={() => setLayoutAlgorithm(option.value)}
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
                    <p>{option.description}</p>
                    <GraphTooltipArrow />
                  </GraphTooltipContent>
                </GraphTooltipPortal>
              </GraphTooltip>
              {index < LAYOUT_OPTIONS.length - 1 && (
                <div className="h-5 w-px bg-slate-300 mx-0.5" />
              )}
            </div>
          );
        })}
      </div>
    </GraphTooltipProvider>
  );
}
