import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import {
  ChevronRight,
  ChevronDown,
  Table as TableIcon,
  LayoutList,
  FileCode,
  Search,
  PackageOpen,
  Columns3,
  FileText,
  ArrowRight,
  GripHorizontal,
} from 'lucide-react';
import { useLineage } from '@pondpilot/flowscope-react';
import { cn } from '@/lib/utils';
import { Input } from '@/components/ui/input';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

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
}

export function HierarchyView({ className }: HierarchyViewProps) {
  const { state, actions } = useLineage();
  const { result } = state;
  const nodes = result?.globalLineage?.nodes || [];
  const edges = result?.globalLineage?.edges || [];
  const statements = result?.statements || [];

  const [filter, setFilter] = useState('');
  const [expandedNodes, setExpandedNodes] = useState<Set<string>>(new Set());
  const [unusedExpanded, setUnusedExpanded] = useState(false);
  const [detailsPanelHeight, setDetailsPanelHeight] = useState(150);
  const [isResizing, setIsResizing] = useState(false);
  const [focusedNodeKey, setFocusedNodeKey] = useState<string | null>(null);

  const treeContainerRef = useRef<HTMLDivElement>(null);
  const filterInputRef = useRef<HTMLInputElement>(null);

  const getNode = (id: string) => nodes.find((n) => n.id === id);

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
  }, [nodes, edges, statements]);

  const getMappingsForTable = (tableId: string) => {
    const upstreamMappings: Array<{ fromTable: string; fromCol: string; toCol: string }> = [];
    const downstreamMappings: Array<{ toTable: string; fromCol: string; toCol: string }> = [];

    edges.forEach((e) => {
      if (e.type === 'data_flow') {
        const fromNode = getNode(e.from);
        const toNode = getNode(e.to);

        if (fromNode?.type === 'column' && toNode?.type === 'column') {
          const fromTableEdge = edges.find(
            (oe) => oe.type === 'ownership' && oe.to === e.from
          );
          const toTableEdge = edges.find(
            (oe) => oe.type === 'ownership' && oe.to === e.to
          );

          if (fromTableEdge && toTableEdge) {
            const fromTableNode = getNode(fromTableEdge.from);
            const toTableNode = getNode(toTableEdge.from);

            const fromColName = fromNode.label.includes('.')
              ? fromNode.label.split('.').pop()!
              : fromNode.label;
            const toColName = toNode.label.includes('.')
              ? toNode.label.split('.').pop()!
              : toNode.label;

            if (toTableEdge.from === tableId && fromTableNode) {
              upstreamMappings.push({
                fromTable: fromTableNode.label,
                fromCol: fromColName,
                toCol: toColName,
              });
            }

            if (fromTableEdge.from === tableId && toTableNode) {
              downstreamMappings.push({
                toTable: toTableNode.label,
                fromCol: fromColName,
                toCol: toColName,
              });
            }
          }
        }
      }
    });

    return { upstreamMappings, downstreamMappings };
  };

  const getNodeDetails = useCallback((nodeId: string): NodeDetails | null => {
    const node = getNode(nodeId);
    if (!node) return null;

    const columns = columnsByTable.get(nodeId) || [];
    const scripts = scriptsByTable.get(nodeId) || [];
    const { upstreamMappings, downstreamMappings } = getMappingsForTable(nodeId);

    return {
      id: nodeId,
      label: node.label,
      type: node.type,
      columns: columns.sort(),
      scripts,
      upstreamMappings,
      downstreamMappings,
    };
  }, [columnsByTable, scriptsByTable, nodes, edges]);

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
  }, [nodes, edges]);

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
    const matchesFilter = filterLower
      ? label.toLowerCase().includes(filterLower)
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
  }, [sinks, edges, nodes, filter]);

  const filteredUnused = useMemo(() => {
    const filterLower = filter.toLowerCase().trim();
    if (!filterLower) return unusedSources;

    return unusedSources.filter((n) =>
      (n.label || n.id).toLowerCase().includes(filterLower)
    );
  }, [unusedSources, filter]);

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

  const toggleNode = useCallback((nodeId: string) => {
    setExpandedNodes((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  }, []);

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

    const startY = e.clientY;
    const startHeight = detailsPanelHeight;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaY = startY - e.clientY;
      const newHeight = Math.max(80, Math.min(400, startHeight + deltaY));
      setDetailsPanelHeight(newHeight);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  }, [detailsPanelHeight]);

  const hasContent = sinkTrees.length > 0 || filteredUnused.length > 0;

  const selectedDetails = state.selectedNodeId
    ? getNodeDetails(state.selectedNodeId)
    : null;

  return (
    <TooltipProvider delayDuration={400}>
      <div className={cn('flex flex-col h-full bg-background', className)}>
        {/* Filter Input */}
        <div className="p-2 border-b">
          <div className="relative">
            <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
            <Input
              ref={filterInputRef}
              className="h-8 text-xs pl-8"
              placeholder="Filter tables..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown' && flatNodeList.length > 0) {
                  e.preventDefault();
                  setFocusedNodeKey(flatNodeList[0].nodeKey);
                  treeContainerRef.current?.focus();
                }
              }}
            />
          </div>
        </div>

        {/* Tree Content */}
        <div
          ref={treeContainerRef}
          className="flex-1 min-h-0 overflow-auto focus:outline-none"
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
                  getNodeDetails={getNodeDetails}
                />
              ))}

              {filteredUnused.length > 0 && (
                <div className="mt-2 border-t">
                  <button
                    className={cn(
                      'w-full flex items-center gap-1.5 px-2 py-1.5 text-[11px]',
                      'text-muted-foreground hover:bg-muted/50 transition-colors',
                      'focus:outline-none focus-visible:ring-1 focus-visible:ring-ring'
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
                        return (
                          <Tooltip key={unusedNodeKey}>
                            <TooltipTrigger asChild>
                              <div
                                data-node-key={unusedNodeKey}
                                className={cn(
                                  'flex items-center gap-1.5 py-1 px-2 cursor-pointer transition-colors',
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
                                    'truncate',
                                    filter &&
                                      (node.label || node.id).toLowerCase().includes(filter.toLowerCase()) &&
                                      'bg-yellow-200/50 dark:bg-yellow-500/30'
                                  )}
                                >
                                  {node.label || node.id}
                                </span>
                              </div>
                            </TooltipTrigger>
                            <TooltipContent side="right" className="max-w-xs">
                              <NodeTooltipContent details={getNodeDetails(node.id)} />
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
  );
}

function NodeIcon({ type, className }: { type: string; className?: string }) {
  if (type === 'view') {
    return <LayoutList className={cn(className, 'text-blue-500')} />;
  }
  if (type === 'cte') {
    return <FileCode className={cn(className, 'text-amber-500')} />;
  }
  return <TableIcon className={cn(className, 'text-muted-foreground')} />;
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

function DetailsPanel({ details }: { details: NodeDetails }) {
  return (
    <div className="p-3 text-xs space-y-3">
      {/* Header */}
      <div className="flex items-center gap-2">
        <NodeIcon type={details.type} className="w-4 h-4" />
        <span className="font-medium">{details.label}</span>
        <span className="text-muted-foreground">({details.type})</span>
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

      {/* Scripts */}
      {details.scripts.length > 0 && (
        <div>
          <div className="text-muted-foreground flex items-center gap-1.5 mb-1.5 text-[11px] uppercase tracking-wide">
            <FileText className="w-3 h-3" />
            Scripts
          </div>
          <div className="space-y-0.5">
            {details.scripts.map((script, i) => (
              <div key={i} className="font-mono text-[11px] text-muted-foreground truncate">
                {script}
              </div>
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
  getNodeDetails,
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
  getNodeDetails: (id: string) => NodeDetails | null;
}) {
  const hasChildren = node.upstream.length > 0;
  const isExpanded = expandedNodes.has(node.id);
  const showHighlight = filter && node.matchesFilter;

  return (
    <div role="treeitem" aria-expanded={hasChildren ? isExpanded : undefined}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            data-node-key={nodeKey}
            className={cn(
              'flex items-center gap-1.5 py-1 px-2 cursor-pointer transition-colors',
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
                'truncate',
                showHighlight && 'bg-yellow-200/50 dark:bg-yellow-500/30'
              )}
            >
              {node.label}
            </span>

            {!isExpanded && hasChildren && (
              <span className="ml-auto text-muted-foreground/50 tabular-nums shrink-0">
                {node.upstream.length}
              </span>
            )}
          </div>
        </TooltipTrigger>
        <TooltipContent side="right" className="max-w-xs">
          <NodeTooltipContent details={getNodeDetails(node.id)} />
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
                getNodeDetails={getNodeDetails}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}
