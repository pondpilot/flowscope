import { useRef, useMemo, useState, useCallback } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import {
  GraphView,
  SchemaView,
  MatrixView,
  GraphErrorBoundary,
} from '@pondpilot/flowscope-react';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { SchemaEditor } from './SchemaEditor';
import { SchemaAwareIssuesPanel } from './SchemaAwareIssuesPanel';
import { ComplexityDots } from './ComplexityDots';
import { HierarchyView } from './HierarchyView';
import { useProject } from '@/lib/project-store';
import { Settings } from 'lucide-react';
import type { SchemaTable, AnalyzeResult } from '@pondpilot/flowscope-core';

/**
 * Extract schema tables from the analysis result using resolved schema metadata.
 * When no schema is provided, returns empty array (Schema tab will show "No schema data").
 */
function extractSchemaFromResult(result: AnalyzeResult): SchemaTable[] {
  // Only show schema when explicitly provided via resolvedSchema
  // This has origin tracking and complete metadata from user-provided schema or DDL
  if (result.resolvedSchema?.tables) {
    return result.resolvedSchema.tables;
  }

  // No schema available - return empty array
  // Schema tab will display "No schema data to display"
  return [];
}

/**
 * Main analysis view component showing lineage graph, schema, and details.
 */
export function AnalysisView() {
  const { state } = useLineage();
  const { result } = state;
  const graphContainerRef = useRef<HTMLDivElement>(null);
  const { currentProject, updateSchemaSQL, activeProjectId } = useProject();
  const [schemaEditorOpen, setSchemaEditorOpen] = useState(false);

  const schema = useMemo(() => {
    if (!result) return [];
    return extractSchemaFromResult(result);
  }, [result]);

  const handleSaveSchema = useCallback((schemaSQL: string) => {
    if (activeProjectId) {
      updateSchemaSQL(activeProjectId, schemaSQL);
      // Analysis will be re-triggered automatically via useEffect in parent
    }
  }, [activeProjectId, updateSchemaSQL]);

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
        <div className="px-4 py-2 border-b flex items-center justify-between bg-muted/10 h-[44px] shrink-0">
          <TabsList>
            <TabsTrigger value="lineage">
              Lineage
            </TabsTrigger>
            <TabsTrigger value="hierarchy">
              Hierarchy
            </TabsTrigger>
            <TabsTrigger value="matrix">
              Matrix
            </TabsTrigger>
            <TabsTrigger value="schema">
              Schema
            </TabsTrigger>
            {hasIssues && (
              <TabsTrigger value="issues" className="text-warning-light dark:text-warning-dark">
                Issues ({summary.issueCount.errors + summary.issueCount.warnings})
              </TabsTrigger>
            )}
          </TabsList>

          {/* Summary Stats and Actions Right Aligned */}
          <div className="flex items-center gap-4 text-xs text-muted-foreground">
            <div className="flex items-center gap-1">
              <span className="font-semibold text-foreground">{summary.tableCount}</span>
              <span>tables</span>
            </div>
            <div className="flex items-center gap-1">
              <span className="font-semibold text-foreground">{summary.columnCount}</span>
              <span>columns</span>
            </div>
            <div className="flex items-center gap-1">
              <span className="font-semibold text-foreground">{summary.joinCount}</span>
              <span>joins</span>
            </div>
            <ComplexityDots score={summary.complexityScore} />
            <Button
              variant="outline"
              size="sm"
              onClick={() => setSchemaEditorOpen(true)}
              className="h-7 text-xs"
            >
              <Settings className="h-3 w-3 mr-1" />
              Edit Schema
            </Button>
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative">
          <TabsContent value="lineage" className="h-full mt-0 p-0 absolute inset-0">
            <GraphErrorBoundary>
              <GraphView graphContainerRef={graphContainerRef} className="h-full w-full" />
            </GraphErrorBoundary>
          </TabsContent>

          <TabsContent value="hierarchy" className="h-full mt-0 p-0 absolute inset-0">
            <GraphErrorBoundary>
              <HierarchyView className="h-full" />
            </GraphErrorBoundary>
          </TabsContent>

          <TabsContent value="matrix" className="h-full mt-0 p-0 absolute inset-0">
            <MatrixView className="h-full" />
          </TabsContent>

          <TabsContent value="schema" className="h-full mt-0 p-0 absolute inset-0">
            <SchemaView schema={schema} />
          </TabsContent>

          {hasIssues && (
            <TabsContent value="issues" className="h-full mt-0 overflow-auto p-0 absolute inset-0">
              <SchemaAwareIssuesPanel onOpenSchemaEditor={() => setSchemaEditorOpen(true)} />
            </TabsContent>
          )}
        </div>
      </Tabs>

      {/* Schema Editor Modal */}
      {currentProject && (
        <SchemaEditor
          open={schemaEditorOpen}
          onOpenChange={setSchemaEditorOpen}
          schemaSQL={currentProject.schemaSQL}
          dialect={currentProject.dialect}
          onSave={handleSaveSchema}
        />
      )}
    </div>
  );
}