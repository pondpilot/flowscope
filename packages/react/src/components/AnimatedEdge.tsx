import { useMemo, type CSSProperties } from 'react';
import { BaseEdge, EdgeLabelRenderer, getBezierPath } from '@xyflow/react';
import type { EdgeProps } from '@xyflow/react';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
} from './ui/graph-tooltip';
import { GRAPH_CONFIG, COLORS, EDGE_STYLES } from '../constants';

export type EdgeType = 'dataFlow' | 'derivation' | 'aggregation';

interface EdgeStyleConfig {
  stroke: string;
  strokeWidth: number;
  strokeDasharray: string | undefined;
}

const EDGE_TYPE_STYLES: Record<EdgeType | 'default', EdgeStyleConfig> = {
  dataFlow: {
    stroke: EDGE_STYLES.dataFlow.stroke,
    strokeWidth: EDGE_STYLES.dataFlow.strokeWidth,
    strokeDasharray: EDGE_STYLES.dataFlow.strokeDasharray,
  },
  derivation: {
    stroke: EDGE_STYLES.derivation.stroke,
    strokeWidth: EDGE_STYLES.derivation.strokeWidth,
    strokeDasharray: EDGE_STYLES.derivation.strokeDasharray,
  },
  aggregation: {
    stroke: EDGE_STYLES.aggregation.stroke,
    strokeWidth: EDGE_STYLES.aggregation.strokeWidth,
    strokeDasharray: EDGE_STYLES.aggregation.strokeDasharray,
  },
  default: {
    stroke: EDGE_STYLES.dataFlow.stroke,
    strokeWidth: EDGE_STYLES.dataFlow.strokeWidth,
    strokeDasharray: EDGE_STYLES.dataFlow.strokeDasharray,
  },
};

/**
 * Get edge styling based on edge type and highlight state
 */
function getEdgeStyle(
  edgeType: EdgeType | string | undefined,
  isHighlighted: boolean | undefined,
  customStyle?: CSSProperties
): CSSProperties {
  const baseStyle = EDGE_TYPE_STYLES[edgeType as EdgeType] || EDGE_TYPE_STYLES.default;

  if (isHighlighted) {
    return {
      stroke: COLORS.edges.highlighted,
      strokeWidth: EDGE_STYLES.highlighted.strokeWidth,
      strokeDasharray: baseStyle.strokeDasharray,
      opacity: 1,
      ...customStyle,
    };
  }

  return {
    stroke: baseStyle.stroke,
    strokeWidth: baseStyle.strokeWidth,
    strokeDasharray: baseStyle.strokeDasharray,
    opacity: 0.6,
    ...customStyle,
  };
}

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
  const edgeType = data?.type as EdgeType | string | undefined;

  const tooltipContent = useMemo(() => {
    let content = customTooltip || '';
    if (sourceColumn && targetColumn) {
      content += content ? `\n${sourceColumn} → ${targetColumn}` : `${sourceColumn} → ${targetColumn}`;
    }
    if (expression) {
      content += content ? '\n\n' : '';
      content += `Expression:\n${expression}`;
    }
    return content;
  }, [customTooltip, sourceColumn, targetColumn, expression]);

  const edgeStyle = useMemo(
    () => getEdgeStyle(edgeType, isHighlighted, style),
    [edgeType, isHighlighted, style]
  );

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        markerEnd={markerEnd}
        style={edgeStyle}
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
                      border: `2px solid ${COLORS.edges.derivation}`,
                      color: COLORS.edges.derivation,
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
