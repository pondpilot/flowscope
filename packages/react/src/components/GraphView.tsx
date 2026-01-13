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

import { useLineage } from '../store';
import { useNodeFocus } from '../hooks/useNodeFocus';
import { useGraphFiltering } from '../hooks/useGraphFiltering';
import type { GraphViewProps, TableNodeData, LayoutAlgorithm } from '../types';
import { getLayoutedElements, getLayoutedElementsAsync } from '../utils/layout';
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
}: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedNodeId, searchTerm, viewMode, layoutAlgorithm, collapsedNodeIds, defaultCollapsed, showColumnEdges, showScriptTables, expandedTableIds, tableFilter } = state;

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
    if (!result || !result.statements) return null;
    return mergeStatements(result.statements);
  }, [result]);

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
  const { builtGraph, direction } = useMemo(() => {
    if (!result || !result.statements) {
      return { builtGraph: { nodes: [], edges: [] }, direction: 'LR' as const };
    }

    try {
      if (viewMode === 'script') {
        const graph = buildScriptLevelGraph(
          result.statements,
          selectedNodeId,
          effectiveSearchTerm,
          showScriptTables
        );
        return { builtGraph: graph, direction: 'LR' as const };
      }

      // Table view (with optional column-level edges)
      if (!statement) {
        return { builtGraph: { nodes: [], edges: [] }, direction: 'LR' as const };
      }
      const graph = {
        nodes: buildFlowNodes(
          statement,
          selectedNodeId,
          effectiveSearchTerm,
          collapsedNodeIds,
          expandedTableIds,
          result.resolvedSchema,
          defaultCollapsed
        ),
        edges: buildFlowEdges(statement, showColumnEdges, defaultCollapsed, collapsedNodeIds),
      };
      return { builtGraph: graph, direction: 'LR' as const };
    } catch (error) {
      console.error('Graph building failed:', error);
      return { builtGraph: { nodes: [], edges: [] }, direction: 'LR' as const };
    }
  }, [result, statement, selectedNodeId, effectiveSearchTerm, viewMode, collapsedNodeIds, defaultCollapsed, showColumnEdges, showScriptTables, expandedTableIds]);

  // Apply filtering (focus mode, table filter) and compute highlights
  const { filteredGraph, highlightIds } = useGraphFiltering({
    graph: builtGraph,
    selectedNodeId,
    searchTerm: effectiveSearchTerm,
    viewMode,
    showColumnEdges,
    focusMode,
    tableFilter,
  });

  // Enhance graph with highlight styling
  const { rawNodes, rawEdges } = useMemo(() => {
    const enhanced = enhanceGraphWithHighlights(filteredGraph, highlightIds);
    return { rawNodes: enhanced.nodes, rawEdges: enhanced.edges };
  }, [filteredGraph, highlightIds]);

  // State for async layout results
  const [layoutedNodes, setLayoutedNodes] = useState<FlowNode[]>([]);
  const [layoutedEdges, setLayoutedEdges] = useState<FlowEdge[]>([]);

  // Apply layout (sync for dagre, async for elk)
  useEffect(() => {
    if (rawNodes.length === 0) {
      setLayoutedNodes([]);
      setLayoutedEdges([]);
      return;
    }

    let cancelled = false;

    if (layoutAlgorithm === 'elk') {
      getLayoutedElementsAsync(rawNodes, rawEdges, direction, 'elk')
        .then(({ nodes, edges }) => {
          if (!cancelled) {
            setLayoutedNodes(nodes);
            setLayoutedEdges(edges);
          }
        })
        .catch((error) => {
          console.error('ELK layout failed, falling back to dagre:', error);
          if (!cancelled) {
            const { nodes, edges } = getLayoutedElements(rawNodes, rawEdges, direction, 'dagre');
            setLayoutedNodes(nodes);
            setLayoutedEdges(edges);
          }
        });
    } else {
      const { nodes, edges } = getLayoutedElements(rawNodes, rawEdges, direction, 'dagre');
      setLayoutedNodes(nodes);
      setLayoutedEdges(edges);
    }

    return () => {
      cancelled = true;
    };
  }, [rawNodes, rawEdges, direction, layoutAlgorithm]);

  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([]);

  const isInitialized = useRef(false);
  const lastResultId = useRef<string | null>(null);
  const lastViewMode = useRef<string | null>(null);
  const lastShowTables = useRef<boolean | null>(null);
  const lastLayoutAlgorithm = useRef<LayoutAlgorithm | null>(null);

  useEffect(() => {
    const currentResultId = result ? JSON.stringify(result.summary) : null;

    const needsUpdate =
      !isInitialized.current ||
      currentResultId !== lastResultId.current ||
      viewMode !== lastViewMode.current ||
      showScriptTables !== lastShowTables.current ||
      layoutAlgorithm !== lastLayoutAlgorithm.current;

    if (needsUpdate && layoutedNodes.length > 0) {
      setNodes(layoutedNodes);
      setEdges(layoutedEdges);
      isInitialized.current = true;
      lastResultId.current = currentResultId;
      lastViewMode.current = viewMode;
      lastShowTables.current = showScriptTables;
      lastLayoutAlgorithm.current = layoutAlgorithm;
    } else if (layoutedNodes.length > 0) {
      setNodes((currentNodes) => {
        return layoutedNodes.map((layoutNode) => {
          const currentNode = currentNodes.find((n) => n.id === layoutNode.id);
          if (currentNode) {
            return {
              ...layoutNode,
              position: currentNode.position,
            };
          }
          return layoutNode;
        });
      });
      setEdges(layoutedEdges);
    }
  }, [
    layoutedNodes,
    layoutedEdges,
    setNodes,
    setEdges,
    result,
    viewMode,
    layoutAlgorithm,
    collapsedNodeIds,
    showScriptTables,
  ]);


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
      </ReactFlow>
    </div>
  );
}
