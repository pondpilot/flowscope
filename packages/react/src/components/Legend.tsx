import { useState, type ReactNode, type JSX } from 'react';
import { ChevronDown, ChevronUp, Table2, Database, FileCode, Columns3, Eye } from 'lucide-react';
import { COLORS, EDGE_STYLES, PANEL_STYLES } from '../constants';

interface LegendProps {
  viewMode?: 'script' | 'table';
}

/**
 * Legend component explaining the visual elements in the lineage graph.
 * Collapsible panel that shows in the bottom-left corner of the graph.
 */
export function Legend({ viewMode = 'table' }: LegendProps): JSX.Element {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="relative">
      <div
        className={`${PANEL_STYLES.container} px-1.5 transition-all duration-200`}
        data-graph-panel
      >
        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className={`
            flex items-center gap-2 h-7 px-3 rounded-full transition-all duration-200 text-sm font-medium
            ${isExpanded ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100' : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-100'}
          `}
        >
          <span>Legend</span>
          {isExpanded ? <ChevronUp className="size-4" /> : <ChevronDown className="size-4" />}
        </button>
      </div>

      {/* Expanded content */}
      {isExpanded && (
        <div
          className="absolute right-0 top-full mt-2 w-64 rounded-xl border border-slate-200/60 dark:border-slate-700/60 bg-white dark:bg-slate-900 shadow-lg p-3 space-y-3 z-50 animate-in fade-in zoom-in-95 duration-200"
          data-graph-panel
        >
          {/* Nodes section */}
          <div>
            <div className="text-xs font-semibold uppercase tracking-wider mb-2 text-slate-500 dark:text-slate-400">
              Nodes
            </div>
            <div className="space-y-1.5">
              <LegendNodeItem
                icon={<Table2 className="h-3 w-3" />}
                label="Table"
                color={COLORS.nodes.table.accent}
                bgColor={COLORS.nodes.table.headerBg}
              />
              <LegendNodeItem
                icon={<Eye className="h-3 w-3" />}
                label="View"
                sublabel="Defined query"
                color={COLORS.nodes.view.accent}
                bgColor={COLORS.nodes.view.headerBg}
              />
              <LegendNodeItem
                icon={<Database className="h-3 w-3" />}
                label="CTE"
                sublabel="Temporary result"
                color={COLORS.nodes.cte.accent}
                bgColor={COLORS.nodes.cte.headerBg}
              />
              <LegendNodeItem
                icon={<Columns3 className="h-3 w-3" />}
                label="Output"
                sublabel="Final result"
                color={COLORS.nodes.virtualOutput.accent}
                bgColor={COLORS.nodes.virtualOutput.headerBg}
              />
              {viewMode === 'script' && (
                <LegendNodeItem
                  icon={<FileCode className="h-3 w-3" />}
                  label="Script"
                  sublabel="SQL file"
                  color={COLORS.nodes.script.accent}
                  bgColor={COLORS.nodes.script.headerBg}
                />
              )}
            </div>
          </div>

          {/* Edges section */}
          <div>
            <div className="text-xs font-semibold uppercase tracking-wider mb-2 text-slate-500 dark:text-slate-400">
              Edges
            </div>
            <div className="space-y-1.5">
              <LegendEdgeItem
                style="solid"
                color={EDGE_STYLES.dataFlow.stroke}
                label="Data flow"
                sublabel="Direct movement"
              />
              <LegendEdgeItem
                style="dotted"
                color={EDGE_STYLES.joinDependency.stroke}
                label="Join dependency"
                sublabel="Join-only filter"
              />
              <LegendEdgeItem
                style="dashed"
                color={EDGE_STYLES.derivation.stroke}
                label="Derivation"
                sublabel="Transformation"
              />
            </div>
          </div>

          {/* States section */}
          <div>
            <div className="text-xs font-semibold uppercase tracking-wider mb-2 text-slate-500 dark:text-slate-400">
              States
            </div>
            <div className="space-y-1.5">
              <LegendStateItem color={COLORS.interactive.selection} label="Selected" filled />
              <LegendStateItem
                color={COLORS.interactive.selection}
                label="Related"
                filled={false}
              />
              <LegendStateItem color={COLORS.recursive} label="Recursive" filled={false} />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface LegendNodeItemProps {
  icon: ReactNode;
  label: string;
  sublabel?: string;
  color: string;
  bgColor: string;
}

function LegendNodeItem({
  icon,
  label,
  sublabel,
  color,
  bgColor,
}: LegendNodeItemProps): JSX.Element {
  return (
    <div className="flex items-center gap-2">
      <div
        className="flex h-5 w-5 items-center justify-center rounded"
        style={{ backgroundColor: bgColor, color }}
      >
        {icon}
      </div>
      <div className="flex flex-col">
        <span className="text-xs font-medium text-slate-900 dark:text-white">{label}</span>
        {sublabel && (
          <span className="text-[10px] text-slate-500 dark:text-slate-300">{sublabel}</span>
        )}
      </div>
    </div>
  );
}

interface LegendEdgeItemProps {
  style: 'solid' | 'dashed' | 'dotted';
  color: string;
  label: string;
  sublabel?: string;
}

function LegendEdgeItem({ style, color, label, sublabel }: LegendEdgeItemProps): JSX.Element {
  const dashArray = style === 'dashed' ? '6 4' : style === 'dotted' ? '2 2' : undefined;

  return (
    <div className="flex items-center gap-2">
      <svg width="24" height="12" className="shrink-0">
        <line
          x1="0"
          y1="6"
          x2="24"
          y2="6"
          stroke={color}
          strokeWidth="2"
          strokeDasharray={dashArray}
        />
        {/* Arrow marker */}
        <polygon points="24,6 18,3 18,9" fill={color} />
      </svg>
      <div className="flex flex-col">
        <span className="text-xs font-medium text-slate-900 dark:text-white">{label}</span>
        {sublabel && (
          <span className="text-[10px] text-slate-500 dark:text-slate-300">{sublabel}</span>
        )}
      </div>
    </div>
  );
}

interface LegendStateItemProps {
  color: string;
  label: string;
  filled: boolean;
}

function LegendStateItem({ color, label, filled }: LegendStateItemProps): JSX.Element {
  return (
    <div className="flex items-center gap-2">
      <div
        className="h-3 w-3 rounded-full border-2"
        style={{
          borderColor: color,
          backgroundColor: filled ? color : 'transparent',
        }}
      />
      <span className="text-xs text-slate-900 dark:text-white">{label}</span>
    </div>
  );
}
