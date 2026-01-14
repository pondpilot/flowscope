import { useMemo, useCallback, useRef, useEffect, useState, createContext, useContext, forwardRef, useImperativeHandle } from 'react';
import {
  ChevronRight,
  ChevronDown,
  Table as TableIcon,
  LayoutList,
  FileCode,
  PackageOpen,
  Columns3,
  FileText,
  ArrowRight,
  GripHorizontal,
  Eye,
  Grid3X3,
  ExternalLink,
} from 'lucide-react';
import { useLineage, SearchAutocomplete, type SearchSuggestion, type SearchAutocompleteRef } from '@pondpilot/flowscope-react';
import { cn } from '@/lib/utils';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useNavigation } from '@/lib/navigation-context';
import { usePersistedHierarchyState } from '@/hooks/usePersistedHierarchyState';

// Context for hierarchy view actions to reduce prop drilling
interface HierarchyActionsContextValue {
  getNodeDetails: (id: string) => NodeDetails | null;
  onNavigateToLineage: (nodeId: string) => void;
  onNavigateToSchema: (tableName: string) => void;
  onNavigateToEditor: (scripts: string[]) => void;
}

const HierarchyActionsContext = createContext<HierarchyActionsContextValue | null>(null);

function useHierarchyActions() {
  const context = useContext(HierarchyActionsContext);
  if (!context) {
    throw new Error('useHierarchyActions must be used within HierarchyView');
  }
  return context;
}

interface LineageNode {
  id: string;
  label: string;
  type: string;
  upstream: LineageNode[];
  matchesFilter: boolean;
  hasMatchingDescendant: boolean;
}

interface NodeDetails {
  id: string;
  label: string;
  type: string;
  columns: string[];
  scripts: string[];
  upstreamMappings: Array<{ fromTable: string; fromCol: string; toCol: string }>;
  downstreamMappings: Array<{ toTable: string; fromCol: string; toCol: string }>;
}

interface FlatNode {
  id: string;
  nodeKey: string; // Unique key for this position in tree (includes path)
  node: LineageNode;
  depth: number;
  isUnused?: boolean;
}

interface HierarchyViewProps {
  className?: string;
  projectId: string | null;
}

/** Ref handle for HierarchyView - exposes focus methods */
export interface HierarchyViewRef {
  /** Focus the search input */
  focusSearch: () => void;
}

export const HierarchyView = forwardRef<HierarchyViewRef, HierarchyViewProps>(function HierarchyView({ className, projectId }, ref) {
  const { state, actions } = useLineage();
  const { result } = state;
  const { navigateTo, navigateToEditor } = useNavigation();
  const nodes = result?.globalLineage?.nodes || [];
  const edges = result?.globalLineage?.edges || [];
  const statements = result?.statements || [];

  // Build a lookup map for O(1) node access instead of O(n) find() calls
  const nodesById = useMemo(() => {
    const map = new Map<string, (typeof nodes)[number]>();
    for (const node of nodes) {
      map.set(node.id, node);
    }
    return map;
  }, [nodes]);

  const getNode = useCallback((id: string) => nodesById.get(id), [nodesById]);

  // Use persisted state hook for all view state
  const {
    expandedNodes,
    filter,
    detailsPanelHeight,
    focusedNodeKey,
    unusedExpanded,
    setExpandedNodes,
    setFilter,
    setDetailsPanelHeight,
    setFocusedNodeKey,
    setUnusedExpanded,
    toggleNode,
  } = usePersistedHierarchyState(projectId);

  // State for resize tracking (visual feedback) and ref (drag logic)
  const [isResizing, setIsResizing] = useState(false);
  const isResizingRef = useRef(false);

  const treeContainerRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<SearchAutocompleteRef>(null);

  // Focus tree when ArrowDown pressed in search with no suggestions
  const handleSearchArrowDownExit = useCallback(() => {
    treeContainerRef.current?.focus();
  }, []);

  /** Focus the search input - exposed for keyboard shortcuts */
  const focusSearch = useCallback(() => {
    searchRef.current?.focus();
  }, []);

  // Expose focus methods via ref
  useImperativeHandle(ref, () => ({
    focusSearch,
  }), [focusSearch]);

  // Navigation handlers
  const handleNavigateToLineage = useCallback((nodeId: string) => {
    navigateTo('lineage', { tableId: nodeId });
  }, [navigateTo]);

  const handleNavigateToSchema = useCallback((tableName: string) => {
    navigateTo('schema', { tableName });
  }, [navigateTo]);

  const handleNavigateToEditor = useCallback((scripts: string[]) => {
    if (scripts.length > 0) {
      // Navigate to the first script
      navigateToEditor(scripts[0]);
    }
  }, [navigateToEditor]);

  // Build lookup maps for details
  const { columnsByTable, scriptsByTable } = useMemo(() => {
    const colMap = new Map<string, string[]>();
    const scriptMap = new Map<string, string[]>();

    edges.forEach((e) => {
      if (e.type === 'ownership') {
        const parentNode = getNode(e.from);
        const childNode = getNode(e.to);
        if (parentNode && childNode && childNode.type === 'column') {
          const cols = colMap.get(e.from) || [];
          const colName = childNode.label.includes('.')
            ? childNode.label.split('.').pop() || childNode.label
            : childNode.label;
          if (!cols.includes(colName)) {
            cols.push(colName);
          }
          colMap.set(e.from, cols);
        }
      }
    });

    nodes.forEach((node) => {
      if (['table', 'view', 'cte'].includes(node.type) && node.statementRefs) {
        const scripts: string[] = [];
        node.statementRefs.forEach((ref) => {
          const stmt = statements[ref.statementIndex];
          if (stmt?.sourceName && !scripts.includes(stmt.sourceName)) {
            scripts.push(stmt.sourceName);
          }
        });
        if (scripts.length > 0) {
          scriptMap.set(node.id, scripts);
        }
      }
    });

    return { columnsByTable: colMap, scriptsByTable: scriptMap };
  }, [getNode, nodes, edges, statements]);

  // Pre-compute all table mappings for efficient lookup
  const mappingsByTable = useMemo(() => {
    const upstreamMap = new Map<string, Array<{ fromTable: string; fromCol: string; toCol: string }>>();
    const downstreamMap = new Map<string, Array<{ toTable: string; fromCol: string; toCol: string }>>();

    // Build ownership lookup: column ID -> table ID
    const columnToTable = new Map<string, string>();
    edges.forEach((e) => {
      if (e.type === 'ownership') {
        const childNode = getNode(e.to);
        if (childNode?.type === 'column') {
          columnToTable.set(e.to, e.from);
        }
      }
    });

    // Process data flow edges to build mappings
    edges.forEach((e) => {
      if (e.type === 'data_flow') {
        const fromNode = getNode(e.from);
        const toNode = getNode(e.to);

        if (fromNode?.type === 'column' && toNode?.type === 'column') {
          const fromTableId = columnToTable.get(e.from);
          const toTableId = columnToTable.get(e.to);

          if (fromTableId && toTableId) {
            const fromTableNode = getNode(fromTableId);
            const toTableNode = getNode(toTableId);

            if (fromTableNode && toTableNode) {
              const fromColName = fromNode.label.includes('.')
                ? fromNode.label.split('.').pop()!
                : fromNode.label;
              const toColName = toNode.label.includes('.')
                ? toNode.label.split('.').pop()!
                : toNode.label;

              // Add upstream mapping for the target table
              const upstream = upstreamMap.get(toTableId) || [];
              upstream.push({
                fromTable: fromTableNode.label,
                fromCol: fromColName,
                toCol: toColName,
              });
              upstreamMap.set(toTableId, upstream);

              // Add downstream mapping for the source table
              const downstream = downstreamMap.get(fromTableId) || [];
              downstream.push({
                toTable: toTableNode.label,
                fromCol: fromColName,
                toCol: toColName,
              });
              downstreamMap.set(fromTableId, downstream);
            }
          }
        }
      }
    });

    return { upstreamMap, downstreamMap };
  }, [getNode, edges]);

  const getNodeDetails = useCallback((nodeId: string): NodeDetails | null => {
    const node = getNode(nodeId);
    if (!node) return null;

    const columns = columnsByTable.get(nodeId) || [];
    const scripts = scriptsByTable.get(nodeId) || [];
    const upstreamMappings = mappingsByTable.upstreamMap.get(nodeId) || [];
    const downstreamMappings = mappingsByTable.downstreamMap.get(nodeId) || [];

    return {
      id: nodeId,
      label: node.label,
      type: node.type,
      columns: columns.sort(),
      scripts,
      upstreamMappings,
      downstreamMappings,
    };
  }, [getNode, columnsByTable, scriptsByTable, mappingsByTable]);

  const { sinks, unusedSources } = useMemo(() => {
    const tableNodes = nodes.filter((n) => ['table', 'view', 'cte'].includes(n.type));

    const hasDownstream = new Set<string>();
    const hasUpstream = new Set<string>();

    edges.forEach((e) => {
      const fromNode = getNode(e.from);
      const toNode = getNode(e.to);
      if (
        fromNode &&
        toNode &&
        ['table', 'view', 'cte'].includes(fromNode.type) &&
        ['table', 'view', 'cte'].includes(toNode.type)
      ) {
        hasDownstream.add(e.from);
        hasUpstream.add(e.to);
      }
    });

    const sinkNodes = tableNodes.filter((n) => !hasDownstream.has(n.id));
    const orphanNodes = tableNodes.filter(
      (n) => !hasDownstream.has(n.id) && !hasUpstream.has(n.id)
    );
    const orphanIds = new Set(orphanNodes.map((n) => n.id));
    const realSinks = sinkNodes.filter((n) => !orphanIds.has(n.id));

    return {
      sinks: realSinks,
      unusedSources: orphanNodes,
    };
  }, [getNode, nodes, edges]);

  const buildUpstreamTree = (
    nodeId: string,
    visited: Set<string> = new Set(),
    filterLower: string
  ): LineageNode | null => {
    const nodeData = getNode(nodeId);
    if (!nodeData) return null;
    if (!['table', 'view', 'cte'].includes(nodeData.type)) return null;

    if (visited.has(nodeId)) {
      return null;
    }
    visited.add(nodeId);

    const upstreamEdges = edges.filter((e) => {
      if (e.to !== nodeId) return false;
      const fromNode = getNode(e.from);
      return fromNode && ['table', 'view', 'cte'].includes(fromNode.type);
    });

    const upstream = upstreamEdges
      .map((edge) => buildUpstreamTree(edge.from, new Set(visited), filterLower))
      .filter((n): n is LineageNode => n !== null)
      .sort((a, b) => a.label.localeCompare(b.label));

    const label = nodeData.label || nodeData.id;
    // Check if the table name matches
    const nameMatches = label.toLowerCase().includes(filterLower);
    // Check if any column name matches
    const columns = columnsByTable.get(nodeId) || [];
    const columnMatches = columns.some((col) => col.toLowerCase().includes(filterLower));
    const matchesFilter = filterLower
      ? nameMatches || columnMatches
      : true;
    const hasMatchingDescendant = upstream.some(
      (u) => u.matchesFilter || u.hasMatchingDescendant
    );

    return {
      id: nodeId,
      label,
      type: nodeData.type,
      upstream,
      matchesFilter,
      hasMatchingDescendant,
    };
  };

  const sinkTrees = useMemo(() => {
    const filterLower = filter.toLowerCase().trim();

    const trees = sinks
      .map((sink) => buildUpstreamTree(sink.id, new Set(), filterLower))
      .filter((tree): tree is LineageNode => tree !== null)
      .sort((a, b) => a.label.localeCompare(b.label));

    if (filterLower) {
      return trees.filter(
        (tree) => tree.matchesFilter || tree.hasMatchingDescendant
      );
    }

    return trees;
  }, [sinks, edges, nodes, filter, columnsByTable]);

  const filteredUnused = useMemo(() => {
    const filterLower = filter.toLowerCase().trim();
    if (!filterLower) return unusedSources;

    return unusedSources.filter((n) => {
      const nameMatches = (n.label || n.id).toLowerCase().includes(filterLower);
      const columns = columnsByTable.get(n.id) || [];
      const columnMatches = columns.some((col) => col.toLowerCase().includes(filterLower));
      return nameMatches || columnMatches;
    });
  }, [unusedSources, filter, columnsByTable]);

  // Build flat list of visible nodes for keyboard navigation
  const flatNodeList = useMemo(() => {
    const list: FlatNode[] = [];

    function traverse(node: LineageNode, depth: number, path: string) {
      const nodeKey = `${path}/${node.id}`;
      list.push({ id: node.id, nodeKey, node, depth });
      if (expandedNodes.has(node.id)) {
        node.upstream.forEach((child, idx) => traverse(child, depth + 1, `${nodeKey}:${idx}`));
      }
    }

    sinkTrees.forEach((tree, idx) => traverse(tree, 0, `root:${idx}`));

    if (unusedExpanded && filteredUnused.length > 0) {
      filteredUnused.forEach((n, idx) => {
        list.push({
          id: n.id,
          nodeKey: `unused:${idx}/${n.id}`,
          node: {
            id: n.id,
            label: n.label || n.id,
            type: n.type,
            upstream: [],
            matchesFilter: true,
            hasMatchingDescendant: false,
          },
          depth: 1,
          isUnused: true,
        });
      });
    }

    return list;
  }, [sinkTrees, expandedNodes, unusedExpanded, filteredUnused]);

  // Auto-expand on filter
  useMemo(() => {
    if (!filter.trim()) {
      setExpandedNodes(new Set());
      return;
    }

    const toExpand = new Set<string>();

    function collectExpandable(node: LineageNode) {
      if (node.hasMatchingDescendant) {
        toExpand.add(node.id);
      }
      node.upstream.forEach(collectExpandable);
    }

    sinkTrees.forEach(collectExpandable);
    setExpandedNodes(toExpand);
  }, [filter, sinkTrees]);

  // Handle suggestion selection from autocomplete
  const handleSuggestionSelect = useCallback(
    (suggestion: SearchSuggestion) => {
      setFilter(suggestion.label);
    },
    [setFilter]
  );

  // Get the focused node from the flat list
  const focusedNode = useMemo(() => {
    if (!focusedNodeKey) return null;
    return flatNodeList.find((n) => n.nodeKey === focusedNodeKey) || null;
  }, [focusedNodeKey, flatNodeList]);

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (flatNodeList.length === 0) return;

      const currentIndex = focusedNodeKey
        ? flatNodeList.findIndex((n) => n.nodeKey === focusedNodeKey)
        : -1;

      switch (e.key) {
        case 'ArrowDown': {
          e.preventDefault();
          const nextIndex = currentIndex < flatNodeList.length - 1 ? currentIndex + 1 : 0;
          setFocusedNodeKey(flatNodeList[nextIndex].nodeKey);
          break;
        }
        case 'ArrowUp': {
          e.preventDefault();
          const prevIndex = currentIndex > 0 ? currentIndex - 1 : flatNodeList.length - 1;
          setFocusedNodeKey(flatNodeList[prevIndex].nodeKey);
          break;
        }
        case 'ArrowRight': {
          e.preventDefault();
          if (focusedNode) {
            if (focusedNode.node.upstream.length > 0 && !expandedNodes.has(focusedNode.id)) {
              toggleNode(focusedNode.id);
            }
          }
          break;
        }
        case 'ArrowLeft': {
          e.preventDefault();
          if (focusedNode) {
            if (expandedNodes.has(focusedNode.id)) {
              toggleNode(focusedNode.id);
            } else {
              // Find parent and focus it
              if (currentIndex > 0) {
                const currentDepth = focusedNode.depth;
                for (let i = currentIndex - 1; i >= 0; i--) {
                  if (flatNodeList[i].depth < currentDepth) {
                    setFocusedNodeKey(flatNodeList[i].nodeKey);
                    break;
                  }
                }
              }
            }
          }
          break;
        }
        case 'Enter':
        case ' ': {
          e.preventDefault();
          if (focusedNode) {
            actions.selectNode(focusedNode.id);
          }
          break;
        }
        case 'Home': {
          e.preventDefault();
          if (flatNodeList.length > 0) {
            setFocusedNodeKey(flatNodeList[0].nodeKey);
          }
          break;
        }
        case 'End': {
          e.preventDefault();
          if (flatNodeList.length > 0) {
            setFocusedNodeKey(flatNodeList[flatNodeList.length - 1].nodeKey);
          }
          break;
        }
      }
    },
    [flatNodeList, focusedNodeKey, focusedNode, expandedNodes, toggleNode, actions]
  );

  // Scroll focused node into view
  useEffect(() => {
    if (focusedNodeKey && treeContainerRef.current) {
      const element = treeContainerRef.current.querySelector(
        `[data-node-key="${CSS.escape(focusedNodeKey)}"]`
      );
      if (element) {
        element.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      }
    }
  }, [focusedNodeKey]);

  // Resize handler
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
    isResizingRef.current = true;

    const startY = e.clientY;
    const startHeight = detailsPanelHeight;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaY = startY - e.clientY;
      const newHeight = Math.max(80, Math.min(400, startHeight + deltaY));
      setDetailsPanelHeight(newHeight);
    };

    const handleMouseUp = () => {
      isResizingRef.current = false;
      setIsResizing(false);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  }, [detailsPanelHeight, setDetailsPanelHeight]);

  const hasContent = sinkTrees.length > 0 || filteredUnused.length > 0;

  const selectedDetails = state.selectedNodeId
    ? getNodeDetails(state.selectedNodeId)
    : null;

  // Memoize context value to prevent unnecessary re-renders
  const hierarchyActionsValue = useMemo(() => ({
    getNodeDetails,
    onNavigateToLineage: handleNavigateToLineage,
    onNavigateToSchema: handleNavigateToSchema,
    onNavigateToEditor: handleNavigateToEditor,
  }), [getNodeDetails, handleNavigateToLineage, handleNavigateToSchema, handleNavigateToEditor]);

  return (
    <HierarchyActionsContext.Provider value={hierarchyActionsValue}>
      <TooltipProvider delayDuration={400}>
        <div className={cn('flex flex-col h-full bg-background', className)}>
        {/* Filter Input with Autocomplete */}
        <div className="p-2 border-b">
          <SearchAutocomplete
            ref={searchRef}
            initialValue={filter}
            onSearch={setFilter}
            searchableTypes={['table', 'view', 'cte', 'column']}
            placeholder="Filter tables and columns..."
            onSuggestionSelect={handleSuggestionSelect}
            searchInputId="hierarchy"
            onArrowDownExit={handleSearchArrowDownExit}
            className="w-full"
          />
        </div>

        {/* Tree Content */}
        <div
          ref={treeContainerRef}
          className="flex-1 min-h-0 overflow-auto focus:outline-hidden"
          tabIndex={0}
          onKeyDown={handleKeyDown}
          role="tree"
          aria-label="Lineage tree"
        >
          {!hasContent ? (
            <div className="text-sm text-muted-foreground text-center py-10">
              {filter ? 'No matches found' : 'No lineage data available'}
            </div>
          ) : (
            <div className="py-1 text-xs">
              {sinkTrees.map((tree, idx) => (
                <LineageNodeItem
                  key={`root:${idx}/${tree.id}`}
                  node={tree}
                  nodeKey={`root:${idx}/${tree.id}`}
                  depth={0}
                  expandedNodes={expandedNodes}
                  onToggle={toggleNode}
                  onSelect={(id) => actions.selectNode(id)}
                  activeId={state.selectedNodeId}
                  focusedKey={focusedNodeKey}
                  onFocus={setFocusedNodeKey}
                  filter={filter.toLowerCase().trim()}
                />
              ))}

              {filteredUnused.length > 0 && (
                <div className="mt-2 border-t">
                  <button
                    className={cn(
                      'w-full flex items-center gap-1.5 px-2 py-1.5 text-[11px]',
                      'text-muted-foreground hover:bg-muted/50 transition-colors',
                      'focus:outline-hidden focus-visible:ring-1 focus-visible:ring-ring'
                    )}
                    onClick={() => setUnusedExpanded(!unusedExpanded)}
                  >
                    {unusedExpanded ? (
                      <ChevronDown className="w-3 h-3" />
                    ) : (
                      <ChevronRight className="w-3 h-3" />
                    )}
                    <PackageOpen className="w-3.5 h-3.5" />
                    <span>Unused Sources</span>
                    <span className="ml-auto tabular-nums text-muted-foreground/70">
                      {filteredUnused.length}
                    </span>
                  </button>

                  {unusedExpanded && (
                    <div className="py-1">
                      {filteredUnused.map((node, idx) => {
                        const unusedNodeKey = `unused:${idx}/${node.id}`;
                        const details = getNodeDetails(node.id);
                        return (
                          <Tooltip key={unusedNodeKey}>
                            <TooltipTrigger asChild>
                              <div
                                data-node-key={unusedNodeKey}
                                className={cn(
                                  'group relative flex items-center gap-1.5 py-1 px-2 cursor-pointer transition-colors',
                                  'hover:bg-muted/50',
                                  state.selectedNodeId === node.id && 'bg-muted',
                                  focusedNodeKey === unusedNodeKey && 'ring-1 ring-inset ring-primary/50'
                                )}
                                style={{ paddingLeft: '24px' }}
                                onClick={() => actions.selectNode(node.id)}
                                onMouseEnter={() => setFocusedNodeKey(unusedNodeKey)}
                              >
                                <NodeIcon type={node.type} className="w-3.5 h-3.5 shrink-0" />
                                <span
                                  className={cn(
                                    'truncate flex-1',
                                    filter &&
                                      (node.label || node.id).toLowerCase().includes(filter.toLowerCase()) &&
                                      'bg-yellow-200/50 dark:bg-yellow-500/30'
                                  )}
                                >
                                  {node.label || node.id}
                                </span>
                                {/* Hover action icons */}
                                <div className="flex items-center gap-0.5 ml-auto shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                                  <NodeActionButtons
                                    nodeId={node.id}
                                    nodeName={node.label || node.id}
                                    scripts={details?.scripts}
                                  />
                                </div>
                              </div>
                            </TooltipTrigger>
                            <TooltipContent side="right" className="max-w-xs">
                              <NodeTooltipContent details={details} />
                            </TooltipContent>
                          </Tooltip>
                        );
                      })}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Resizable Details Panel */}
        {selectedDetails && (
          <>
            {/* Resize Handle */}
            <div
              className={cn(
                'h-2 border-t bg-muted/30 cursor-ns-resize flex items-center justify-center',
                'hover:bg-muted/50 transition-colors',
                isResizing && 'bg-muted/50'
              )}
              onMouseDown={handleResizeStart}
            >
              <GripHorizontal className="w-4 h-4 text-muted-foreground/50" />
            </div>

            {/* Details Panel */}
            <div
              className="bg-muted/30 overflow-auto"
              style={{ height: detailsPanelHeight }}
            >
              <DetailsPanel details={selectedDetails} />
            </div>
          </>
        )}
        </div>
      </TooltipProvider>
    </HierarchyActionsContext.Provider>
  );
});

function NodeIcon({ type, className }: { type: string; className?: string }) {
  switch (type) {
    case 'view':
      return <LayoutList className={cn(className, 'text-blue-500')} />;
    case 'cte':
      return <FileCode className={cn(className, 'text-amber-500')} />;
    default:
      return <TableIcon className={cn(className, 'text-muted-foreground')} />;
  }
}

interface NodeActionButtonsProps {
  nodeId: string;
  nodeName: string;
  scripts?: string[];
  /** Button size variant */
  size?: 'sm' | 'md';
}

/**
 * Reusable action buttons for navigation to lineage, schema, and editor views.
 * Uses HierarchyActionsContext for navigation handlers.
 * Supports keyboard navigation with Enter/Space.
 */
function NodeActionButtons({
  nodeId,
  nodeName,
  scripts,
  size = 'sm',
}: NodeActionButtonsProps) {
  const { onNavigateToLineage, onNavigateToSchema, onNavigateToEditor } = useHierarchyActions();
  const buttonClass = size === 'sm'
    ? 'w-5 h-5 flex items-center justify-center rounded hover:bg-muted-foreground/20 text-muted-foreground/70 hover:text-foreground'
    : 'w-6 h-6 flex items-center justify-center rounded hover:bg-muted text-muted-foreground hover:text-foreground';
  const iconClass = size === 'sm' ? 'w-3 h-3' : 'w-3.5 h-3.5';

  const handleKeyDown = (e: React.KeyboardEvent, action: () => void) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      action();
    }
  };

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            className={buttonClass}
            onClick={(e) => {
              e.stopPropagation();
              onNavigateToLineage(nodeId);
            }}
            onKeyDown={(e) => handleKeyDown(e, () => onNavigateToLineage(nodeId))}
            aria-label="Show in Lineage"
          >
            <Eye className={iconClass} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="top" className="text-xs">Show in Lineage</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            className={buttonClass}
            onClick={(e) => {
              e.stopPropagation();
              onNavigateToSchema(nodeName);
            }}
            onKeyDown={(e) => handleKeyDown(e, () => onNavigateToSchema(nodeName))}
            aria-label="Show in Schema"
          >
            <Grid3X3 className={iconClass} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="top" className="text-xs">Show in Schema</TooltipContent>
      </Tooltip>
      {scripts && scripts.length > 0 && (
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              className={buttonClass}
              onClick={(e) => {
                e.stopPropagation();
                onNavigateToEditor(scripts);
              }}
              onKeyDown={(e) => handleKeyDown(e, () => onNavigateToEditor(scripts))}
              aria-label="Open in Editor"
            >
              <ExternalLink className={iconClass} />
            </button>
          </TooltipTrigger>
          <TooltipContent side="top" className="text-xs">Open in Editor</TooltipContent>
        </Tooltip>
      )}
    </>
  );
}

function NodeTooltipContent({ details }: { details: NodeDetails | null }) {
  if (!details) return <span className="text-muted-foreground">No details</span>;

  return (
    <div className="text-xs space-y-2 py-1">
      <div className="font-medium flex items-center gap-1.5">
        <NodeIcon type={details.type} className="w-3.5 h-3.5" />
        {details.label}
        <span className="text-muted-foreground font-normal">({details.type})</span>
      </div>

      {details.columns.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1 mb-0.5">
            <Columns3 className="w-3 h-3" />
            Columns ({details.columns.length})
          </div>
          <div className="text-[11px] text-muted-foreground/80 truncate max-w-[200px]">
            {details.columns.slice(0, 5).join(', ')}
            {details.columns.length > 5 && ` +${details.columns.length - 5} more`}
          </div>
        </div>
      )}

      {details.scripts.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1 mb-0.5">
            <FileText className="w-3 h-3" />
            Scripts
          </div>
          <div className="text-[11px] text-muted-foreground/80">
            {details.scripts.map((s, i) => (
              <div key={i} className="truncate max-w-[200px]">{s}</div>
            ))}
          </div>
        </div>
      )}

      {details.upstreamMappings.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1 mb-0.5">
            <ArrowRight className="w-3 h-3" />
            Mappings ({details.upstreamMappings.length})
          </div>
        </div>
      )}
    </div>
  );
}

function DetailsPanel({
  details,
}: {
  details: NodeDetails;
}) {
  const { onNavigateToEditor } = useHierarchyActions();
  return (
    <div className="p-3 text-xs space-y-3">
      {/* Header with navigation buttons */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <NodeIcon type={details.type} className="w-4 h-4" />
          <span className="font-medium">{details.label}</span>
          <span className="text-muted-foreground">({details.type})</span>
        </div>
        <div className="flex items-center gap-1">
          <NodeActionButtons
            nodeId={details.id}
            nodeName={details.label}
            scripts={details.scripts}
            size="md"
          />
        </div>
      </div>

      {/* Columns */}
      {details.columns.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1.5 mb-1.5 text-[11px] uppercase tracking-wide">
            <Columns3 className="w-3 h-3" />
            Columns ({details.columns.length})
          </div>
          <div className="flex flex-wrap gap-1">
            {details.columns.map((col) => (
              <span
                key={col}
                className="px-1.5 py-0.5 bg-muted rounded text-[11px] font-mono"
              >
                {col}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Scripts - clickable to open in editor */}
      {details.scripts.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1.5 mb-1.5 text-[11px] uppercase tracking-wide">
            <FileText className="w-3 h-3" />
            Scripts
          </div>
          <div className="space-y-0.5">
            {details.scripts.map((script, i) => (
              <button
                key={i}
                className="w-full text-left font-mono text-[11px] text-muted-foreground hover:text-foreground hover:underline truncate flex items-center gap-1"
                onClick={() => onNavigateToEditor([script])}
              >
                <ExternalLink className="w-3 h-3 shrink-0" />
                <span className="truncate">{script}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Upstream Mappings */}
      {details.upstreamMappings.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1.5 mb-1.5 text-[11px] uppercase tracking-wide">
            <ArrowRight className="w-3 h-3 rotate-180" />
            From ({details.upstreamMappings.length})
          </div>
          <div className="space-y-0.5 font-mono text-[11px]">
            {details.upstreamMappings.slice(0, 10).map((m, i) => (
              <div key={i} className="flex items-center gap-1 text-muted-foreground">
                <span className="truncate max-w-[120px]">{m.fromTable}.{m.fromCol}</span>
                <ArrowRight className="w-3 h-3 shrink-0" />
                <span>{m.toCol}</span>
              </div>
            ))}
            {details.upstreamMappings.length > 10 && (
              <div className="text-muted-foreground/70">
                +{details.upstreamMappings.length - 10} more
              </div>
            )}
          </div>
        </div>
      )}

      {/* Downstream Mappings */}
      {details.downstreamMappings.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1.5 mb-1.5 text-[11px] uppercase tracking-wide">
            <ArrowRight className="w-3 h-3" />
            To ({details.downstreamMappings.length})
          </div>
          <div className="space-y-0.5 font-mono text-[11px]">
            {details.downstreamMappings.slice(0, 10).map((m, i) => (
              <div key={i} className="flex items-center gap-1 text-muted-foreground">
                <span>{m.fromCol}</span>
                <ArrowRight className="w-3 h-3 shrink-0" />
                <span className="truncate max-w-[120px]">{m.toTable}.{m.toCol}</span>
              </div>
            ))}
            {details.downstreamMappings.length > 10 && (
              <div className="text-muted-foreground/70">
                +{details.downstreamMappings.length - 10} more
              </div>
            )}
          </div>
        </div>
      )}

      {/* Empty state */}
      {details.columns.length === 0 &&
        details.scripts.length === 0 &&
        details.upstreamMappings.length === 0 &&
        details.downstreamMappings.length === 0 && (
          <div className="text-muted-foreground italic">No additional details available</div>
        )}
    </div>
  );
}

function LineageNodeItem({
  node,
  nodeKey,
  depth,
  expandedNodes,
  onToggle,
  onSelect,
  activeId,
  focusedKey,
  onFocus,
  filter,
}: {
  node: LineageNode;
  nodeKey: string;
  depth: number;
  expandedNodes: Set<string>;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
  activeId: string | null;
  focusedKey: string | null;
  onFocus: (key: string) => void;
  filter: string;
}) {
  const { getNodeDetails } = useHierarchyActions();
  const hasChildren = node.upstream.length > 0;
  const isExpanded = expandedNodes.has(node.id);
  const showHighlight = filter && node.matchesFilter;
  const details = getNodeDetails(node.id);

  return (
    <div role="treeitem" aria-expanded={hasChildren ? isExpanded : undefined}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            data-node-key={nodeKey}
            className={cn(
              'group relative flex items-center gap-1.5 py-1 px-2 pr-8 cursor-pointer transition-colors',
              'hover:bg-muted/50',
              activeId === node.id && 'bg-muted',
              focusedKey === nodeKey && 'ring-1 ring-inset ring-primary/50'
            )}
            style={{ paddingLeft: `${8 + depth * 16}px` }}
            onClick={() => onSelect(node.id)}
            onMouseEnter={() => onFocus(nodeKey)}
          >
            <button
              className={cn(
                'w-4 h-4 flex items-center justify-center rounded-sm shrink-0',
                'hover:bg-muted-foreground/20 text-muted-foreground/70',
                !hasChildren && 'invisible'
              )}
              onClick={(e) => {
                e.stopPropagation();
                onToggle(node.id);
              }}
              tabIndex={-1}
            >
              {isExpanded ? (
                <ChevronDown className="w-3 h-3" />
              ) : (
                <ChevronRight className="w-3 h-3" />
              )}
            </button>

            <NodeIcon type={node.type} className="w-3.5 h-3.5 shrink-0" />

            <span
              className={cn(
                'truncate flex-1',
                showHighlight && 'bg-yellow-200/50 dark:bg-yellow-500/30'
              )}
            >
              {node.label}
            </span>

            {/* Hover action icons - use opacity instead of display to prevent layout shift */}
            <div className="flex items-center gap-0.5 ml-auto shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
              <NodeActionButtons
                nodeId={node.id}
                nodeName={node.label}
                scripts={details?.scripts}
              />
            </div>

            {/* Count badge - positioned absolutely to not affect layout */}
            {!isExpanded && hasChildren && (
              <span className="absolute right-2 opacity-100 group-hover:opacity-0 transition-opacity text-muted-foreground/50 tabular-nums">
                {node.upstream.length}
              </span>
            )}
          </div>
        </TooltipTrigger>
        <TooltipContent side="right" className="max-w-xs">
          <NodeTooltipContent details={details} />
        </TooltipContent>
      </Tooltip>

      {isExpanded && hasChildren && (
        <div className="relative" role="group">
          <div
            className="absolute top-0 bottom-0 w-px bg-border/50"
            style={{ left: `${16 + depth * 16}px` }}
          />

          {node.upstream.map((child, idx) => {
            const childKey = `${nodeKey}:${idx}/${child.id}`;
            return (
              <LineageNodeItem
                key={childKey}
                node={child}
                nodeKey={childKey}
                depth={depth + 1}
                expandedNodes={expandedNodes}
                onToggle={onToggle}
                onSelect={onSelect}
                activeId={activeId}
                focusedKey={focusedKey}
                onFocus={onFocus}
                filter={filter}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}
