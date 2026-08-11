import type { JSX, ReactNode } from 'react';
import { Background, Controls, MiniMap, Panel } from '@xyflow/react';
import { GitBranch, LayoutList, Maximize2, Minimize2, Route } from 'lucide-react';

import { getMinimapNodeColor, PANEL_STYLES } from '../constants';
import type { SearchableType } from '../hooks/useSearchSuggestions';
import type { LineageViewMode } from '../types';
import { isTableNodeData } from '../utils/graphTraversal';
import { GraphSearchControl } from './GraphSearchControl';
import { LayoutProgressIndicator } from './LayoutProgressIndicator';
import { LayoutSelector } from './LayoutSelector';
import { Legend } from './Legend';
import { TableFilterDropdown } from './TableFilterDropdown';
import { ViewModeSelector } from './ViewModeSelector';
import {
  GraphTooltip,
  GraphTooltipArrow,
  GraphTooltipContent,
  GraphTooltipPortal,
  GraphTooltipProvider,
  GraphTooltipTrigger,
} from './ui/graph-tooltip';

interface ToolbarToggleButtonProps {
  isActive: boolean;
  onClick: () => void;
  ariaLabel: string;
  tooltip: string;
  icon: ReactNode;
}

/**
 * Reusable toggle button for graph toolbar actions.
 * Provides consistent styling and tooltip behavior.
 */
function ToolbarToggleButton({
  isActive,
  onClick,
  ariaLabel,
  tooltip,
  icon,
}: ToolbarToggleButtonProps): JSX.Element {
  return (
    <div className={PANEL_STYLES.container} data-graph-panel>
      <GraphTooltipProvider>
        <GraphTooltip delayDuration={300}>
          <GraphTooltipTrigger asChild>
            <button
              onClick={onClick}
              className={`
                inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-all duration-200
                ${isActive ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100' : 'text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'}
                focus-visible:outline-hidden
              `}
              aria-label={ariaLabel}
              aria-pressed={isActive}
            >
              {icon}
            </button>
          </GraphTooltipTrigger>
          <GraphTooltipPortal>
            <GraphTooltipContent side="bottom">
              <p>{tooltip}</p>
              <GraphTooltipArrow />
            </GraphTooltipContent>
          </GraphTooltipPortal>
        </GraphTooltip>
      </GraphTooltipProvider>
    </div>
  );
}

export interface GraphViewControlsProps {
  viewMode: LineageViewMode;
  showScriptTables: boolean;
  defaultCollapsed: boolean;
  showColumnEdges: boolean;
  searchTerm: string;
  searchableTypes: SearchableType[];
  focusMode: boolean;
  showMiniMap: boolean;
  onSearchTermChange: (searchTerm: string) => void;
  onFocusModeChange: (enabled: boolean) => void;
  onToggleShowScriptTables: () => void;
  onSetAllNodesCollapsed: (collapsed: boolean) => void;
  onToggleColumnEdges: () => void;
}

/**
 * Presentation-only controls rendered inside the ReactFlow canvas.
 */
export function GraphViewControls({
  viewMode,
  showScriptTables,
  defaultCollapsed,
  showColumnEdges,
  searchTerm,
  searchableTypes,
  focusMode,
  showMiniMap,
  onSearchTermChange,
  onFocusModeChange,
  onToggleShowScriptTables,
  onSetAllNodesCollapsed,
  onToggleColumnEdges,
}: GraphViewControlsProps): JSX.Element {
  return (
    <>
      <Background />
      <Controls />
      <Panel position="top-left" className="flex gap-3 items-start">
        <ViewModeSelector />
        {viewMode === 'script' && (
          <ToolbarToggleButton
            isActive={showScriptTables}
            onClick={onToggleShowScriptTables}
            ariaLabel="Toggle table details"
            tooltip={showScriptTables ? 'Hide tables' : 'Show tables'}
            icon={<LayoutList className="size-4" strokeWidth={showScriptTables ? 2.5 : 1.5} />}
          />
        )}
        <GraphSearchControl
          searchTerm={searchTerm}
          onSearchTermChange={onSearchTermChange}
          searchableTypes={searchableTypes}
          focusMode={focusMode}
          onFocusModeChange={onFocusModeChange}
        />
        {viewMode !== 'script' && (
          <ToolbarToggleButton
            isActive={!defaultCollapsed}
            onClick={() => onSetAllNodesCollapsed(!defaultCollapsed)}
            ariaLabel={defaultCollapsed ? 'Expand all tables' : 'Collapse all tables'}
            tooltip={defaultCollapsed ? 'Expand all tables' : 'Collapse all tables'}
            icon={
              defaultCollapsed ? (
                <Maximize2 className="size-4" strokeWidth={1.5} />
              ) : (
                <Minimize2 className="size-4" strokeWidth={1.5} />
              )
            }
          />
        )}
        {viewMode !== 'script' && (
          <ToolbarToggleButton
            isActive={showColumnEdges}
            onClick={onToggleColumnEdges}
            ariaLabel={showColumnEdges ? 'Show table connections' : 'Show column lineage'}
            tooltip={showColumnEdges ? 'Show table connections' : 'Show column lineage'}
            icon={
              showColumnEdges ? (
                <GitBranch className="size-4" strokeWidth={1.5} />
              ) : (
                <Route className="size-4" strokeWidth={1.5} />
              )
            }
          />
        )}
        {viewMode !== 'script' && <TableFilterDropdown />}
      </Panel>
      <Panel position="top-right" className="flex gap-3 items-start">
        <Legend viewMode={viewMode} />
        <LayoutSelector />
      </Panel>
      <Panel position="bottom-left" className="!m-3">
        <LayoutProgressIndicator />
      </Panel>
      {showMiniMap && (
        <MiniMap
          nodeColor={(node) => {
            if (isTableNodeData(node.data)) {
              return getMinimapNodeColor(node.data.nodeType || 'table');
            }
            // For script nodes, check node type from id prefix
            if (node.id.startsWith('script:')) {
              return getMinimapNodeColor('script');
            }
            return getMinimapNodeColor('table');
          }}
        />
      )}
    </>
  );
}
