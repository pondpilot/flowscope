import { useMemo, type CSSProperties, type JSX } from 'react';
import { BaseEdge, EdgeLabelRenderer, getBezierPath } from '@xyflow/react';
import type { EdgeProps } from '@xyflow/react';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
} from './ui/graph-tooltip';
import { GRAPH_CONFIG, EDGE_STYLES, JOIN_TYPE_LABELS } from '../constants';
import { useColors } from '../hooks/useColors';

export type EdgeType = 'dataFlow' | 'derivation' | 'joinDependency' | 'aggregation';

interface EdgeStyleConfig {
  stroke: string;
  strokeWidth: number;
  strokeDasharray: string | undefined;
}

type ColorPalette = ReturnType<typeof useColors>;

function getEdgeTypeStyles(colors: ColorPalette): Record<EdgeType | 'default', EdgeStyleConfig> {
  return {
    dataFlow: {
      stroke: colors.edges.dataFlow,
      strokeWidth: EDGE_STYLES.dataFlow.strokeWidth,
      strokeDasharray: EDGE_STYLES.dataFlow.strokeDasharray,
    },
    derivation: {
      stroke: colors.edges.derivation,
      strokeWidth: EDGE_STYLES.derivation.strokeWidth,
      strokeDasharray: EDGE_STYLES.derivation.strokeDasharray,
    },
    joinDependency: {
      stroke: colors.edges.joinDependency,
      strokeWidth: EDGE_STYLES.joinDependency.strokeWidth,
      strokeDasharray: EDGE_STYLES.joinDependency.strokeDasharray,
    },
    aggregation: {
      stroke: colors.edges.aggregation,
      strokeWidth: EDGE_STYLES.aggregation.strokeWidth,
      strokeDasharray: EDGE_STYLES.aggregation.strokeDasharray,
    },
    default: {
      stroke: colors.edges.dataFlow,
      strokeWidth: EDGE_STYLES.dataFlow.strokeWidth,
      strokeDasharray: EDGE_STYLES.dataFlow.strokeDasharray,
    },
  };
}

/**
 * Get edge styling based on edge type and highlight state
 */
function getEdgeStyle(
  edgeType: EdgeType | string | undefined,
  isHighlighted: boolean | undefined,
  colors: ColorPalette,
  customStyle?: CSSProperties
): CSSProperties {
  const edgeTypeStyles = getEdgeTypeStyles(colors);
  const baseStyle = edgeTypeStyles[edgeType as EdgeType] || edgeTypeStyles.default;

  if (isHighlighted) {
    return {
      stroke: colors.edges.highlighted,
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
  const colors = useColors();
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
  const joinType = data?.joinType as string | undefined;
  const joinCondition = data?.joinCondition as string | undefined;
  const labelZIndex = isHighlighted ? GRAPH_CONFIG.HIGHLIGHTED_EDGE_Z_INDEX : GRAPH_CONFIG.EDGE_LABEL_BASE_Z_INDEX;

  const tooltipContent = useMemo(() => {
    let content = customTooltip || '';
    if (sourceColumn && targetColumn) {
      content += content ? `\n${sourceColumn} → ${targetColumn}` : `${sourceColumn} → ${targetColumn}`;
    }
    if (joinType) {
      content += content ? '\n\n' : '';
      content += `Join: ${JOIN_TYPE_LABELS[joinType] || joinType.replace(/_/g, ' ')}`;
    }
    if (joinCondition) {
      content += content ? '\n' : '';
      content += `ON ${joinCondition}`;
    }
    if (expression) {
      content += content ? '\n\n' : '';
      content += `Expression:\n${expression}`;
    }
    return content;
  }, [customTooltip, sourceColumn, targetColumn, expression, joinType, joinCondition]);

  const edgeStyle = useMemo(
    () => getEdgeStyle(edgeType, isHighlighted, colors, style),
    [edgeType, isHighlighted, colors, style]
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
              zIndex: labelZIndex,
            }}
          >
            <GraphTooltipProvider>
              <GraphTooltip delayDuration={GRAPH_CONFIG.TOOLTIP_DELAY}>
                <GraphTooltipTrigger asChild>
                  <div
                    style={{
                      cursor: 'help',
                      backgroundColor: colors.nodes.table.bg,
                      border: `2px solid ${colors.edges.derivation}`,
                      color: colors.edges.derivation,
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
      {!expression && joinType && (
        <EdgeLabelRenderer>
          <div
            style={{
              position: 'absolute',
              transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
              pointerEvents: 'all',
              zIndex: labelZIndex,
            }}
          >
            <GraphTooltipProvider>
              <GraphTooltip delayDuration={GRAPH_CONFIG.TOOLTIP_DELAY}>
                <GraphTooltipTrigger asChild>
                  <div
                    style={{
                      cursor: joinCondition ? 'help' : 'default',
                      backgroundColor: colors.nodes.table.bg,
                      border: `1px solid ${colors.edges.dataFlow}`,
                      color: colors.edges.dataFlow,
                      borderRadius: 4,
                      padding: '2px 6px',
                      fontSize: 9,
                      fontWeight: 600,
                      textTransform: 'uppercase',
                      letterSpacing: 0.5,
                      boxShadow: '0 1px 2px rgba(0,0,0,0.1)',
                    }}
                  >
                    {JOIN_TYPE_LABELS[joinType] || joinType.replace(/_/g, ' ')}
                  </div>
                </GraphTooltipTrigger>
                {joinCondition && (
                  <GraphTooltipContent side="top">
                    <span style={{ fontWeight: 600 }}>ON</span> {joinCondition}
                    <GraphTooltipArrow />
                  </GraphTooltipContent>
                )}
              </GraphTooltip>
            </GraphTooltipProvider>
          </div>
        </EdgeLabelRenderer>
      )}
      {!expression && !joinType && tooltipContent && (
        <g transform={`translate(${labelX}, ${labelY})`}>
          <title>{tooltipContent}</title>
        </g>
      )}
    </>
  );
}
