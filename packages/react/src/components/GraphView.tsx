import { useMemo, useCallback, useEffect, useRef, useState, useDeferredValue, type JSX } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  useReactFlow,
  Panel,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge, Viewport } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { LayoutList, Maximize2, Minimize2, Route, GitBranch } from 'lucide-react';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';

import { useLineage, useLineageStore } from '../store';
import { useNodeFocus } from '../hooks/useNodeFocus';
import { useGraphFiltering } from '../hooks/useGraphFiltering';
import type { GraphViewProps, TableNodeData, LayoutAlgorithm } from '../types';
import { getLayoutedElements, getLayoutedElementsInWorker, getFastLayoutedNodes, cancelLayoutRequests } from '../utils/layout';
import { LayoutSelector } from './LayoutSelector';
import { isTableNodeData } from '../utils/graphTraversal';
import {
  mergeStatements,
  buildFlowNodes,
  buildFlowEdges,
  buildScriptLevelGraph,
} from '../utils/graphBuilders';
import { ScriptNode } from './ScriptNode';
import { ColumnNode } from './ColumnNode';
import { SimpleTableNode } from './SimpleTableNode';
import { TableNode } from './TableNode';
import { AnimatedEdge } from './AnimatedEdge';
import { ViewModeSelector } from './ViewModeSelector';
import { GraphSearchControl } from './GraphSearchControl';
import { TableFilterDropdown } from './TableFilterDropdown';
import { Legend } from './Legend';
import type { SearchableType } from '../hooks/useSearchSuggestions';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';
import { GRAPH_CONFIG, PANEL_STYLES, getMinimapNodeColor } from '../constants';

const MINIMAP_NODE_LIMIT = 2000;
const ELK_NODE_LIMIT = 2000;

const nowMs = () => {
  if (typeof performance !== 'undefined' && typeof performance.now === 'function') {
    return performance.now();
  }
  return Date.now();
};

/**
 * Helper component to handle node focusing.
 * Must be rendered inside ReactFlow to access useReactFlow hook.
 */
function NodeFocusHandler({
  focusNodeId,
  onFocusApplied,
}: {
  focusNodeId?: string;
  onFocusApplied?: () => void;
}): null {
  useNodeFocus({ focusNodeId, onFocusApplied });
  return null;
}

/**
 * Helper component to trigger fitView when fitViewTrigger changes.
 * Must be rendered inside ReactFlow to access useReactFlow hook.
 */
function FitViewHandler({ trigger }: { trigger?: number }): null {
  const { fitView } = useReactFlow();
  const lastTriggerRef = useRef(trigger);

  useEffect(() => {
    if (trigger !== undefined && trigger !== lastTriggerRef.current) {
      lastTriggerRef.current = trigger;
      // Small delay to ensure nodes are rendered
      setTimeout(() => {
        fitView({ padding: 0.2, duration: 200 });
      }, 50);
    }
  }, [trigger, fitView]);

  return null;
}

/**
 * Helper component to handle viewport changes and restoration.
 * Must be rendered inside ReactFlow to access useReactFlow hook.
 */
function ViewportHandler({
  initialViewport,
  onViewportChange,
}: {
  initialViewport?: Viewport;
  onViewportChange?: (viewport: Viewport) => void;
}): null {
  const { setViewport, getViewport } = useReactFlow();
  const hasRestoredRef = useRef(false);
  const previousInitialViewportRef = useRef<Viewport | undefined>(initialViewport);
  const viewportChangeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (viewportChangeTimerRef.current) {
        clearTimeout(viewportChangeTimerRef.current);
      }
    };
  }, []);

  // Reset restoration flag when initial viewport changes (e.g., project switch)
  useEffect(() => {
    if (previousInitialViewportRef.current !== initialViewport) {
      hasRestoredRef.current = false;
      previousInitialViewportRef.current = initialViewport;
    }
  }, [initialViewport]);

  // Restore initial viewport as needed
  useEffect(() => {
    if (initialViewport && !hasRestoredRef.current) {
      // Delay to ensure ReactFlow is ready
      const timer = setTimeout(() => {
        setViewport(initialViewport, { duration: 0 });
        hasRestoredRef.current = true;
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [initialViewport, setViewport]);

  // Debounced viewport change callback
  useEffect(() => {
    if (!onViewportChange) return;

    const handleViewportChange = () => {
      if (viewportChangeTimerRef.current) {
        clearTimeout(viewportChangeTimerRef.current);
      }
      viewportChangeTimerRef.current = setTimeout(() => {
        const viewport = getViewport();
        onViewportChange(viewport);
      }, 300);
    };

    // Use MutationObserver on the viewport's style attribute rather than ReactFlow's
    // onMove/onViewportChange events. Those events fire at very high frequency during
    // pan/zoom operations which would cause excessive state updates and re-renders.
    // The MutationObserver approach with debouncing provides more reliable, batched updates.
    const container = document.querySelector('.react-flow__viewport');
    if (container) {
      const observer = new MutationObserver(handleViewportChange);
      observer.observe(container, { attributes: true, attributeFilter: ['style'] });
      return () => {
        observer.disconnect();
        if (viewportChangeTimerRef.current) {
          clearTimeout(viewportChangeTimerRef.current);
        }
      };
    }
  }, [onViewportChange, getViewport]);

  return null;
}

// Type guard for data with isSelected property
interface SelectableNodeData {
  isSelected?: boolean;
  [key: string]: unknown;
}

function isSelectableNodeData(data: unknown): data is SelectableNodeData {
  return typeof data === 'object' && data !== null;
}

const nodeTypes = {
  tableNode: TableNode,
  simpleTableNode: SimpleTableNode,
  scriptNode: ScriptNode,
  columnNode: ColumnNode,
};

const edgeTypes = {
  animated: AnimatedEdge,
};

interface ToolbarToggleButtonProps {
  isActive: boolean;
  onClick: () => void;
  ariaLabel: string;
  tooltip: string;
  icon: React.ReactNode;
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

function enhanceGraphWithHighlights(
  graph: { nodes: FlowNode[]; edges: FlowEdge[] },
  highlightIds: Set<string>
): { nodes: FlowNode[]; edges: FlowEdge[] } {
  const enhancedNodes = graph.nodes.map((node) => {
    const isHighlighted = highlightIds.has(node.id);

    // Handle Table Nodes with columns
    if (isTableNodeData(node.data)) {
      const nodeData = node.data;
      const enhancedColumns = nodeData.columns.map((col) => ({
        ...col,
        isHighlighted: highlightIds.has(col.id),
      }));

      return {
        ...node,
        data: {
          ...nodeData,
          columns: enhancedColumns,
          isSelected: nodeData.isSelected || isHighlighted,
        },
      };
    }

    // Handle Script Nodes and generic nodes
    const currentIsSelected = isSelectableNodeData(node.data) ? node.data.isSelected : false;
    return {
      ...node,
      data: {
        ...node.data,
        isSelected: currentIsSelected || isHighlighted,
      },
    };
  });

  const enhancedEdges = graph.edges.map((edge) => ({
    ...edge,
    animated: highlightIds.has(edge.id),
    zIndex: highlightIds.has(edge.id) ? GRAPH_CONFIG.HIGHLIGHTED_EDGE_Z_INDEX : 0,
    data: {
      ...edge.data,
      isHighlighted: highlightIds.has(edge.id),
    },
  }));

  return { nodes: enhancedNodes, edges: enhancedEdges };
}

export function GraphView({
  className,
  onNodeClick,
  graphContainerRef,
  focusNodeId,
  onFocusApplied,
  controlledSearchTerm,
  onSearchTermChange,
  initialViewport,
  onViewportChange,
  fitViewTrigger,
  namespaceFilter,
}: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const setLayoutMetrics = useLineageStore((store) => store.setLayoutMetrics);
  const setGraphMetrics = useLineageStore((store) => store.setGraphMetrics);
  const { result, selectedNodeId, searchTerm, viewMode, layoutAlgorithm, collapsedNodeIds, defaultCollapsed, showColumnEdges, showScriptTables, expandedTableIds, tableFilter } = state;
  const deferredResult = useDeferredValue(result);

  // Determine if search is controlled externally
  const isSearchControlled = controlledSearchTerm !== undefined;

  // The effective search term used for graph filtering
  const effectiveSearchTerm = isSearchControlled ? controlledSearchTerm : searchTerm;

  // Focus mode - when enabled, only show nodes in the search lineage path
  const [focusMode, setFocusMode] = useState(false);

  // Handle search term changes - just update store or call callback, no local state
  const handleSearchTermChange = useCallback((newSearchTerm: string) => {
    if (isSearchControlled) {
      onSearchTermChange?.(newSearchTerm);
    } else {
      actions.setSearchTerm(newSearchTerm);
    }
  }, [isSearchControlled, onSearchTermChange, actions]);

  // Handle focus mode changes
  const handleFocusModeChange = useCallback((enabled: boolean) => {
    setFocusMode(enabled);
  }, []);

  const statement = useMemo(() => {
    if (!deferredResult || !deferredResult.statements) return null;
    return mergeStatements(deferredResult.statements);
  }, [deferredResult]);

  // Determine searchable types based on view mode and column edges setting
  const searchableTypes = useMemo((): SearchableType[] => {
    switch (viewMode) {
      case 'script':
        return ['script', 'table', 'view', 'cte'];
      case 'table':
      default:
        // When showing column edges, include columns in searchable types
        return showColumnEdges
          ? ['table', 'view', 'cte', 'column', 'script']
          : ['table', 'view', 'cte', 'script'];
    }
  }, [viewMode, showColumnEdges]);

  // Build the raw graph based on view mode (before filtering)
  const { builtGraph, direction, buildDurationMs } = useMemo(() => {
    const buildStart = nowMs();

    if (!deferredResult || !deferredResult.statements) {
      return { builtGraph: { nodes: [], edges: [] }, direction: 'LR' as const, buildDurationMs: null };
    }

    try {
      if (viewMode === 'script') {
        const graph = buildScriptLevelGraph(
          deferredResult.statements,
          selectedNodeId,
          effectiveSearchTerm,
          showScriptTables
        );
        return {
          builtGraph: graph,
          direction: 'LR' as const,
          buildDurationMs: nowMs() - buildStart,
        };
      }

      // Table view (with optional column-level edges)
      if (!statement) {
        return { builtGraph: { nodes: [], edges: [] }, direction: 'LR' as const, buildDurationMs: null };
      }
      const graph = {
        nodes: buildFlowNodes(
          statement,
          selectedNodeId,
          effectiveSearchTerm,
          collapsedNodeIds,
          expandedTableIds,
          deferredResult.resolvedSchema,
          defaultCollapsed,
          deferredResult.globalLineage
        ),
        edges: buildFlowEdges(statement, showColumnEdges, defaultCollapsed, collapsedNodeIds),
      };
      return {
        builtGraph: graph,
        direction: 'LR' as const,
        buildDurationMs: nowMs() - buildStart,
      };
    } catch (error) {
      console.error('Graph building failed:', error);
      return { builtGraph: { nodes: [], edges: [] }, direction: 'LR' as const, buildDurationMs: null };
    }
  }, [deferredResult, statement, selectedNodeId, effectiveSearchTerm, viewMode, collapsedNodeIds, defaultCollapsed, showColumnEdges, showScriptTables, expandedTableIds]);

  // Apply filtering (focus mode, table filter, namespace filter) and compute highlights
  const { filteredGraph, highlightIds } = useGraphFiltering({
    graph: builtGraph,
    selectedNodeId,
    searchTerm: effectiveSearchTerm,
    viewMode,
    showColumnEdges,
    focusMode,
    tableFilter,
    namespaceFilter,
  });

  // Enhance graph with highlight styling
  const { rawNodes, rawEdges } = useMemo(() => {
    const enhanced = enhanceGraphWithHighlights(filteredGraph, highlightIds);
    return { rawNodes: enhanced.nodes, rawEdges: enhanced.edges };
  }, [filteredGraph, highlightIds]);

  const showMiniMap = rawNodes.length > 0 && rawNodes.length <= MINIMAP_NODE_LIMIT;

  useEffect(() => {
    if (!deferredResult) {
      return;
    }

    setGraphMetrics({
      lastDurationMs: buildDurationMs,
      nodeCount: builtGraph.nodes.length,
      edgeCount: builtGraph.edges.length,
      lastUpdatedAt: Date.now(),
    });
  }, [deferredResult, buildDurationMs, builtGraph.nodes.length, builtGraph.edges.length, setGraphMetrics]);

  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([]);

  // State for async layout results
  const [layoutedNodes, setLayoutedNodes] = useState<FlowNode[]>([]);
  const [layoutedEdges, setLayoutedEdges] = useState<FlowEdge[]>([]);
  const layoutStartRef = useRef<number | null>(null);
  const layoutSnapshotRef = useRef<{
    resultSummary: AnalyzeResult['summary'] | null;
    viewMode: typeof viewMode;
    showScriptTables: typeof showScriptTables;
    layoutAlgorithm: LayoutAlgorithm;
    defaultCollapsed: boolean;
  } | null>(null);

  // Apply layout using Web Worker for non-blocking UI.
  //
  // This effect implements a two-stage progressive rendering pattern:
  // 1. Immediately update nodes with preserved positions to avoid jarring resets
  // 2. Asynchronously compute layout in worker, then apply final positions
  //
  // The "double render" is intentional - it provides immediate visual feedback
  // while the layout computes, preventing a blank → populated transition.
  useEffect(() => {
    if (rawNodes.length === 0) {
      setLayoutedNodes([]);
      setLayoutedEdges([]);
      setNodes([]);
      setEdges([]);
      return;
    }

    const effectiveLayoutAlgorithm =
      layoutAlgorithm === 'elk' && rawNodes.length > ELK_NODE_LIMIT ? 'dagre' : layoutAlgorithm;

    let cancelled = false;
    layoutStartRef.current = performance.now();
    layoutSnapshotRef.current = {
      resultSummary: deferredResult ? deferredResult.summary : null,
      viewMode,
      showScriptTables,
      layoutAlgorithm: effectiveLayoutAlgorithm,
      defaultCollapsed,
    };

    // Stage 1: Preserve existing node positions for smoother transitions.
    // This prevents nodes from jumping to origin (0,0) while layout computes.
    setNodes((currentNodes) => {
      if (currentNodes.length === 0) {
        return getFastLayoutedNodes(rawNodes, direction);
      }
      const positionMap = new Map(currentNodes.map((node) => [node.id, node.position]));
      return rawNodes.map((node) => {
        const position = positionMap.get(node.id);
        return position ? { ...node, position } : node;
      });
    });
    setEdges(rawEdges);

    // Use worker-based layout for both algorithms to keep UI responsive
    getLayoutedElementsInWorker(rawNodes, rawEdges, direction, effectiveLayoutAlgorithm)
      .then(({ nodes, edges }) => {
        if (!cancelled) {
          setLayoutedNodes(nodes);
          setLayoutedEdges(edges);
          const durationMs = layoutStartRef.current !== null
            ? performance.now() - layoutStartRef.current
            : null;
          setLayoutMetrics({
            lastDurationMs: durationMs,
            nodeCount: nodes.length,
            edgeCount: edges.length,
            algorithm: effectiveLayoutAlgorithm,
            lastUpdatedAt: Date.now(),
          });
        }
      })
      .catch((error) => {
        console.error('Layout failed:', error);
        // Final fallback to sync dagre on main thread
        if (!cancelled) {
          const { nodes, edges } = getLayoutedElements(rawNodes, rawEdges, direction, 'dagre');
          setLayoutedNodes(nodes);
          setLayoutedEdges(edges);
          const durationMs = layoutStartRef.current !== null
            ? performance.now() - layoutStartRef.current
            : null;
          setLayoutMetrics({
            lastDurationMs: durationMs,
            nodeCount: nodes.length,
            edgeCount: edges.length,
            algorithm: 'dagre',
            lastUpdatedAt: Date.now(),
          });
        }
      });

    return () => {
      cancelled = true;
      cancelLayoutRequests();
    };
  }, [rawNodes, rawEdges, direction, layoutAlgorithm, defaultCollapsed, showScriptTables, viewMode, deferredResult, setNodes, setEdges, setLayoutMetrics]);

  const isInitialized = useRef(false);
  const lastResultId = useRef<string | null>(null);
  const lastViewMode = useRef<string | null>(null);
  const lastShowTables = useRef<boolean | null>(null);
  const lastLayoutAlgorithm = useRef<LayoutAlgorithm | null>(null);
  const lastAppliedDefaultCollapsed = useRef<boolean | null>(null);

  // Track last applied collapse states to detect individual node collapse changes
  const lastAppliedCollapseStates = useRef<Map<string, boolean>>(new Map());

  // Stage 2: Apply computed layout positions once the worker completes.
  // This effect runs when layoutedNodes/layoutedEdges update, applying the
  // final positions. It handles two cases:
  // - Full update: apply all computed positions (view mode change, new data, etc.)
  // - Incremental update: preserve user-dragged positions, only update node data
  useEffect(() => {
    if (layoutedNodes.length === 0) return;

    const layoutSnapshot = layoutSnapshotRef.current;
    if (!layoutSnapshot) {
      return;
    }

    // Note: The layoutIsStale check was removed because it's incompatible with
    // async Web Worker layout. With async layout, layoutedNodes always reflects
    // the collapsed state at the time layout was computed, and we should render
    // that state rather than blocking until a new layout completes.

    const currentResultId = layoutSnapshot.resultSummary
      ? JSON.stringify(layoutSnapshot.resultSummary)
      : null;
    const defaultCollapseChanged = layoutSnapshot.defaultCollapsed !== lastAppliedDefaultCollapsed.current;

    // Check if any individual node's collapse state changed (affects node height/layout)
    const nodeCollapseChanged = layoutedNodes.some((node) => {
      if (!isTableNodeData(node.data)) return false;
      const currentCollapsed = node.data.isCollapsed ?? false;
      const lastCollapsed = lastAppliedCollapseStates.current.get(node.id);
      return lastCollapsed !== undefined && lastCollapsed !== currentCollapsed;
    });

    // Trigger full layout reapplication when view-affecting settings change
    const needsFullUpdate =
      !isInitialized.current ||
      currentResultId !== lastResultId.current ||
      layoutSnapshot.viewMode !== lastViewMode.current ||
      layoutSnapshot.showScriptTables !== lastShowTables.current ||
      layoutSnapshot.layoutAlgorithm !== lastLayoutAlgorithm.current ||
      defaultCollapseChanged ||
      nodeCollapseChanged;

    if (needsFullUpdate) {
      setNodes(layoutedNodes);
      setEdges(layoutedEdges);
      isInitialized.current = true;
      lastResultId.current = currentResultId;
      lastViewMode.current = layoutSnapshot.viewMode;
      lastShowTables.current = layoutSnapshot.showScriptTables;
      lastLayoutAlgorithm.current = layoutSnapshot.layoutAlgorithm;
      lastAppliedDefaultCollapsed.current = layoutSnapshot.defaultCollapsed;
    } else {
      // Preserve user-adjusted positions while updating node data
      setNodes((currentNodes) => {
        return layoutedNodes.map((layoutNode) => {
          const currentNode = currentNodes.find((n) => n.id === layoutNode.id);
          if (currentNode) {
            return { ...layoutNode, position: currentNode.position };
          }
          return layoutNode;
        });
      });
      setEdges(layoutedEdges);
    }

    // Update tracked collapse states
    const newCollapseStates = new Map<string, boolean>();
    for (const node of layoutedNodes) {
      if (isTableNodeData(node.data)) {
        newCollapseStates.set(node.id, node.data.isCollapsed ?? false);
      }
    }
    lastAppliedCollapseStates.current = newCollapseStates;
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges]);

  const internalGraphRef = useRef<HTMLDivElement>(null);
  const finalRef = graphContainerRef || internalGraphRef;

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: FlowNode) => {
      actions.selectNode(node.id);

      let sourceName: string | undefined;
      let span: { start: number; end: number } | undefined;

      // 1. Try to get source/span from node data (Script View / Hybrid View)
      if (node.data && typeof node.data === 'object') {
        if ('sourceName' in node.data && typeof node.data.sourceName === 'string') {
          sourceName = node.data.sourceName;
        }
      }

      // 2. Try to get from lineage statement (Table View / Column View)
      if (statement) {
        const lineageNode = statement.nodes.find((n) => n.id === node.id);
        if (lineageNode) {
          if (lineageNode.span) {
            actions.highlightSpan(lineageNode.span);
            span = lineageNode.span;
          }
          onNodeClick?.(lineageNode);

          // If node doesn't have sourceName, use statement's sourceName OR metadata
          if (!sourceName) {
            if (lineageNode.metadata && typeof lineageNode.metadata.sourceName === 'string') {
              sourceName = lineageNode.metadata.sourceName;
            } else if (statement.sourceName) {
              sourceName = statement.sourceName;
            }
          }
        }
      }

      // 3. Dispatch navigation request if we have a source file
      if (sourceName) {
        let targetType: 'table' | 'view' | 'cte' | 'column' | 'script' | undefined;
        const flowNodeType = node.type;

        if (flowNodeType === 'scriptNode') {
          targetType = 'script';
        } else if (flowNodeType === 'columnNode') {
          targetType = 'column';
        } else if (flowNodeType === 'tableNode' || flowNodeType === 'simpleTableNode') {
          const data = node.data as TableNodeData;
          if (data.nodeType === 'cte') targetType = 'cte';
          else if (data.nodeType === 'view') targetType = 'view';
          else targetType = 'table';
        }

        const targetName = typeof node.data?.label === 'string' ? node.data.label : undefined;

        actions.requestNavigation({
          sourceName,
          span,
          targetName,
          targetType,
        });
      }
    },
    [actions, statement, onNodeClick]
  );

  const handleEdgeClick = useCallback(
    (_event: React.MouseEvent, edge: FlowEdge) => {
      actions.selectNode(edge.id);
    },
    [actions]
  );

  const handlePaneClick = useCallback(() => {
    actions.selectNode(null);
  }, [actions]);

  if (!result || !statement) {
    return (
      <div
        className={className}
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          color: '#9ca3af',
        }}
      >
        <p>No lineage data to display</p>
      </div>
    );
  }

  return (
    <div className={className} style={{ height: '100%' }} ref={finalRef}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onEdgeClick={handleEdgeClick}
        onPaneClick={handlePaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView={!initialViewport}
        minZoom={0.1}
        maxZoom={2}
        onlyRenderVisibleElements
      >
        <NodeFocusHandler focusNodeId={focusNodeId} onFocusApplied={onFocusApplied} />
        <ViewportHandler initialViewport={initialViewport} onViewportChange={onViewportChange} />
        <FitViewHandler trigger={fitViewTrigger} />
        <Background />
        <Controls />
        <Panel position="top-left" className="flex gap-3 items-start">
          <ViewModeSelector />
          {viewMode === 'script' && (
            <ToolbarToggleButton
              isActive={showScriptTables}
              onClick={actions.toggleShowScriptTables}
              ariaLabel="Toggle table details"
              tooltip={showScriptTables ? 'Hide tables' : 'Show tables'}
              icon={<LayoutList className="size-4" strokeWidth={showScriptTables ? 2.5 : 1.5} />}
            />
          )}
          <GraphSearchControl
            searchTerm={effectiveSearchTerm ?? ''}
            onSearchTermChange={handleSearchTermChange}
            searchableTypes={searchableTypes}
            focusMode={focusMode}
            onFocusModeChange={handleFocusModeChange}
          />
          {viewMode !== 'script' && (
            <ToolbarToggleButton
              isActive={!defaultCollapsed}
              onClick={() => actions.setAllNodesCollapsed(!defaultCollapsed)}
              ariaLabel={defaultCollapsed ? 'Expand all tables' : 'Collapse all tables'}
              tooltip={defaultCollapsed ? 'Expand all tables' : 'Collapse all tables'}
              icon={defaultCollapsed ? <Maximize2 className="size-4" strokeWidth={1.5} /> : <Minimize2 className="size-4" strokeWidth={1.5} />}
            />
          )}
          {viewMode !== 'script' && (
            <ToolbarToggleButton
              isActive={showColumnEdges}
              onClick={actions.toggleColumnEdges}
              ariaLabel={showColumnEdges ? 'Show table connections' : 'Show column lineage'}
              tooltip={showColumnEdges ? 'Show table connections' : 'Show column lineage'}
              icon={showColumnEdges ? <GitBranch className="size-4" strokeWidth={1.5} /> : <Route className="size-4" strokeWidth={1.5} />}
            />
          )}
          {viewMode !== 'script' && <TableFilterDropdown />}
        </Panel>
        <Panel position="top-right" className="flex gap-3 items-start">
          <Legend viewMode={viewMode} />
          <LayoutSelector />
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
      </ReactFlow>
    </div>
  );
}
