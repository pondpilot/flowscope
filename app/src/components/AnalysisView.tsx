import { useRef, useMemo } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import {
  GraphView,
  IssuesPanel,
  SchemaView,
  GraphErrorBoundary,
} from '@pondpilot/flowscope-react';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs';
import type { SchemaTable, AnalyzeResult } from '@pondpilot/flowscope-core';

/**
 * Extract schema tables from the analysis result by grouping columns by their parent tables.
 */
function extractSchemaFromResult(result: AnalyzeResult): SchemaTable[] {
  const tableMap = new Map<string, Set<string>>();

  // Collect all tables and their columns from the lineage nodes
  for (const statement of result.statements) {
    for (const node of statement.nodes) {
      if (node.type === 'table' || node.type === 'cte') {
        const tableName = node.qualifiedName || node.label;
        if (!tableMap.has(tableName)) {
          tableMap.set(tableName, new Set());
        }
      } else if (node.type === 'column') {
        // Try to determine which table this column belongs to
        // This is a simplified approach - in a real scenario, you'd need more metadata
        const columnName = node.label;

        // Find edges that connect this column to a table
        const parentEdge = statement.edges.find(
          edge => edge.to === node.id && edge.type === 'ownership'
        );

        if (parentEdge) {
          const parentNode = statement.nodes.find(n => n.id === parentEdge.from);
          if (parentNode && (parentNode.type === 'table' || parentNode.type === 'cte')) {
            const tableName = parentNode.qualifiedName || parentNode.label;
            if (!tableMap.has(tableName)) {
              tableMap.set(tableName, new Set());
            }
            tableMap.get(tableName)!.add(columnName);
          }
        }
      }
    }
  }

  // Convert map to SchemaTable array
  return Array.from(tableMap.entries()).map(([name, columnsSet]) => ({
    name,
    columns: Array.from(columnsSet).map(columnName => ({ name: columnName })),
  }));
}

/**
 * Main analysis view component showing lineage graph, schema, and details.
 */
export function AnalysisView() {
  const { state } = useLineage();
  const { result } = state;
  const graphContainerRef = useRef<HTMLDivElement>(null);

  const schema = useMemo(() => {
    if (!result) return [];
    return extractSchemaFromResult(result);
  }, [result]);

  if (!result) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
        <div className="p-6 text-center">
          <h3 className="font-semibold mb-2">No Analysis Results</h3>
          <p className="text-sm max-w-xs mx-auto">
            Run analysis on your SQL script to see lineage and schema details here.
          </p>
        </div>
      </div>
    );
  }

  const { summary } = result;
  const hasIssues = summary.issueCount.errors > 0 || summary.issueCount.warnings > 0;

  return (
    <div className="flex flex-col h-full bg-background">
      <Tabs defaultValue="lineage" className="flex-1 flex flex-col min-h-0">
        <div className="px-4 border-b flex items-center justify-between bg-muted/10">
          <TabsList className="h-10 w-auto justify-start bg-transparent p-0">
            <TabsTrigger 
              value="lineage" 
              className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4 h-full"
            >
              Lineage
            </TabsTrigger>
            <TabsTrigger 
              value="schema" 
              className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4 h-full"
            >
              Schema
            </TabsTrigger>
            {hasIssues && (
              <TabsTrigger 
                value="issues" 
                className="data-[state=active]:bg-transparent data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none px-4 h-full text-yellow-600 data-[state=active]:text-yellow-700"
              >
                Issues ({summary.issueCount.errors + summary.issueCount.warnings})
              </TabsTrigger>
            )}
          </TabsList>

          {/* Summary Stats Right Aligned */}
          <div className="flex items-center gap-4 text-xs text-muted-foreground">
            <div className="flex items-center gap-1">
              <span className="font-semibold text-foreground">{summary.tableCount}</span>
              <span>tables</span>
            </div>
            <div className="flex items-center gap-1">
              <span className="font-semibold text-foreground">{summary.columnCount}</span>
              <span>columns</span>
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative">
          <TabsContent value="lineage" className="h-full mt-0 p-0 absolute inset-0">
            <GraphErrorBoundary>
              <GraphView graphContainerRef={graphContainerRef} className="h-full w-full" />
            </GraphErrorBoundary>
          </TabsContent>

          <TabsContent value="schema" className="h-full mt-0 p-0 absolute inset-0">
            <SchemaView schema={schema} />
          </TabsContent>

          {hasIssues && (
            <TabsContent value="issues" className="h-full mt-0 overflow-auto p-0 absolute inset-0">
              <IssuesPanel />
            </TabsContent>
          )}
        </div>
      </Tabs>
    </div>
  );
}