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
import { useGlobalShortcuts } from '@/hooks';
import type { GlobalShortcut } from '@/hooks';
import { getShortcutDisplay } from '@/lib/shortcuts';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { usePersistedLineageState } from '@/hooks/usePersistedLineageState';
import { usePersistedMatrixState } from '@/hooks/usePersistedMatrixState';
import { usePersistedSchemaState } from '@/hooks/usePersistedSchemaState';
import { isValidTab, useNavigation } from '@/lib/navigation-context';
import { useViewStateStore, getNamespaceFilterStateWithDefaults } from '@/lib/view-state-store';
import { useProject } from '@/lib/project-store';
import { schemaMetadataToSQL } from '@/lib/schema-parser';
import { HierarchyView, type HierarchyViewRef } from './HierarchyView';
import { StatsPopover } from './StatsPopover';
import { NamespaceFilterBar } from './NamespaceFilterBar';
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
  const hierarchyViewRef = useRef<HierarchyViewRef>(null);

  // Helper to focus search inputs with development-mode warnings
  const focusSearchInput = useCallback((selector: string, fallbackName: string) => {
    const element = document.querySelector(selector) as HTMLInputElement;
    if (element) {
      element.focus();
    } else if (import.meta.env.DEV) {
      console.warn(`Focus target "${fallbackName}" not found with selector: ${selector}`);
    }
  }, []);

  // Use refs to avoid stale closures and prevent unnecessary shortcut re-memoization
  const actionsRef = useRef<LineageActions>(actions);
  const stateRef = useRef(state);
  useEffect(() => {
    actionsRef.current = actions;
    stateRef.current = state;
  }, [actions, state]);
  const { currentProject, updateSchemaSQL, activeProjectId, isBackendMode, backendSchema } =
    useProject();
  const [schemaEditorOpen, setSchemaEditorOpen] = useState(false);
  const { activeTab, setActiveTab, navigationTarget, clearNavigationTarget } = useNavigation();
  const [lineageFocusNodeId, setLineageFocusNodeId] = useState<string | undefined>(undefined);
  const [fitViewTrigger, setFitViewTrigger] = useState(0);
  const [mountedTabs, setMountedTabs] = useState<Set<string>>(() => new Set([activeTab]));

  // Persisted state hooks for each view
  const matrixState = usePersistedMatrixState(activeProjectId);
  const lineageState = usePersistedLineageState(activeProjectId);
  const schemaState = usePersistedSchemaState(activeProjectId);

  // Refs for shortcut handlers to access latest values without re-memoization
  const activeTabRef = useRef(activeTab);
  const matrixStateRef = useRef(matrixState);
  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);
  useEffect(() => {
    matrixStateRef.current = matrixState;
  }, [matrixState]);

  // Handle navigation target for GraphView - select and focus node/statement when navigating to lineage tab
  useEffect(() => {
    if (activeTab === 'lineage' && navigationTarget) {
      if (navigationTarget.tableId) {
        // Navigate to specific table node
        actionsRef.current.selectNode(navigationTarget.tableId);
        setLineageFocusNodeId(navigationTarget.tableId);
      } else if (navigationTarget.fitView) {
        // Trigger fitView to show all nodes (e.g., from Issues panel)
        setFitViewTrigger((prev) => prev + 1);
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

  // Read namespace filter state from view state store
  const storedNamespaceFilter = useViewStateStore((state) =>
    activeProjectId ? state.viewStates[activeProjectId]?.namespaceFilter : undefined
  );
  const namespaceFilter = useMemo(
    () => getNamespaceFilterStateWithDefaults(storedNamespaceFilter),
    [storedNamespaceFilter]
  );

  // Extract unique schemas and databases from globalLineage nodes
  const { availableSchemas, availableDatabases } = useMemo(() => {
    if (!result?.globalLineage?.nodes) {
      return { availableSchemas: [], availableDatabases: [] };
    }

    const schemas = new Set<string>();
    const databases = new Set<string>();

    for (const node of result.globalLineage.nodes) {
      // Skip column nodes - their canonicalName structure differs:
      // columns have qualified_name like "schema.table.column" which
      // parse_canonical_name incorrectly interprets as "catalog.schema.table"
      if (node.type === 'column') continue;

      const { schema, catalog } = node.canonicalName || {};
      if (schema) schemas.add(schema);
      if (catalog) databases.add(catalog);
    }

    return {
      availableSchemas: Array.from(schemas).sort(),
      availableDatabases: Array.from(databases).sort(),
    };
  }, [result]);

  const handleSaveSchema = useCallback(
    (schemaSQL: string) => {
      if (activeProjectId) {
        updateSchemaSQL(activeProjectId, schemaSQL);
        // Analysis will be re-triggered automatically via useEffect in parent
      }
    },
    [activeProjectId, updateSchemaSQL]
  );

  const handleTabChange = useCallback(
    (value: string) => {
      if (isValidTab(value)) {
        setActiveTab(value);
      }
    },
    [setActiveTab]
  );

  // Ensure the active tab is always mounted (handles both user clicks and external changes)
  useEffect(() => {
    setMountedTabs((prev) => {
      if (prev.has(activeTab)) return prev;
      return new Set([...prev, activeTab]);
    });
  }, [activeTab]);

  // Ref for shortcuts to use handleTabChange without re-memoization
  const handleTabChangeRef = useRef(handleTabChange);
  useEffect(() => {
    handleTabChangeRef.current = handleTabChange;
  }, [handleTabChange]);

  const summary = result?.summary;
  const hasIssues = summary
    ? summary.issueCount.errors > 0 || summary.issueCount.warnings > 0
    : false;

  // Tab switching and schema editor shortcuts
  // Uses refs for frequently-changing values to avoid re-memoization on every state change
  const tabShortcuts = useMemo<GlobalShortcut[]>(
    () => [
      { key: '1', handler: () => handleTabChangeRef.current('lineage') },
      { key: '2', handler: () => handleTabChangeRef.current('hierarchy') },
      { key: '3', handler: () => handleTabChangeRef.current('matrix') },
      { key: '4', handler: () => handleTabChangeRef.current('schema') },
      {
        key: '5',
        handler: () => {
          if (hasIssues) handleTabChangeRef.current('issues');
        },
      },
      // Schema editor shortcut
      {
        key: 'k',
        cmdOrCtrl: true,
        shift: true,
        handler: () => setSchemaEditorOpen(true),
      },
      // Lineage view shortcuts (only active when on lineage tab)
      {
        key: 'v',
        handler: () => {
          if (activeTabRef.current === 'lineage') {
            const newMode = stateRef.current.viewMode === 'table' ? 'script' : 'table';
            actionsRef.current.setViewMode(newMode);
          }
        },
      },
      {
        key: 'c',
        handler: () => {
          if (activeTabRef.current === 'lineage') {
            actionsRef.current.toggleColumnEdges();
          }
        },
      },
      {
        key: 'e',
        handler: () => {
          if (activeTabRef.current === 'lineage') {
            actionsRef.current.setAllNodesCollapsed(false); // Expand all
          }
        },
      },
      {
        key: 'e',
        shift: true,
        handler: () => {
          if (activeTabRef.current === 'lineage') {
            actionsRef.current.setAllNodesCollapsed(true); // Collapse all
          }
        },
      },
      {
        key: 't',
        handler: () => {
          if (activeTabRef.current === 'lineage') {
            actionsRef.current.toggleShowScriptTables();
          }
        },
      },
      {
        key: 'l',
        handler: () => {
          if (activeTabRef.current === 'lineage') {
            const newLayout = stateRef.current.layoutAlgorithm === 'dagre' ? 'elk' : 'dagre';
            actionsRef.current.setLayoutAlgorithm(newLayout);
          }
        },
      },
      {
        key: '/',
        handler: () => {
          // Focus the search input in the current view
          if (activeTabRef.current === 'lineage') {
            focusSearchInput('[data-graph-search-input]', 'lineage search');
          } else if (activeTabRef.current === 'hierarchy') {
            // Use ref for hierarchy view (app layer, full control)
            hierarchyViewRef.current?.focusSearch();
          } else if (activeTabRef.current === 'matrix') {
            focusSearchInput('[data-matrix-search-input]', 'matrix search');
          }
        },
      },
      // Matrix view shortcuts
      {
        key: 'h',
        handler: () => {
          if (activeTabRef.current === 'matrix') {
            const ms = matrixStateRef.current;
            const currentHeatmap = ms.controlledState.heatmapMode ?? false;
            ms.onStateChange({ heatmapMode: !currentHeatmap });
          }
        },
      },
      {
        key: 'x',
        handler: () => {
          if (activeTabRef.current === 'matrix') {
            const ms = matrixStateRef.current;
            const currentXRay = ms.controlledState.xRayMode ?? false;
            ms.onStateChange({ xRayMode: !currentXRay });
          }
        },
      },
    ],
    [hasIssues, focusSearchInput]
  );

  useGlobalShortcuts(tabShortcuts);

  // Redirect from issues tab if there are no issues
  // This effect must be before any early returns to satisfy Rules of Hooks
  useEffect(() => {
    if (!hasIssues && activeTab === 'issues') {
      handleTabChangeRef.current('lineage');
    }
  }, [hasIssues, activeTab]);

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
      <Tabs
        value={activeTab}
        onValueChange={handleTabChange}
        className="flex-1 flex flex-col min-h-0"
      >
        <div className="px-4 py-2 border-b flex items-center justify-between bg-muted/10 h-[44px] shrink-0">
          <TabsList>
            <TabsTrigger value="lineage">Lineage</TabsTrigger>
            <TabsTrigger value="hierarchy">Hierarchy</TabsTrigger>
            <TabsTrigger value="matrix">Matrix</TabsTrigger>
            <TabsTrigger value="schema">Schema</TabsTrigger>
            {hasIssues && (
              <TabsTrigger value="issues" className="text-warning-light dark:text-warning-dark">
                Issues ({summary.issueCount.errors + summary.issueCount.warnings})
              </TabsTrigger>
            )}
          </TabsList>

          {/* Stats Popover and Actions */}
          <div className="flex items-center gap-2">
            <StatsPopover
              tableCount={summary.tableCount}
              columnCount={summary.columnCount}
              joinCount={summary.joinCount}
              complexityScore={summary.complexityScore}
            />
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setSchemaEditorOpen(true)}
                    className="h-7 text-xs"
                  >
                    <Settings className="h-3 w-3 mr-1" />
                    Schema
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  <p className="flex items-center gap-2">
                    Edit schema
                    <kbd className="px-1.5 py-0.5 text-xs bg-muted rounded border font-mono">
                      {getShortcutDisplay('edit-schema')}
                    </kbd>
                  </p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
        </div>

        {/* Namespace filter bar - only shown when schemas/databases are available */}
        {activeProjectId && (availableSchemas.length > 0 || availableDatabases.length > 0) && (
          <NamespaceFilterBar
            projectId={activeProjectId}
            availableSchemas={availableSchemas}
            availableDatabases={availableDatabases}
          />
        )}

        <div className="flex-1 overflow-hidden relative">
          {/* forceMount keeps components mounted when switching tabs to preserve state */}
          <TabsContent
            value="lineage"
            forceMount
            className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden"
          >
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
                namespaceFilter={namespaceFilter}
              />
            </GraphErrorBoundary>
          </TabsContent>

          <TabsContent
            value="hierarchy"
            forceMount
            className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden"
          >
            {mountedTabs.has('hierarchy') && (
              <GraphErrorBoundary>
                <HierarchyView
                  ref={hierarchyViewRef}
                  className="h-full"
                  projectId={activeProjectId}
                />
              </GraphErrorBoundary>
            )}
          </TabsContent>

          <TabsContent
            value="matrix"
            forceMount
            className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden"
          >
            {mountedTabs.has('matrix') && (
              <MatrixView
                className="h-full"
                controlledState={matrixState.controlledState}
                onStateChange={matrixState.onStateChange}
              />
            )}
          </TabsContent>

          <TabsContent
            value="schema"
            forceMount
            className="h-full mt-0 p-0 absolute inset-0 data-[state=inactive]:hidden"
          >
            {mountedTabs.has('schema') && (
              <SchemaView
                schema={schema}
                selectedTableName={schemaState.selectedTableName}
                onClearSelection={schemaState.clearSelection}
              />
            )}
          </TabsContent>

          {hasIssues && activeProjectId && (
            <TabsContent
              value="issues"
              forceMount
              className="h-full mt-0 overflow-auto p-0 absolute inset-0 data-[state=inactive]:hidden"
            >
              {mountedTabs.has('issues') && (
                <SchemaAwareIssuesPanel
                  projectId={activeProjectId}
                  onOpenSchemaEditor={() => setSchemaEditorOpen(true)}
                />
              )}
            </TabsContent>
          )}
        </div>
      </Tabs>

      {/* Schema Editor Modal */}
      {currentProject && (
        <SchemaEditor
          open={schemaEditorOpen}
          onOpenChange={setSchemaEditorOpen}
          schemaSQL={
            isBackendMode ? schemaMetadataToSQL(backendSchema) : currentProject.schemaSQL
          }
          dialect={currentProject.dialect}
          onSave={handleSaveSchema}
          isReadOnly={isBackendMode}
        />
      )}
    </div>
  );
}
