import { useMemo, useCallback, useEffect, useRef, useState, type JSX } from 'react';
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
import type { AnalyzeResult, Node as LineageNode } from '@pondpilot/flowscope-core';

import { useLineage, useLineageStore } from '../store';
import { useNodeFocus } from '../hooks/useNodeFocus';
import { useGraphFiltering } from '../hooks/useGraphFiltering';
import type { GraphViewProps, TableNodeData, LayoutAlgorithm } from '../types';
import { getLayoutedElements, getLayoutedElementsInWorker, getFastLayoutedNodes, cancelLayoutRequests } from '../utils/layout';
import { LayoutSelector } from './LayoutSelector';
import { isTableNodeData } from '../utils/graphTraversal';
import { GRAPH_DEBUG, nowMs } from '../utils/debug';
import {
  buildTableGraphInWorker,
  buildScriptGraphInWorker,
  cancelPendingBuilds,
} from '../utils/graphBuilderWorkerService';
import { ScriptNode } from './ScriptNode';
import { ColumnNode } from './ColumnNode';
import { SimpleTableNode } from './SimpleTableNode';
import { TableNode } from './TableNode';
import { AnimatedEdge } from './AnimatedEdge';
import { ViewModeSelector } from './ViewModeSelector';
import { GraphSearchControl } from './GraphSearchControl';
import { TableFilterDropdown } from './TableFilterDropdown';
import { Legend } from './Legend';
import { LayoutProgressIndicator } from './LayoutProgressIndicator';
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
  const setIsLayouting = useLineageStore((store) => store.setIsLayouting);
  const setIsBuilding = useLineageStore((store) => store.setIsBuilding);
  const { result, selectedNodeId, searchTerm, viewMode, layoutAlgorithm, collapsedNodeIds, defaultCollapsed, showColumnEdges, showScriptTables, expandedTableIds, tableFilter } = state;
  // Use result directly instead of useDeferredValue. The deferred approach was causing
  // ~7 second delays during concurrent rendering. Worker-based computation with
  // isBuilding/isLayouting indicators now provides better UX than deferred rendering.
  const analysisResult = result;

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

  const lineageNodeMapRef = useRef<Map<string, LineageNode>>(new Map());

  // Cleanup refs on unmount to prevent memory leaks
  useEffect(() => {
    return () => {
      lineageNodeMapRef.current.clear();
    };
  }, []);

  // Determine searchable types based on view mode and column edges setting
  const searchableTypes = useMemo((): SearchableType[] => {
    if (viewMode === 'script') {
      return ['script', 'table', 'view', 'cte'];
    }
    // Table view: include columns when showing column edges
    return showColumnEdges
      ? ['table', 'view', 'cte', 'column', 'script']
      : ['table', 'view', 'cte', 'script'];
  }, [viewMode, showColumnEdges]);

  // State for async graph building results
  const [builtGraph, setBuiltGraph] = useState<{ nodes: FlowNode[]; edges: FlowEdge[] }>({ nodes: [], edges: [] });
  const [buildDurationMs, setBuildDurationMs] = useState<number | null>(null);

  // Counter for unique build request IDs (avoids StrictMode timing confusion)
  const buildIdCounterRef = useRef(0);

  // Direction is always LR for now
  const direction = 'LR' as const;

  // Build the raw graph asynchronously in Web Worker (before filtering)
  useEffect(() => {
    if (!analysisResult || !analysisResult.statements) {
      setBuiltGraph({ nodes: [], edges: [] });
      setBuildDurationMs(null);
      lineageNodeMapRef.current = new Map();
      return;
    }

    let cancelled = false;
    const buildId = ++buildIdCounterRef.current;
    const buildStartTime = nowMs();
    setIsBuilding(true);
    lineageNodeMapRef.current = new Map();

    if (GRAPH_DEBUG) console.log(`[GraphBuilder #${buildId}] Starting async graph build`);

    // Use queueMicrotask to yield to the browser for spinner rendering
    // before starting worker communication
    queueMicrotask(() => {
      if (cancelled) {
        if (GRAPH_DEBUG) console.log(`[GraphBuilder #${buildId}] Cancelled before worker call`);
        return;
      }

      const workerStartTime = nowMs();
      if (GRAPH_DEBUG) console.log(`[GraphBuilder #${buildId}] Calling worker (${(workerStartTime - buildStartTime).toFixed(1)}ms since effect start)`);

      const buildPromise = viewMode === 'script'
        ? buildScriptGraphInWorker({
            statements: analysisResult.statements,
            selectedNodeId,
            searchTerm: effectiveSearchTerm,
            showTables: showScriptTables,
          })
        : buildTableGraphInWorker({
            statements: analysisResult.statements,
            selectedNodeId,
            searchTerm: effectiveSearchTerm,
            collapsedNodeIds,
            expandedTableIds,
            resolvedSchema: analysisResult.resolvedSchema,
            defaultCollapsed,
            globalLineage: analysisResult.globalLineage,
            showColumnEdges,
          });

      buildPromise
        .then(({ nodes, edges, lineageNodes }) => {
          const callbackTime = nowMs();
          const totalDuration = callbackTime - buildStartTime;
          const workerRoundtrip = callbackTime - workerStartTime;

          if (GRAPH_DEBUG) {
            console.log(`[GraphBuilder #${buildId}] Worker returned: ${nodes.length} nodes, ${edges.length} edges`);
            console.log(`[GraphBuilder #${buildId}] Worker roundtrip: ${workerRoundtrip.toFixed(1)}ms, Total: ${totalDuration.toFixed(1)}ms`);
          }

          if (!cancelled) {
            setBuiltGraph({ nodes, edges });
            setBuildDurationMs(totalDuration);
            setIsBuilding(false);
            if (lineageNodes) {
              lineageNodeMapRef.current = new Map(lineageNodes.map((node) => [node.id, node]));
            }
          } else {
            if (GRAPH_DEBUG) console.log(`[GraphBuilder #${buildId}] Cancelled, discarding result`);
          }
        })
        .catch((error) => {
          // Ignore cancellation errors
          if (error instanceof Error && error.message === 'Build cancelled') {
            if (GRAPH_DEBUG) console.log(`[GraphBuilder #${buildId}] Build cancelled (expected)`);
            return;
          }

          console.error(`[GraphBuilder #${buildId}] Build failed:`, error);
          if (!cancelled) {
            setBuiltGraph({ nodes: [], edges: [] });
            setBuildDurationMs(null);
            setIsBuilding(false);
            lineageNodeMapRef.current = new Map();
          }
        });
    });

    return () => {
      if (GRAPH_DEBUG) console.log(`[GraphBuilder #${buildId}] Cleanup - cancelling`);
      cancelled = true;
      cancelPendingBuilds();
    };
  }, [analysisResult, selectedNodeId, effectiveSearchTerm, viewMode, collapsedNodeIds, defaultCollapsed, showColumnEdges, showScriptTables, expandedTableIds, setIsBuilding]);

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

  // Enhance graph with highlight styling (render data), but keep layout inputs separate
  // so highlight-only changes don't trigger full layout recomputation.
  const renderGraph = useMemo(
    () => enhanceGraphWithHighlights(filteredGraph, highlightIds),
    [filteredGraph, highlightIds]
  );
  const renderGraphRef = useRef(renderGraph);

  useEffect(() => {
    renderGraphRef.current = renderGraph;
  }, [renderGraph]);

  const layoutNodes = filteredGraph.nodes;
  const layoutEdges = filteredGraph.edges;

  const renderNodeDataById = useMemo(() => {
    const map = new Map<string, FlowNode['data']>();
    for (const node of renderGraph.nodes) {
      map.set(node.id, node.data);
    }
    return map;
  }, [renderGraph.nodes]);

  const renderEdgeById = useMemo(() => {
    const map = new Map<string, FlowEdge>();
    for (const edge of renderGraph.edges) {
      map.set(edge.id, edge);
    }
    return map;
  }, [renderGraph.edges]);

  const showMiniMap = renderGraph.nodes.length > 0 && renderGraph.nodes.length <= MINIMAP_NODE_LIMIT;

  useEffect(() => {
    if (!analysisResult) {
      return;
    }

    setGraphMetrics({
      lastDurationMs: buildDurationMs,
      nodeCount: builtGraph.nodes.length,
      edgeCount: builtGraph.edges.length,
      lastUpdatedAt: Date.now(),
    });
  }, [analysisResult, buildDurationMs, builtGraph.nodes.length, builtGraph.edges.length, setGraphMetrics]);

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
    if (layoutNodes.length === 0) {
      setLayoutedNodes([]);
      setLayoutedEdges([]);
      setNodes([]);
      setEdges([]);
      return;
    }

    const effectiveLayoutAlgorithm =
      layoutAlgorithm === 'elk' && layoutNodes.length > ELK_NODE_LIMIT ? 'dagre' : layoutAlgorithm;

    let cancelled = false;
    layoutStartRef.current = performance.now();
    layoutSnapshotRef.current = {
      resultSummary: analysisResult ? analysisResult.summary : null,
      viewMode,
      showScriptTables,
      layoutAlgorithm: effectiveLayoutAlgorithm,
      defaultCollapsed,
    };

    setIsLayouting(true);

    const renderGraphSnapshot = renderGraphRef.current;

    if (GRAPH_DEBUG) console.time('[Layout] Stage 1: preserve positions');
    // Stage 1: Preserve existing node positions for smoother transitions.
    // This prevents nodes from jumping to origin (0,0) while layout computes.
    setNodes((currentNodes) => {
      if (currentNodes.length === 0) {
        if (GRAPH_DEBUG) console.time('[Layout] getFastLayoutedNodes');
        const fastResult = getFastLayoutedNodes(renderGraphSnapshot.nodes, direction);
        if (GRAPH_DEBUG) console.timeEnd('[Layout] getFastLayoutedNodes');
        return fastResult;
      }
      const positionMap = new Map(currentNodes.map((node) => [node.id, node.position]));
      return renderGraphSnapshot.nodes.map((node) => {
        const position = positionMap.get(node.id);
        return position ? { ...node, position } : node;
      });
    });
    setEdges(renderGraphSnapshot.edges);
    if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 1: preserve positions');

    if (GRAPH_DEBUG) {
      console.log('[Layout] Starting worker layout for', layoutNodes.length, 'nodes,', layoutEdges.length, 'edges');
      console.time('[Layout] Worker layout total');
    }

    // Use queueMicrotask to yield to browser for spinner rendering
    queueMicrotask(() => {
      if (cancelled) return;

      // Use worker-based layout for both algorithms to keep UI responsive
      getLayoutedElementsInWorker(layoutNodes, layoutEdges, direction, effectiveLayoutAlgorithm)
      .then(({ nodes, edges }) => {
        if (GRAPH_DEBUG) console.timeEnd('[Layout] Worker layout total');
        if (!cancelled) {
          if (GRAPH_DEBUG) console.time('[Layout] Apply layouted nodes/edges');
          setLayoutedNodes(nodes);
          setLayoutedEdges(edges);
          if (GRAPH_DEBUG) console.timeEnd('[Layout] Apply layouted nodes/edges');
          const durationMs = layoutStartRef.current !== null
            ? nowMs() - layoutStartRef.current
            : null;
          setLayoutMetrics({
            lastDurationMs: durationMs,
            nodeCount: nodes.length,
            edgeCount: edges.length,
            algorithm: effectiveLayoutAlgorithm,
            lastUpdatedAt: Date.now(),
          });
          setIsLayouting(false);
        }
      })
      .catch((error) => {
        // Ignore cancellation errors - these are expected during React StrictMode
        // double-invoke or when dependencies change rapidly
        if (error instanceof Error && error.message === 'Layout cancelled') {
          if (GRAPH_DEBUG) console.log('[Layout] Cancelled (expected)');
          return;
        }

        console.error('Layout failed:', error);
        // Final fallback to sync dagre on main thread
        if (!cancelled) {
          if (GRAPH_DEBUG) console.time('[Layout] Fallback sync layout');
          const { nodes, edges } = getLayoutedElements(layoutNodes, layoutEdges, direction, 'dagre');
          if (GRAPH_DEBUG) console.timeEnd('[Layout] Fallback sync layout');
          setLayoutedNodes(nodes);
          setLayoutedEdges(edges);
          const durationMs = layoutStartRef.current !== null
            ? nowMs() - layoutStartRef.current
            : null;
          setLayoutMetrics({
            lastDurationMs: durationMs,
            nodeCount: nodes.length,
            edgeCount: edges.length,
            algorithm: 'dagre',
            lastUpdatedAt: Date.now(),
          });
          setIsLayouting(false);
        }
      });
    });

    return () => {
      cancelled = true;
      cancelLayoutRequests();
    };
  }, [layoutNodes, layoutEdges, direction, layoutAlgorithm, defaultCollapsed, showScriptTables, viewMode, analysisResult, setNodes, setEdges, setLayoutMetrics, setIsLayouting]);

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
    if (GRAPH_DEBUG) {
      console.time('[Layout] Stage 2: apply layout positions');
      console.log('[Layout] Stage 2 triggered for', layoutedNodes.length, 'nodes');
    }

    const layoutSnapshot = layoutSnapshotRef.current;
    if (!layoutSnapshot) {
      if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 2: apply layout positions');
      return;
    }

    const hasRenderData = layoutedNodes.every((node) => renderNodeDataById.has(node.id));
    if (!hasRenderData) {
      if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 2: apply layout positions');
      return;
    }

    const applyRenderDataToNode = (node: FlowNode): FlowNode => {
      const renderData = renderNodeDataById.get(node.id);
      if (!renderData || node.data === renderData) return node;
      return { ...node, data: renderData };
    };

    const applyRenderDataToEdge = (edge: FlowEdge): FlowEdge => {
      const renderEdge = renderEdgeById.get(edge.id);
      if (!renderEdge || edge === renderEdge) return edge;
      return {
        ...edge,
        type: renderEdge.type,
        label: renderEdge.label,
        animated: renderEdge.animated,
        zIndex: renderEdge.zIndex,
        data: renderEdge.data,
        style: renderEdge.style,
      };
    };

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
      setNodes(layoutedNodes.map(applyRenderDataToNode));
      setEdges(layoutedEdges.map(applyRenderDataToEdge));
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
            return applyRenderDataToNode({ ...layoutNode, position: currentNode.position });
          }
          return applyRenderDataToNode(layoutNode);
        });
      });
      setEdges(layoutedEdges.map(applyRenderDataToEdge));
    }

    // Update tracked collapse states
    const newCollapseStates = new Map<string, boolean>();
    for (const node of layoutedNodes) {
      if (isTableNodeData(node.data)) {
        newCollapseStates.set(node.id, node.data.isCollapsed ?? false);
      }
    }
    lastAppliedCollapseStates.current = newCollapseStates;
    if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 2: apply layout positions');
  }, [layoutedNodes, layoutedEdges, renderNodeDataById, renderEdgeById, setNodes, setEdges]);

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

      // 2. Try to get lineage info for table/column nodes
      const lineageNode = lineageNodeMapRef.current.get(node.id);
      if (lineageNode) {
        if (lineageNode.span) {
          actions.highlightSpan(lineageNode.span);
          span = lineageNode.span;
        }
        onNodeClick?.(lineageNode);

        if (!sourceName && lineageNode.metadata && typeof lineageNode.metadata.sourceName === 'string') {
          sourceName = lineageNode.metadata.sourceName;
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
    [actions, onNodeClick]
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

  if (!result || !result.statements || result.statements.length === 0) {
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
      </ReactFlow>
    </div>
  );
}
