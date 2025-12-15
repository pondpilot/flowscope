import { useMemo, useState } from 'react';
import type {
  AnalyzeResult,
  ColumnTag,
  NodeType,
  SchemaTable,
} from '@pondpilot/flowscope-core';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Database, FileText, Tag as TagIcon, ArrowDownToLine, Layers, Activity, Tags } from 'lucide-react';
import { TagManagerPanel } from './TagManagerPanel';
import { cn } from '@/lib/utils';

interface TagImpactViewProps {
  result: AnalyzeResult;
  schema: SchemaTable[];
  overrides?: Record<string, Record<string, ColumnTag[]>>;
  onUpdateTags?: (tableCanonical: string, columnName: string, tags: ColumnTag[]) => void;
  onClearTags?: (tableCanonical: string, columnName: string) => void;
}

interface TagDetail {
  name: string;
  directCount: number;
  propagatedCount: number;
  directSources: TaggedItem[];
  propagatedTargets: TaggedItem[];
}

interface TaggedItem {
  id: string;
  tableName: string;
  columnName: string;
  nodeType: NodeType;
  fullPath: string;
}

const SENSITIVE_KEYWORDS = ['pii', 'secret', 'password', 'key', 'token', 'gdpr', 'hipaa', 'confidential', 'ssn', 'email', 'credit'];

function getTagVariant(tagName: string): "default" | "secondary" | "destructive" | "outline" {
  if (SENSITIVE_KEYWORDS.some(k => tagName.toLowerCase().includes(k))) {
    return "destructive";
  }
  return "secondary";
}

export function TagImpactView({
  result,
  schema,
  overrides,
  onUpdateTags,
  onClearTags,
}: TagImpactViewProps) {
  const [activeView, setActiveView] = useState<'overview' | 'manage'>('overview');
  const [selectedTag, setSelectedTag] = useState<string | null>(null);

  const normalizedOverrides = overrides ?? {};
  const handleUpdateTags = onUpdateTags ?? (() => {});
  const handleClearTags = onClearTags ?? (() => {});

  // 1. Build the Tag Centric Data Structure
  const tagData = useMemo(() => {
    return buildTagData(result, schema, normalizedOverrides);
  }, [result, schema, normalizedOverrides]);

  const sortedTags = useMemo(() => {
    return Array.from(tagData.values()).sort((a, b) => {
      // Sort by total impact (direct + propagated) descending
      const totalA = a.directCount + a.propagatedCount;
      const totalB = b.directCount + b.propagatedCount;
      if (totalA !== totalB) return totalB - totalA;
      return a.name.localeCompare(b.name);
    });
  }, [tagData]);

  // Select first tag by default if none selected
  const activeTagData = useMemo(() => {
    if (selectedTag && tagData.has(selectedTag)) {
      return tagData.get(selectedTag)!;
    }
    if (sortedTags.length > 0) {
      return sortedTags[0];
    }
    return null;
  }, [tagData, selectedTag, sortedTags]);

  return (
    <Tabs
      value={activeView}
      onValueChange={(value) => setActiveView(value as 'overview' | 'manage')}
      className="flex h-full flex-col"
    >
      <div className="border-b bg-muted/10 px-4 py-2">
        <div className="flex items-center justify-between">
          <TabsList className="bg-transparent p-0 gap-1 h-auto">
            <TabsTrigger 
                value="overview" 
                className="flex items-center gap-2 rounded-md px-3 py-1.5 data-[state=active]:bg-secondary data-[state=active]:text-secondary-foreground data-[state=active]:shadow-none bg-transparent hover:bg-muted/50"
            >
                <Activity className="w-4 h-4" />
                Impact
            </TabsTrigger>
            <TabsTrigger 
                value="manage" 
                className="flex items-center gap-2 rounded-md px-3 py-1.5 data-[state=active]:bg-secondary data-[state=active]:text-secondary-foreground data-[state=active]:shadow-none bg-transparent hover:bg-muted/50"
            >
                <Tags className="w-4 h-4" />
                Tags
            </TabsTrigger>
          </TabsList>
        </div>
      </div>

      <TabsContent value="overview" className="flex-1 overflow-hidden m-0 data-[state=active]:flex flex-col">
        {sortedTags.length === 0 ? (
           <div className="flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
              <div className="rounded-full bg-muted/20 p-4">
                <TagIcon className="h-8 w-8 text-muted-foreground/50" />
              </div>
              <div>
                <h3 className="text-lg font-semibold">No Tags Detected</h3>
                <p className="text-sm text-muted-foreground max-w-sm mx-auto mt-1">
                  Add classifications in the "Manage Tags" tab or import a schema with tags to see their impact here.
                </p>
                <Button variant="outline" className="mt-4" onClick={() => setActiveView('manage')}>
                  Go to Tag Manager
                </Button>
              </div>
            </div>
        ) : (
          <div className="grid h-full w-full grid-cols-[250px_minmax(0,1fr)] divide-x">
            {/* LEFT: Tag List */}
            <div className="flex h-full flex-col bg-muted/10">
              <div className="p-3 border-b text-xs font-medium text-muted-foreground uppercase tracking-wider">
                Active Tags ({sortedTags.length})
              </div>
              <ScrollArea className="flex-1">
                <div className="flex flex-col p-2 gap-1">
                  {sortedTags.map((tag) => (
                    <button
                      key={tag.name}
                      onClick={() => setSelectedTag(tag.name)}
                      className={cn(
                        "flex items-center justify-between gap-2 rounded-md px-3 py-2 text-sm transition-colors hover:bg-accent/50 text-left",
                        activeTagData?.name === tag.name 
                          ? "bg-accent text-accent-foreground shadow-sm font-medium" 
                          : "text-muted-foreground"
                      )}
                    >
                      <div className="flex items-center gap-2 truncate">
                        <Badge variant={getTagVariant(tag.name)} className="px-1.5 py-0 text-[10px] h-5 min-w-[1.25rem] justify-center">
                           #
                        </Badge>
                        <span className="truncate">{tag.name}</span>
                      </div>
                      <span className="text-xs opacity-70 tabular-nums">
                        {tag.directCount + tag.propagatedCount}
                      </span>
                    </button>
                  ))}
                </div>
              </ScrollArea>
            </div>

            {/* RIGHT: Detail View */}
            <div className="flex h-full flex-col bg-background min-w-0">
              {activeTagData && (
                <>
                  {/* Header */}
                  <div className="border-b px-6 py-5 bg-background/95 backdrop-blur z-10">
                    <div className="flex items-start justify-between">
                        <div>
                            <div className="flex items-center gap-3">
                                <h2 className="text-2xl font-bold tracking-tight text-foreground">{activeTagData.name}</h2>
                                <Badge variant={getTagVariant(activeTagData.name)} className="text-sm px-2 py-0.5">
                                    {getTagVariant(activeTagData.name) === 'destructive' ? 'Sensitive' : 'General'}
                                </Badge>
                            </div>
                            <p className="text-sm text-muted-foreground mt-1">
                                Found in <strong className="text-foreground">{activeTagData.directCount + activeTagData.propagatedCount}</strong> locations across the lineage.
                            </p>
                        </div>
                        <div className="flex gap-4 text-sm">
                            <div className="flex flex-col items-end">
                                <span className="text-muted-foreground text-xs uppercase">Direct</span>
                                <span className="font-semibold text-lg">{activeTagData.directCount}</span>
                            </div>
                            <div className="w-px bg-border h-10" />
                            <div className="flex flex-col items-end">
                                <span className="text-muted-foreground text-xs uppercase">Propagated</span>
                                <span className="font-semibold text-lg">{activeTagData.propagatedCount}</span>
                            </div>
                        </div>
                    </div>
                  </div>

                  <ScrollArea className="flex-1">
                    <div className="p-6 space-y-8">
                        {/* Direct Sources Section */}
                        <section>
                            <div className="flex items-center gap-2 mb-4">
                                <div className="p-1.5 rounded-md bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400">
                                    <Database className="w-4 h-4" />
                                </div>
                                <h3 className="text-lg font-semibold">Direct Sources</h3>
                                <Badge variant="outline" className="ml-auto text-xs font-normal">
                                    {activeTagData.directSources.length} items
                                </Badge>
                            </div>
                            
                            {activeTagData.directSources.length === 0 ? (
                                <div className="text-sm text-muted-foreground italic pl-9">
                                    No direct assignments found (tag might be purely propagated or orphaned).
                                </div>
                            ) : (
                                <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                                    {activeTagData.directSources.map((item) => (
                                        <ImpactCard key={item.id} item={item} type="source" />
                                    ))}
                                </div>
                            )}
                        </section>

                        <Separator />

                        {/* Propagated Impact Section */}
                        <section>
                            <div className="flex items-center gap-2 mb-4">
                                <div className="p-1.5 rounded-md bg-amber-100 dark:bg-amber-900/30 text-amber-600 dark:text-amber-400">
                                    <ArrowDownToLine className="w-4 h-4" />
                                </div>
                                <h3 className="text-lg font-semibold">Propagated Impact</h3>
                                <Badge variant="outline" className="ml-auto text-xs font-normal">
                                    {activeTagData.propagatedTargets.length} items
                                </Badge>
                            </div>
                            
                            {activeTagData.propagatedTargets.length === 0 ? (
                                <div className="text-sm text-muted-foreground italic pl-9">
                                    No downstream propagation detected for this tag.
                                </div>
                            ) : (
                                <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                                    {activeTagData.propagatedTargets.map((item) => (
                                        <ImpactCard key={item.id} item={item} type="target" />
                                    ))}
                                </div>
                            )}
                        </section>
                    </div>
                  </ScrollArea>
                </>
              )}
            </div>
          </div>
        )}
      </TabsContent>

      <TabsContent value="manage" className="flex-1 focus-visible:outline-none m-0 data-[state=active]:flex flex-col">
        <TagManagerPanel
          schema={schema}
          overrides={normalizedOverrides}
          onUpdateTags={handleUpdateTags}
          onClearTags={handleClearTags}
        />
      </TabsContent>
    </Tabs>
  );
}

function ImpactCard({ item, type }: { item: TaggedItem; type: 'source' | 'target' }) {
    return (
        <div className="group relative flex flex-col rounded-lg border bg-card p-3 shadow-sm hover:shadow-md transition-shadow">
            <div className="flex items-center gap-2 mb-2">
                <div className={cn(
                    "w-1 h-8 rounded-full shrink-0", 
                    type === 'source' ? "bg-blue-500" : "bg-amber-500"
                )} />
                <div className="min-w-0 flex-1">
                     <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-0.5">
                        <FileText className="w-3 h-3" />
                        <span className="truncate">{item.tableName}</span>
                    </div>
                    <div className="font-medium text-sm truncate" title={item.columnName}>
                        {item.columnName}
                    </div>
                </div>
            </div>
            <div className="mt-auto pt-2 border-t flex items-center justify-between text-[10px] text-muted-foreground">
                <span className="uppercase tracking-wider opacity-70">{item.nodeType}</span>
                {type === 'target' && (
                    <span className="flex items-center gap-1 text-amber-600 dark:text-amber-400">
                        <Layers className="w-3 h-3" /> Inherited
                    </span>
                )}
                 {type === 'source' && (
                    <span className="flex items-center gap-1 text-blue-600 dark:text-blue-400">
                        <Database className="w-3 h-3" /> Assigned
                    </span>
                )}
            </div>
        </div>
    );
}

// --- Data Builders ---

function buildTagData(
  result: AnalyzeResult,
  schema: SchemaTable[],
  overrides: Record<string, Record<string, ColumnTag[]>>
): Map<string, TagDetail> {
  const tagMap = new Map<string, TagDetail>();

  const ensureTag = (tagName: string) => {
    if (!tagMap.has(tagName)) {
      tagMap.set(tagName, {
        name: tagName,
        directCount: 0,
        propagatedCount: 0,
        directSources: [],
        propagatedTargets: [],
      });
    }
    return tagMap.get(tagName)!;
  };

  const makeKey = (table: string, col: string) => `${table}::${col}`.toLowerCase();

  const { columns: statementColumns, nodeByKey, nodeById } = collectStatementColumns(result);

  const directAssignments = new Set<string>();

  schema.forEach((table) => {
    const tableName = [table.catalog, table.schema, table.name].filter(Boolean).join('.');
    table.columns?.forEach((col) => {
      if (!col.classifications?.length) return;
      const lookup = nodeByKey.get(makeKey(tableName, col.name));

      col.classifications.forEach((tag) => {
        const detail = ensureTag(tag.name);
        if (lookup) {
          detail.directSources.push({ ...lookup });
          directAssignments.add(`${lookup.id}:${tag.name}`);
        }
      });
    });
  });

  Object.entries(overrides).forEach(([tableKey, cols]) => {
    Object.entries(cols).forEach(([colName, tags]) => {
      if (!tags?.length) return;
      const lookup = nodeByKey.get(makeKey(tableKey, colName));
      tags.forEach((tag) => {
        const detail = ensureTag(tag.name);
        if (lookup) {
          const assignmentKey = `${lookup.id}:${tag.name}`;
          if (!directAssignments.has(assignmentKey)) {
            detail.directSources.push({ ...lookup });
            directAssignments.add(assignmentKey);
          }
        }
      });
    });
  });

  statementColumns.forEach((col) => {
    if (!col.tags?.length) return;
    col.tags.forEach((tag) => {
      const assignmentKey = `${col.id}:${tag.name}`;
      if (directAssignments.has(assignmentKey)) return;
      const detail = ensureTag(tag.name);
      detail.propagatedTargets.push({
        id: col.id,
        tableName: col.tableName,
        columnName: col.columnName,
        nodeType: col.nodeType,
        fullPath: `${col.tableName}.${col.columnName}`,
      });
    });
  });

  (result.summary?.tagFlows ?? []).forEach((flow) => {
    const detail = ensureTag(flow.tag);
    flow.sources?.forEach((sourceId) => {
      const info = nodeById.get(sourceId);
      if (info && !detail.directSources.some((item) => item.id === info.id)) {
        detail.directSources.push({ ...info });
      }
    });
    flow.targets?.forEach((targetId) => {
      const info = nodeById.get(targetId);
      if (info && !directAssignments.has(`${info.id}:${flow.tag}`)) {
        detail.propagatedTargets.push({ ...info });
      }
    });
  });

  tagMap.forEach((detail) => {
    detail.directSources = uniqueItems(detail.directSources);
    detail.propagatedTargets = uniqueItems(detail.propagatedTargets);
    detail.directCount = detail.directSources.length;
    detail.propagatedCount = detail.propagatedTargets.length;
  });

  return tagMap;
}

function collectStatementColumns(result: AnalyzeResult) {
  const columns: Array<{
    id: string;
    tableName: string;
    columnName: string;
    tags: ColumnTag[];
    nodeType: NodeType;
  }> = [];
  const nodeByKey = new Map<
    string,
    { id: string; tableName: string; columnName: string; nodeType: NodeType; fullPath: string }
  >();
  const nodeById = new Map<string, { id: string; tableName: string; columnName: string; nodeType: NodeType; fullPath: string }>();

  const makeKey = (table: string, col: string) => `${table}::${col}`.toLowerCase();

  for (const statement of result.statements ?? []) {
    const tableNodes = new Map<
      string,
      { label: string; qualifiedName?: string; nodeType: NodeType }
    >();
    statement.nodes.forEach((node) => {
      if (node.type === 'table' || node.type === 'view' || node.type === 'cte') {
        const label = node.qualifiedName || node.label || node.id;
        tableNodes.set(node.id, { label, qualifiedName: node.qualifiedName, nodeType: node.type });
      }
    });

    const columnOwnership = new Map<string, string>();
    statement.edges.forEach((edge) => {
      if (edge.type === 'ownership' && tableNodes.has(edge.from)) {
        columnOwnership.set(edge.to, edge.from);
      }
    });

    statement.nodes.forEach((node) => {
      if (node.type !== 'column') return;
      const tableId = columnOwnership.get(node.id);
      if (!tableId) return;
      const tableInfo = tableNodes.get(tableId);
      const tableName = tableInfo?.qualifiedName || tableInfo?.label || tableId;
      const entry = {
        id: node.id,
        tableName,
        columnName: node.label,
        tags: node.tags ?? [],
        nodeType: node.type,
        fullPath: `${tableName}.${node.label}`,
      };
      columns.push(entry);
      const key = makeKey(tableName, node.label);
      if (!nodeByKey.has(key)) {
        nodeByKey.set(key, entry);
      }
      if (!nodeById.has(node.id)) {
        nodeById.set(node.id, entry);
      }
    });
  }

  return { columns, nodeByKey, nodeById };
}

function uniqueItems(items: TaggedItem[]): TaggedItem[] {
    const seen = new Set<string>();
    return items.filter(item => {
        if (seen.has(item.id)) return false;
        seen.add(item.id);
        return true;
    });
}
