import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { LineageActions } from '@pondpilot/flowscope-react';
import {
  GraphErrorBoundary,
  GraphView,
  MatrixView,
  SchemaView,
  useLineage,
} from '@pondpilot/flowscope-react';
import type { AnalyzeResult, SchemaTable } from '@pondpilot/flowscope-core';
import { Settings } from 'lucide-react';

import { Button } from '@/components/ui/button';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/components/ui/tabs';
import { usePersistedLineageState } from '@/hooks/usePersistedLineageState';
import { usePersistedMatrixState } from '@/hooks/usePersistedMatrixState';
import { usePersistedSchemaState } from '@/hooks/usePersistedSchemaState';
import { isValidTab, useNavigation } from '@/lib/navigation-context';
import { useProject } from '@/lib/project-store';
import { ComplexityDots } from './ComplexityDots';
import { HierarchyView } from './HierarchyView';
import { SchemaAwareIssuesPanel } from './SchemaAwareIssuesPanel';
import { SchemaEditor } from './SchemaEditor';

interface AnalysisViewProps {
  graphContainerRef?: React.RefObject<HTMLDivElement | null>;
}

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
export function AnalysisView({ graphContainerRef: externalGraphRef }: AnalysisViewProps) {
  const { state, actions } = useLineage();
  const { result } = state;
  const internalGraphRef = useRef<HTMLDivElement>(null);
  const graphContainerRef = externalGraphRef || internalGraphRef;

  // Use ref to avoid stale closures and prevent unnecessary effect re-runs
  const actionsRef = useRef<LineageActions>(actions);
  useEffect(() => {
    actionsRef.current = actions;
  }, [actions]);
  const { currentProject, updateSchemaSQL, activeProjectId } = useProject();
  const [schemaEditorOpen, setSchemaEditorOpen] = useState(false);
  const { activeTab, setActiveTab, navigationTarget, clearNavigationTarget } = useNavigation();
  const [lineageFocusNodeId, setLineageFocusNodeId] = useState<string | undefined>(undefined);
  const [fitViewTrigger, setFitViewTrigger] = useState(0);

  // Persisted state hooks for each view
  const matrixState = usePersistedMatrixState(activeProjectId);
  const lineageState = usePersistedLineageState(activeProjectId);
  const schemaState = usePersistedSchemaState(activeProjectId);

  // Handle navigation target for GraphView - select and focus node/statement when navigating to lineage tab
  useEffect(() => {
    if (activeTab === 'lineage' && navigationTarget) {
      if (navigationTarget.tableId) {
        // Navigate to specific table node
        actionsRef.current.selectNode(navigationTarget.tableId);
        setLineageFocusNodeId(navigationTarget.tableId);
      } else if (navigationTarget.fitView) {
        // Trigger fitView to show all nodes (e.g., from Issues panel)
        setFitViewTrigger(prev => prev + 1);
      }
      clearNavigationTarget();
    }
  }, [activeTab, navigationTarget, clearNavigationTarget]);

  // Handle navigation target for SchemaView - select table when navigating to schema tab
  useEffect(() => {
    if (activeTab === 'schema' && navigationTarget?.tableName) {
      schemaState.setSelectedTableName(navigationTarget.tableName);
      clearNavigationTarget();
    }
  }, [activeTab, navigationTarget, clearNavigationTarget, schemaState]);

  const handleLineageFocusApplied = useCallback(() => {
    setLineageFocusNodeId(undefined);
  }, []);

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

  const handleTabChange = useCallback((value: string) => {
    if (isValidTab(value)) {
      setActiveTab(value);
    }
  }, [setActiveTab]);

  const summary = result?.summary;
  const hasIssues = summary ? (summary.issueCount.errors > 0 || summary.issueCount.warnings > 0) : false;

  // Redirect from issues tab if there are no issues
  // This effect must be before any early returns to satisfy Rules of Hooks
  useEffect(() => {
    if (!hasIssues && activeTab === 'issues') {
      setActiveTab('lineage');
    }
  }, [hasIssues, activeTab, setActiveTab]);

  if (!result || !summary) {
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

  return (
    <div className="flex flex-col h-full bg-background">
      <Tabs value={activeTab} onValueChange={handleTabChange} className="flex-1 flex flex-col min-h-0">
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
          {/* forceMount keeps components mounted when switching tabs to preserve state */}
          <TabsContent value="lineage" forceMount className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden">
            <GraphErrorBoundary>
              <GraphView
                graphContainerRef={graphContainerRef}
                className="h-full w-full"
                focusNodeId={lineageFocusNodeId}
                onFocusApplied={handleLineageFocusApplied}
                controlledSearchTerm={lineageState.searchTerm}
                onSearchTermChange={lineageState.onSearchTermChange}
                initialViewport={lineageState.initialViewport}
                onViewportChange={lineageState.onViewportChange}
                fitViewTrigger={fitViewTrigger}
              />
            </GraphErrorBoundary>
          </TabsContent>

          <TabsContent value="hierarchy" forceMount className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden">
            <GraphErrorBoundary>
              <HierarchyView className="h-full" projectId={activeProjectId} />
            </GraphErrorBoundary>
          </TabsContent>

          <TabsContent value="matrix" forceMount className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden">
            <MatrixView
              className="h-full"
              controlledState={matrixState.controlledState}
              onStateChange={matrixState.onStateChange}
            />
          </TabsContent>

          <TabsContent value="schema" forceMount className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden">
            <SchemaView
              schema={schema}
              selectedTableName={schemaState.selectedTableName}
              onClearSelection={schemaState.clearSelection}
            />
          </TabsContent>

          {hasIssues && (
            <TabsContent value="issues" forceMount className="h-full mt-0 overflow-auto p-0 absolute inset-0 data-[state=inactive]:hidden">
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
