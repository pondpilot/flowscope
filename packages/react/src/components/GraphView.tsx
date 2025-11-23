import { useMemo, useCallback, useEffect, useRef, useState } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  Panel,
} from '@xyflow/react';
import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { Search, Network, LayoutList } from 'lucide-react';

import { useLineage } from '../store';
import type { GraphViewProps, TableNodeData } from '../types';
import { getLayoutedElements } from '../utils/layout';
import { findConnectedElements } from '../utils/graphTraversal';
import {
  mergeStatements,
  buildFlowNodes,
  buildFlowEdges,
  buildScriptLevelGraph,
  buildColumnLevelGraph,
} from '../utils/graphBuilders';
import { ScriptNode } from './ScriptNode';
import { ColumnNode } from './ColumnNode';
import { SimpleTableNode } from './SimpleTableNode';
import { TableNode } from './TableNode';
import { AnimatedEdge } from './AnimatedEdge';
import { Input } from './ui/input';
import { ExportMenu } from './ExportMenu';
import { ViewModeSelector } from './ViewModeSelector';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';
import { UI_CONSTANTS, GRAPH_CONFIG } from '../constants';

// Type guard for safer type checking
function isTableNodeData(data: unknown): data is TableNodeData {
  return (
    typeof data === 'object' &&
    data !== null &&
    'label' in data &&
    'nodeType' in data &&
    'columns' in data
  );
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

export function GraphView({ className, onNodeClick, graphContainerRef }: GraphViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedNodeId, searchTerm, viewMode, collapsedNodeIds, showScriptTables } = state;

  // Local search state with debouncing
  const [localSearchTerm, setLocalSearchTerm] = useState(searchTerm);

  // Debounce search term updates
  useEffect(() => {
    const handler = setTimeout(() => {
      actions.setSearchTerm(localSearchTerm);
    }, UI_CONSTANTS.SEARCH_DEBOUNCE_DELAY);

    return () => clearTimeout(handler);
  }, [localSearchTerm, actions]);

  // Sync local state when external searchTerm changes
  useEffect(() => {
    setLocalSearchTerm(searchTerm);
  }, [searchTerm]);

  const statement = useMemo(() => {
    if (!result || !result.statements) return null;
    return mergeStatements(result.statements);
  }, [result]);

  const { layoutedNodes, layoutedEdges } = useMemo(() => {
    if (!result || !result.statements) return { layoutedNodes: [], layoutedEdges: [] };

    try {
      let rawNodes: FlowNode[];
      let rawEdges: FlowEdge[];
      let direction: 'LR' | 'TB' = 'LR';

      if (viewMode === 'script') {
        const tempGraph = buildScriptLevelGraph(
          result.statements,
          selectedNodeId,
          searchTerm,
          showScriptTables
        );

        let highlightIds = new Set<string>();
        if (selectedNodeId) {
          highlightIds = findConnectedElements(selectedNodeId, tempGraph.edges);
        }

        const enhancedGraph = enhanceGraphWithHighlights(tempGraph, highlightIds);
        rawNodes = enhancedGraph.nodes;
        rawEdges = enhancedGraph.edges;
        direction = 'LR';
      } else if (viewMode === 'column') {
        if (!statement) return { layoutedNodes: [], layoutedEdges: [] };

        const tempGraph = buildColumnLevelGraph(statement, selectedNodeId, searchTerm, new Set());

        let highlightIds = new Set<string>();
        if (selectedNodeId) {
          highlightIds = findConnectedElements(selectedNodeId, tempGraph.edges);
        }

        const graph = buildColumnLevelGraph(
          statement,
          selectedNodeId,
          searchTerm,
          collapsedNodeIds
        );
        const enhancedGraph = enhanceGraphWithHighlights(graph, highlightIds);
        rawNodes = enhancedGraph.nodes;
        rawEdges = enhancedGraph.edges;
        direction = 'LR';
      } else {
        if (!statement) return { layoutedNodes: [], layoutedEdges: [] };
        rawNodes = buildFlowNodes(statement, selectedNodeId, searchTerm, collapsedNodeIds);
        rawEdges = buildFlowEdges(statement);
        direction = 'LR';
      }

      const { nodes: ln, edges: le } = getLayoutedElements(rawNodes, rawEdges, direction);
      return { layoutedNodes: ln, layoutedEdges: le };
    } catch (error) {
      console.error('Graph building failed:', error);
      return { layoutedNodes: [], layoutedEdges: [] };
    }
  }, [result, statement, selectedNodeId, searchTerm, viewMode, collapsedNodeIds, showScriptTables]);

  const [nodes, setNodes, onNodesChange] = useNodesState<FlowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<FlowEdge>([]);

  const isInitialized = useRef(false);
  const lastResultId = useRef<string | null>(null);
  const lastViewMode = useRef<string | null>(null);
  const lastShowTables = useRef<boolean | null>(null);

  useEffect(() => {
    const currentResultId = result ? JSON.stringify(result.summary) : null;

    const needsUpdate =
      !isInitialized.current ||
      currentResultId !== lastResultId.current ||
      viewMode !== lastViewMode.current ||
      showScriptTables !== lastShowTables.current;

    if (needsUpdate && layoutedNodes.length > 0) {
      setNodes(layoutedNodes);
      setEdges(layoutedEdges);
      isInitialized.current = true;
      lastResultId.current = currentResultId;
      lastViewMode.current = viewMode;
      lastShowTables.current = showScriptTables;
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
    collapsedNodeIds,
    showScriptTables,
  ]);

  const handleRearrange = useCallback(() => {
    setNodes(layoutedNodes);
    setEdges(layoutedEdges);
  }, [layoutedNodes, layoutedEdges, setNodes, setEdges]);

  const internalGraphRef = useRef<HTMLDivElement>(null);
  const finalRef = graphContainerRef || internalGraphRef;

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: FlowNode) => {
      actions.selectNode(node.id);

      let sourceName: string | undefined;
      let span: any | undefined;

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
        let targetType: 'table' | 'cte' | 'column' | 'script' | undefined;
        const flowNodeType = node.type;

        if (flowNodeType === 'scriptNode') {
          targetType = 'script';
        } else if (flowNodeType === 'columnNode') {
          targetType = 'column';
        } else if (flowNodeType === 'tableNode' || flowNodeType === 'simpleTableNode') {
          const data = node.data as TableNodeData;
          if (data.nodeType === 'cte') targetType = 'cte';
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
        fitView
        minZoom={0.1}
        maxZoom={2}
      >
        <Background />
        <Controls />
        <Panel position="top-left" className="flex gap-3">
          <div className="flex items-center gap-3 rounded-lg border border-slate-200/60 bg-white px-1 py-1 shadow-sm backdrop-blur-sm">
            <ViewModeSelector />

            {viewMode === 'script' && (
              <>
                <div className="h-5 w-px bg-slate-300" />
                <GraphTooltipProvider>
                  <GraphTooltip delayDuration={300}>
                    <GraphTooltipTrigger asChild>
                      <button
                        onClick={actions.toggleShowScriptTables}
                        className={`
                          flex h-8 w-8 shrink-0 items-center justify-center rounded transition-colors
                          ${showScriptTables ? 'bg-slate-200 text-slate-900' : 'text-slate-500 hover:bg-slate-50 hover:text-slate-900'}
                          focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40
                        `}
                        aria-label="Toggle table details"
                        aria-pressed={showScriptTables}
                      >
                        <LayoutList className="h-4 w-4" strokeWidth={showScriptTables ? 2.5 : 1.5} />
                      </button>
                    </GraphTooltipTrigger>
                    <GraphTooltipPortal>
                      <GraphTooltipContent side="bottom">
                        <p>{showScriptTables ? 'Hide tables' : 'Show tables'}</p>
                        <GraphTooltipArrow />
                      </GraphTooltipContent>
                    </GraphTooltipPortal>
                  </GraphTooltip>
                </GraphTooltipProvider>
              </>
            )}
          </div>
          <div
            className="relative flex items-center rounded-lg border border-slate-200/60 bg-white px-2 py-1 shadow-sm backdrop-blur-sm"
            style={{ minWidth: UI_CONSTANTS.SEARCH_MIN_WIDTH }}
          >
            <Search
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-slate-400"
              strokeWidth={1.5}
            />
            <Input
              placeholder="Search..."
              value={localSearchTerm}
              onChange={(e) => setLocalSearchTerm(e.target.value)}
              className="h-8 border-0 bg-transparent pl-8 pr-2 text-sm shadow-none placeholder:text-slate-400 focus-visible:ring-0"
            />
          </div>
        </Panel>
        <Panel position="top-right" className="flex gap-3">
          <div className="flex items-center rounded-lg border border-slate-200/60 bg-white px-1 py-1 shadow-sm backdrop-blur-sm">
            <GraphTooltipProvider>
              <GraphTooltip delayDuration={300}>
                <GraphTooltipTrigger asChild>
                  <button
                    onClick={handleRearrange}
                    className="flex h-8 w-8 shrink-0 items-center justify-center rounded text-slate-500 transition-colors hover:bg-slate-50 hover:text-slate-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
                    aria-label="Rearrange graph layout"
                  >
                    <Network className="h-4 w-4" strokeWidth={1.5} />
                  </button>
                </GraphTooltipTrigger>
                <GraphTooltipPortal>
                  <GraphTooltipContent side="bottom">
                    <p>Rearrange layout</p>
                    <GraphTooltipArrow />
                  </GraphTooltipContent>
                </GraphTooltipPortal>
              </GraphTooltip>
            </GraphTooltipProvider>
          </div>
          <div className="flex items-center rounded-lg border border-slate-200/60 bg-white px-1 py-1 shadow-sm backdrop-blur-sm">
            <ExportMenu graphRef={finalRef} />
          </div>
        </Panel>
        <MiniMap
          nodeColor={(node) => {
            if (isTableNodeData(node.data)) {
              return node.data.nodeType === 'cte' ? '#a855f7' : '#3b82f6';
            }
            return '#3b82f6';
          }}
        />
      </ReactFlow>
    </div>
  );
}
