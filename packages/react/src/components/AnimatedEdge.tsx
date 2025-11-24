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
  const customTooltip = data?.tooltip as string | undefined;

  let tooltipContent = customTooltip || '';
  if (sourceColumn && targetColumn) {
    tooltipContent += tooltipContent ? `\n${sourceColumn} → ${targetColumn}` : `${sourceColumn} → ${targetColumn}`;
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
          stroke: isHighlighted ? colors.accent : style?.stroke || '#b1b1b7',
          strokeWidth: isHighlighted ? 3 : 2,
          opacity: isHighlighted ? 1 : 0.5,
          strokeDasharray: style?.strokeDasharray,
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
                  <div
                    style={{
                      cursor: 'help',
                      backgroundColor: 'white',
                      border: `2px solid ${colors.accent}`,
                      color: colors.accent,
                      borderRadius: 12,
                      minWidth: 20,
                      height: 20,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      padding: '0 8px',
                      fontSize: 10,
                      fontWeight: 700,
                      textTransform: 'none',
                      letterSpacing: 0.3,
                      boxShadow: '0 1px 3px rgba(0,0,0,0.12)',
                    }}
                  >
                    ƒ
                  </div>
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
