import { useMemo, useCallback, useEffect, useRef, useState, type JSX } from 'react';
import { ReactFlow, useNodesState, useEdgesState } from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import type { AnalyzeResult, Node as LineageNode } from '@pondpilot/flowscope-core';

import { useLineage, useLineageStore } from '../store';
import { useGraphFiltering } from '../hooks/useGraphFiltering';
import { useOccurrenceShortcuts } from '../hooks/useOccurrenceShortcuts';
import type { GraphViewProps, TableNodeData, LayoutAlgorithm } from '../types';
import {
  getLayoutedElements,
  getLayoutedElementsInWorker,
  cancelLayoutRequests,
} from '../utils/layout';
import { GRAPH_DEBUG, nowMs } from '../utils/debug';
import {
  buildTableGraphInWorker,
  buildScriptGraphInWorker,
  cancelPendingBuilds,
} from '../utils/graphBuilderWorkerService';
import {
  getBodySpanForSourceName,
  getOccurrenceSourceName,
  getOccurrenceSpan,
} from '../utils/nodeOccurrences';
import { ScriptNode } from './ScriptNode';
import { ColumnNode } from './ColumnNode';
import { SimpleTableNode } from './SimpleTableNode';
import { TableNode } from './TableNode';
import { AnimatedEdge } from './AnimatedEdge';
import {
  FitViewHandler,
  NodeFocusHandler,
  RevealHandler,
  ViewportHandler,
} from './GraphViewHandlers';
import { GraphViewControls } from './GraphViewControls';
import { REVEAL_PULSE_DURATION_MS } from '../constants';
import {
  applyRenderDataToEdges,
  applyRenderDataToNodes,
  createRenderGraphIndex,
  enhanceGraphWithHighlights,
  getGraphSearchableTypes,
  getNodeCollapseStates,
  hasNodeCollapseChanged,
  prepareProgressiveLayoutNodes,
  shouldShowMiniMap,
} from '../utils/graphViewModel';

const ELK_NODE_LIMIT = 2000;

const nodeTypes = {
  tableNode: TableNode,
  simpleTableNode: SimpleTableNode,
  scriptNode: ScriptNode,
  columnNode: ColumnNode,
};

const edgeTypes = {
  animated: AnimatedEdge,
};

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
  useOccurrenceShortcuts();
  const setLayoutMetrics = useLineageStore((store) => store.setLayoutMetrics);
  const setGraphMetrics = useLineageStore((store) => store.setGraphMetrics);
  const requestNavigation = useLineageStore((store) => store.requestNavigation);
  const setVisibleGraphNodeIds = useLineageStore((store) => store.setVisibleGraphNodeIds);
  const setIsLayouting = useLineageStore((store) => store.setIsLayouting);
  const setIsBuilding = useLineageStore((store) => store.setIsBuilding);
  const revealRequest = useLineageStore((store) => store.revealRequest);
  const {
    result,
    selectedNodeId,
    searchTerm,
    viewMode,
    layoutAlgorithm,
    collapsedNodeIds,
    defaultCollapsed,
    showColumnEdges,
    showScriptTables,
    expandedTableIds,
    tableFilter,
    focusedOccurrenceIndex,
  } = state;
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
  const handleSearchTermChange = useCallback(
    (newSearchTerm: string) => {
      if (isSearchControlled) {
        onSearchTermChange?.(newSearchTerm);
      } else {
        actions.setSearchTerm(newSearchTerm);
      }
    },
    [isSearchControlled, onSearchTermChange, actions]
  );

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
  const searchableTypes = useMemo(
    () => getGraphSearchableTypes(viewMode, showColumnEdges),
    [viewMode, showColumnEdges]
  );

  // State for async graph building results
  const [builtGraph, setBuiltGraph] = useState<{ nodes: FlowNode[]; edges: FlowEdge[] }>({
    nodes: [],
    edges: [],
  });
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
      if (GRAPH_DEBUG)
        console.log(
          `[GraphBuilder #${buildId}] Calling worker (${(workerStartTime - buildStartTime).toFixed(1)}ms since effect start)`
        );

      const buildPromise =
        viewMode === 'script'
          ? buildScriptGraphInWorker({
              result: analysisResult,
              selectedNodeId,
              searchTerm: effectiveSearchTerm,
              showTables: showScriptTables,
            })
          : buildTableGraphInWorker({
              result: analysisResult,
              selectedNodeId,
              searchTerm: effectiveSearchTerm,
              collapsedNodeIds,
              expandedTableIds,
              resolvedSchema: analysisResult.resolvedSchema,
              defaultCollapsed,
              showColumnEdges,
            });

      buildPromise
        .then(({ nodes, edges, lineageNodes }) => {
          const callbackTime = nowMs();
          const totalDuration = callbackTime - buildStartTime;
          const workerRoundtrip = callbackTime - workerStartTime;

          if (GRAPH_DEBUG) {
            console.log(
              `[GraphBuilder #${buildId}] Worker returned: ${nodes.length} nodes, ${edges.length} edges`
            );
            console.log(
              `[GraphBuilder #${buildId}] Worker roundtrip: ${workerRoundtrip.toFixed(1)}ms, Total: ${totalDuration.toFixed(1)}ms`
            );
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
  }, [
    analysisResult,
    selectedNodeId,
    effectiveSearchTerm,
    viewMode,
    collapsedNodeIds,
    defaultCollapsed,
    showColumnEdges,
    showScriptTables,
    expandedTableIds,
    setIsBuilding,
  ]);

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
    setVisibleGraphNodeIds(filteredGraph.nodes.map((node) => node.id));
  }, [filteredGraph.nodes, setVisibleGraphNodeIds]);

  useEffect(() => {
    return () => {
      setVisibleGraphNodeIds([]);
    };
  }, [setVisibleGraphNodeIds]);

  useEffect(() => {
    renderGraphRef.current = renderGraph;
  }, [renderGraph]);

  const layoutNodes = filteredGraph.nodes;
  const layoutEdges = filteredGraph.edges;

  const renderGraphIndex = useMemo(() => createRenderGraphIndex(renderGraph), [renderGraph]);
  const showMiniMap = shouldShowMiniMap(renderGraph.nodes.length);

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
  }, [
    analysisResult,
    buildDurationMs,
    builtGraph.nodes.length,
    builtGraph.edges.length,
    setGraphMetrics,
  ]);

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

    // Capture renderGraph snapshot for this layout cycle. Using a ref ensures we get
    // a consistent snapshot even if renderGraph updates during async layout computation.
    // This prevents race conditions where node counts/IDs change mid-computation.
    const renderGraphSnapshot = renderGraphRef.current;

    if (GRAPH_DEBUG) console.time('[Layout] Stage 1: preserve positions');
    // Stage 1: Preserve existing node positions for smoother transitions.
    // This prevents nodes from jumping to origin (0,0) while layout computes.
    setNodes((currentNodes) =>
      prepareProgressiveLayoutNodes(currentNodes, renderGraphSnapshot.nodes, direction)
    );
    setEdges(renderGraphSnapshot.edges);
    if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 1: preserve positions');

    if (GRAPH_DEBUG) {
      console.log(
        '[Layout] Starting worker layout for',
        layoutNodes.length,
        'nodes,',
        layoutEdges.length,
        'edges'
      );
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
            const durationMs =
              layoutStartRef.current !== null ? nowMs() - layoutStartRef.current : null;
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
            const { nodes, edges } = getLayoutedElements(
              layoutNodes,
              layoutEdges,
              direction,
              'dagre'
            );
            if (GRAPH_DEBUG) console.timeEnd('[Layout] Fallback sync layout');
            setLayoutedNodes(nodes);
            setLayoutedEdges(edges);
            const durationMs =
              layoutStartRef.current !== null ? nowMs() - layoutStartRef.current : null;
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
  }, [
    layoutNodes,
    layoutEdges,
    direction,
    layoutAlgorithm,
    defaultCollapsed,
    showScriptTables,
    viewMode,
    analysisResult,
    setNodes,
    setEdges,
    setLayoutMetrics,
    setIsLayouting,
  ]);

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

    const hasRenderData = layoutedNodes.every((node) => renderGraphIndex.nodeDataById.has(node.id));
    if (!hasRenderData) {
      if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 2: apply layout positions');
      return;
    }

    // Note: The layoutIsStale check was removed because it's incompatible with
    // async Web Worker layout. With async layout, layoutedNodes always reflects
    // the collapsed state at the time layout was computed, and we should render
    // that state rather than blocking until a new layout completes.

    const currentResultId = layoutSnapshot.resultSummary
      ? JSON.stringify(layoutSnapshot.resultSummary)
      : null;
    const defaultCollapseChanged =
      layoutSnapshot.defaultCollapsed !== lastAppliedDefaultCollapsed.current;

    // Check if any individual node's collapse state changed (affects node height/layout)
    const nodeCollapseChanged = hasNodeCollapseChanged(
      layoutedNodes,
      lastAppliedCollapseStates.current
    );

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
      setNodes(applyRenderDataToNodes(layoutedNodes, renderGraphIndex.nodeDataById));
      setEdges(applyRenderDataToEdges(layoutedEdges, renderGraphIndex.edgeById));
      isInitialized.current = true;
      lastResultId.current = currentResultId;
      lastViewMode.current = layoutSnapshot.viewMode;
      lastShowTables.current = layoutSnapshot.showScriptTables;
      lastLayoutAlgorithm.current = layoutSnapshot.layoutAlgorithm;
      lastAppliedDefaultCollapsed.current = layoutSnapshot.defaultCollapsed;
    } else {
      // Preserve user-adjusted positions while updating node data
      setNodes((currentNodes) =>
        applyRenderDataToNodes(layoutedNodes, renderGraphIndex.nodeDataById, currentNodes)
      );
      setEdges(applyRenderDataToEdges(layoutedEdges, renderGraphIndex.edgeById));
    }

    // Update tracked collapse states
    lastAppliedCollapseStates.current = getNodeCollapseStates(layoutedNodes);
    if (GRAPH_DEBUG) console.timeEnd('[Layout] Stage 2: apply layout positions');
  }, [layoutedNodes, layoutedEdges, renderGraphIndex, setNodes, setEdges]);

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
        // Prefer the first occurrence from `nameSpans` (per-occurrence list
        // shipped in #20). Fall back to the legacy `span` for column nodes
        // and any future node type that doesn't yet populate `nameSpans`.
        const targetSpan = getOccurrenceSpan(lineageNode, 0) ?? lineageNode.span;
        if (targetSpan) {
          actions.highlightSpan(targetSpan);
          span = targetSpan;
        }
        onNodeClick?.(lineageNode);

        if (!sourceName) {
          sourceName =
            getOccurrenceSourceName(lineageNode, 0) ??
            (lineageNode.metadata && typeof lineageNode.metadata.sourceName === 'string'
              ? lineageNode.metadata.sourceName
              : undefined);
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

  const handleNodeDoubleClick = useCallback(
    (_event: React.MouseEvent, node: FlowNode) => {
      // Double-click jumps to the CTE body for CTE nodes; for other node
      // types the single-click handler already focused the first occurrence,
      // so we have no extra work to do.
      const lineageNode = lineageNodeMapRef.current.get(node.id);
      const sourceName = lineageNode
        ? (getOccurrenceSourceName(lineageNode, focusedOccurrenceIndex) ??
          (typeof lineageNode.metadata?.sourceName === 'string'
            ? lineageNode.metadata.sourceName
            : undefined))
        : undefined;
      const bodySpan = lineageNode ? getBodySpanForSourceName(lineageNode, sourceName) : undefined;
      if (lineageNode && bodySpan) {
        if (selectedNodeId !== node.id) {
          actions.selectNode(node.id);
        }
        actions.highlightSpan(bodySpan);
        if (sourceName) {
          actions.requestNavigation({
            sourceName,
            span: bodySpan,
            targetName: lineageNode.label,
            targetType: 'cte',
          });
        }
      }
    },
    [actions, focusedOccurrenceIndex, selectedNodeId]
  );

  // Track the latest reveal request via a ref so the bounce effect can peek at
  // it without re-firing when `clearRevealRequest` lands ~100ms after a reveal
  // (which would otherwise navigate to the reveal target and defeat the
  // suppression). `consumedRevealSuppressionNonceRef` guarantees each reveal
  // suppresses at most one bounce, so repeat reveals of the same node still
  // work even though the effect only keys on selection changes.
  const revealRequestRef = useRef(revealRequest);
  revealRequestRef.current = revealRequest;
  const consumedRevealSuppressionNonceRef = useRef<number | null>(null);

  useEffect(() => {
    if (selectedNodeId === null) {
      return;
    }

    const currentRevealRequest = revealRequestRef.current;
    if (
      currentRevealRequest &&
      currentRevealRequest.suppressNavigation &&
      currentRevealRequest.nodeId === selectedNodeId &&
      currentRevealRequest.nonce !== consumedRevealSuppressionNonceRef.current
    ) {
      consumedRevealSuppressionNonceRef.current = currentRevealRequest.nonce;
      return;
    }

    const lineageNode = lineageNodeMapRef.current.get(selectedNodeId);
    if (!lineageNode) {
      return;
    }

    const span = getOccurrenceSpan(lineageNode, focusedOccurrenceIndex) ?? lineageNode.span;
    const sourceName =
      getOccurrenceSourceName(lineageNode, focusedOccurrenceIndex) ??
      (typeof lineageNode.metadata?.sourceName === 'string'
        ? lineageNode.metadata.sourceName
        : undefined);
    const targetType =
      lineageNode.type === 'table' ||
      lineageNode.type === 'view' ||
      lineageNode.type === 'cte' ||
      lineageNode.type === 'column'
        ? lineageNode.type
        : undefined;

    if (sourceName && span) {
      requestNavigation({
        sourceName,
        span,
        targetName: lineageNode.label,
        targetType,
      });
    }
  }, [requestNavigation, focusedOccurrenceIndex, selectedNodeId]);

  const handlePaneClick = useCallback(() => {
    actions.selectNode(null);
  }, [actions]);

  // Apply the reveal pulse class to a node for REVEAL_PULSE_DURATION_MS, then
  // strip it. Used by RevealHandler in response to text→graph navigation.
  const pulseTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  const applyRevealPulse = useCallback(
    (nodeId: string) => {
      const existingTimer = pulseTimersRef.current.get(nodeId);
      if (existingTimer) clearTimeout(existingTimer);

      setNodes((current) =>
        current.map((n) =>
          n.id === nodeId
            ? {
                ...n,
                className: [n.className, 'flowscope-reveal-pulse'].filter(Boolean).join(' '),
              }
            : n
        )
      );

      const timer = setTimeout(() => {
        setNodes((current) =>
          current.map((n) => {
            if (n.id !== nodeId || !n.className) return n;
            const next = n.className
              .split(/\s+/)
              .filter((c) => c && c !== 'flowscope-reveal-pulse')
              .join(' ');
            return { ...n, className: next || undefined };
          })
        );
        pulseTimersRef.current.delete(nodeId);
      }, REVEAL_PULSE_DURATION_MS);
      pulseTimersRef.current.set(nodeId, timer);
    },
    [setNodes]
  );

  useEffect(() => {
    // Drop timers whose target node no longer exists (e.g. the filter changed
    // mid-animation). The pulse class only attaches to rendered nodes, so a
    // stale timer would just be a leaked handle.
    const timers = pulseTimersRef.current;
    if (timers.size === 0) return;
    const liveIds = new Set(nodes.map((node) => node.id));
    for (const [nodeId, timer] of timers) {
      if (!liveIds.has(nodeId)) {
        clearTimeout(timer);
        timers.delete(nodeId);
      }
    }
  }, [nodes]);

  useEffect(() => {
    const timers = pulseTimersRef.current;
    return () => {
      for (const timer of timers.values()) clearTimeout(timer);
      timers.clear();
    };
  }, []);

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
        onNodeDoubleClick={handleNodeDoubleClick}
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
        <RevealHandler applyPulse={applyRevealPulse} />
        <ViewportHandler initialViewport={initialViewport} onViewportChange={onViewportChange} />
        <FitViewHandler trigger={fitViewTrigger} />
        <GraphViewControls
          viewMode={viewMode}
          showScriptTables={showScriptTables}
          defaultCollapsed={defaultCollapsed}
          showColumnEdges={showColumnEdges}
          searchTerm={effectiveSearchTerm ?? ''}
          searchableTypes={searchableTypes}
          focusMode={focusMode}
          showMiniMap={showMiniMap}
          onSearchTermChange={handleSearchTermChange}
          onFocusModeChange={handleFocusModeChange}
          onToggleShowScriptTables={actions.toggleShowScriptTables}
          onSetAllNodesCollapsed={actions.setAllNodesCollapsed}
          onToggleColumnEdges={actions.toggleColumnEdges}
        />
      </ReactFlow>
    </div>
  );
}
