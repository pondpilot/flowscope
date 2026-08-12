import type { Dispatch, JSX, SetStateAction } from 'react';
import { ArrowLeft, ArrowRight, Info, Minus } from 'lucide-react';

interface MatrixLegendProps {
  xRayMode: boolean;
  heatmapMode: boolean;
  clusterMode: boolean;
  complexityMode: boolean;
  setShowLegend: Dispatch<SetStateAction<boolean>>;
}

export function MatrixLegend({
  xRayMode,
  heatmapMode,
  clusterMode,
  complexityMode,
  setShowLegend,
}: MatrixLegendProps): JSX.Element {
  return (
    <div className="p-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-900/50 flex items-center justify-between gap-6 text-xs text-slate-500">
      <div className="flex items-center gap-6">
        <div className="flex items-center gap-2">
          <div className="p-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded shadow-xs">
            <ArrowRight
              className="h-3 w-3 text-emerald-600 dark:text-emerald-400"
              strokeWidth={3}
            />
          </div>
          <span>Writes to</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="p-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded shadow-xs">
            <ArrowLeft className="h-3 w-3 text-blue-600 dark:text-blue-400" strokeWidth={3} />
          </div>
          <span>Reads from</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="p-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded shadow-xs">
            <Minus className="h-3 w-3 text-slate-300" />
          </div>
          <span>Self</span>
        </div>
      </div>

      <div className="flex items-center gap-4 text-[10px] text-slate-400 uppercase tracking-wider font-semibold">
        {xRayMode && <span className="text-purple-500 animate-pulse">X-Ray Active</span>}
        {heatmapMode && <span className="text-orange-500">Heatmap Active</span>}
        {clusterMode && <span className="text-blue-500">Sorted by Clusters</span>}
        {complexityMode && <span className="text-teal-500">Complexity Margins</span>}
      </div>

      <button
        onClick={() => setShowLegend(false)}
        aria-label="Hide Legend"
        className="text-slate-400 hover:text-slate-600"
      >
        <Info className="h-4 w-4" />
      </button>
    </div>
  );
}
