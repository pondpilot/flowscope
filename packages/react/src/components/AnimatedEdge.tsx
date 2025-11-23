import { BaseEdge, EdgeLabelRenderer, getBezierPath } from '@xyflow/react';
import type { EdgeProps } from '@xyflow/react';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
} from './ui/graph-tooltip';
import { GRAPH_CONFIG, COLORS } from '../constants';

const colors = COLORS;

export function AnimatedEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerEnd,
  data,
  style,
}: EdgeProps): JSX.Element {
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  const expression = data?.expression as string | undefined;
  const sourceColumn = data?.sourceColumn as string | undefined;
  const targetColumn = data?.targetColumn as string | undefined;
  const isHighlighted = data?.isHighlighted as boolean | undefined;

  let tooltipContent = '';
  if (sourceColumn && targetColumn) {
    tooltipContent += `${sourceColumn} → ${targetColumn}`;
  }
  if (expression) {
    tooltipContent += tooltipContent ? '\n\n' : '';
    tooltipContent += `Expression:\n${expression}`;
  }

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        markerEnd={markerEnd}
        style={{
          stroke: isHighlighted ? colors.accent : (style?.stroke || '#b1b1b7'),
          strokeWidth: isHighlighted ? 3 : 2,
          opacity: isHighlighted ? 1 : 0.5,
          ...style,
        }}
      />
      {expression && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: 'absolute',
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              pointerEvents: 'all',
              zIndex: 1000,
            }}
          >
            <GraphTooltipProvider>
              <GraphTooltip delayDuration={GRAPH_CONFIG.TOOLTIP_DELAY}>
                <GraphTooltipTrigger asChild>
                  <button
                    type="button"
                    aria-label="View expression details"
                    style={{
                      cursor: 'help',
                      backgroundColor: 'white',
                      border: `2px solid ${colors.accent}`,
                      borderRadius: '50%',
                      width: 20,
                      height: 20,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      padding: 0,
                    }}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="none">
                      <path
                        d="M13 2L3 14H12L11 22L21 10H12L13 2Z"
                        fill={colors.accent}
                        stroke={colors.accent}
                        strokeWidth="2"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipContent side="top">
                  {tooltipContent}
                  <GraphTooltipArrow />
                </GraphTooltipContent>
              </GraphTooltip>
            </GraphTooltipProvider>
          </div>
        </EdgeLabelRenderer>
      )}
      {!expression && tooltipContent && (
        <g transform={`translate(${labelX}, ${labelY})`}>
          <title>{tooltipContent}</title>
        </g>
      )}
    </>
  );
}
