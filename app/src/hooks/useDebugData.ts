import { useMemo } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import { useProject } from '../lib/project-store';
import { getLastParseResult } from '../lib/schema-parser';
import { useAnalysisStore } from '../lib/analysis-store';
import type { Issue, ResolvedSchemaTable } from '@pondpilot/flowscope-core';

export interface DebugData {
  analysisResult: {
    result: unknown;
    sql: string;
    summary: {
      hasResult: boolean;
      statementCount: number;
      issueCount: number;
      hasErrors: boolean;
    };
    performance: {
      lastDurationMs: number | null;
      lastCacheHit: boolean | null;
      lastCacheKey: string | null;
      lastAnalyzedAt: number | null;
      workerTimings: {
        totalMs: number;
        cacheReadMs: number;
        schemaParseMs: number;
        analyzeMs: number;
      } | null;
    };
    globalLineage: {
      nodeCount: number;
      edgeCount: number;
      tableNodes: Array<{ id: string; label: string; type: string }>;
    };
  };
  schema: {
    schemaSQL: string;
    hasSchemaSQL: boolean;
    resolvedSchema: unknown;
    resolvedTableCount: number;
    importedTableCount: number;
    impliedTableCount: number;
    rawResult: unknown;
    parseDebug: {
      parsedTableCount: number;
      parseErrors: string[];
      parseResult: unknown;
    };
  };
  uiState: {
    lineage: {
      selectedNodeId: string | null;
      selectedStatementIndex: number;
      highlightedSpan: unknown;
      searchTerm: string;
      viewMode: string;
      collapsedNodeIds: string[];
      showScriptTables: boolean;
      tableFilter: {
        selectedTableLabels: string[];
        direction: string;
      };
      layoutMetrics: {
        lastDurationMs: number | null;
        nodeCount: number;
        edgeCount: number;
        algorithm: string | null;
        lastUpdatedAt: number | null;
      };
      graphMetrics: {
        lastDurationMs: number | null;
        nodeCount: number;
        edgeCount: number;
        lastUpdatedAt: number | null;
      };
    };
    project: {
      activeProjectId: string | null;
      projectName: string | null;
      activeFileId: string | null;
      activeFileName: string | null;
      dialect: string | null;
      runMode: string | null;
      totalFiles: number;
      selectedFileIds: string[];
    };
  };
  timestamp: string;
}

export function useDebugData(): DebugData {
  const { state: lineageState } = useLineage();
  const projectContext = useProject();
  const getMetrics = useAnalysisStore((state) => state.getMetrics);

  const debugData = useMemo<DebugData>(() => {
    const currentProject = projectContext?.currentProject;
    const activeFile = currentProject?.files.find((f) => f.id === currentProject.activeFileId);

    const resolvedSchema = lineageState.result?.resolvedSchema;
    const importedCount =
      resolvedSchema?.tables?.filter((t: ResolvedSchemaTable) => t.origin === 'imported').length ??
      0;
    const impliedCount =
      resolvedSchema?.tables?.filter((t: ResolvedSchemaTable) => t.origin === 'implied').length ??
      0;

    const metrics = currentProject?.id ? getMetrics(currentProject.id) : null;

    return {
      analysisResult: {
        result: lineageState.result,
        sql: lineageState.sql,
        summary: {
          hasResult: lineageState.result !== null,
          statementCount: lineageState.result?.statements.length ?? 0,
          issueCount: lineageState.result?.issues.length ?? 0,
          hasErrors: lineageState.result?.summary.hasErrors ?? false,
        },
        performance: {
          lastDurationMs: metrics?.lastDurationMs ?? null,
          lastCacheHit: metrics?.lastCacheHit ?? null,
          lastCacheKey: metrics?.lastCacheKey ?? null,
          lastAnalyzedAt: metrics?.lastAnalyzedAt ?? null,
          workerTimings: metrics?.workerTimings ?? null,
        },
        globalLineage: {
          nodeCount: lineageState.result?.globalLineage?.nodes?.length ?? 0,
          edgeCount: lineageState.result?.globalLineage?.edges?.length ?? 0,
          tableNodes: (lineageState.result?.globalLineage?.nodes ?? [])
            .filter(
              (n: { type: string }) => n.type === 'table' || n.type === 'view' || n.type === 'cte'
            )
            .map((n: { id: string; label: string; type: string }) => ({
              id: n.id,
              label: n.label,
              type: n.type,
            })),
        },
      },
      schema: {
        schemaSQL: currentProject?.schemaSQL ?? '',
        hasSchemaSQL: Boolean(currentProject?.schemaSQL?.trim()),
        resolvedSchema: resolvedSchema,
        resolvedTableCount: resolvedSchema?.tables?.length ?? 0,
        importedTableCount: importedCount,
        impliedTableCount: impliedCount,
        rawResult: lineageState.result, // Full result to inspect all fields
        parseDebug: {
          parsedTableCount: getLastParseResult()?.resolvedSchema?.tables?.length ?? 0,
          parseErrors:
            getLastParseResult()
              ?.issues?.filter((i: Issue) => i.severity === 'error')
              .map((i: Issue) => i.message) ?? [],
          parseResult: getLastParseResult(),
        },
      },
      uiState: {
        lineage: {
          selectedNodeId: lineageState.selectedNodeId,
          selectedStatementIndex: lineageState.selectedStatementIndex,
          highlightedSpan: lineageState.highlightedSpan,
          searchTerm: lineageState.searchTerm,
          viewMode: lineageState.viewMode,
          collapsedNodeIds: Array.from(lineageState.collapsedNodeIds),
          showScriptTables: lineageState.showScriptTables,
          tableFilter: {
            selectedTableLabels: Array.from(lineageState.tableFilter.selectedTableLabels),
            direction: lineageState.tableFilter.direction,
          },
          layoutMetrics: {
            lastDurationMs: lineageState.layoutMetrics.lastDurationMs,
            nodeCount: lineageState.layoutMetrics.nodeCount,
            edgeCount: lineageState.layoutMetrics.edgeCount,
            algorithm: lineageState.layoutMetrics.algorithm,
            lastUpdatedAt: lineageState.layoutMetrics.lastUpdatedAt,
          },
          graphMetrics: {
            lastDurationMs: lineageState.graphMetrics.lastDurationMs,
            nodeCount: lineageState.graphMetrics.nodeCount,
            edgeCount: lineageState.graphMetrics.edgeCount,
            lastUpdatedAt: lineageState.graphMetrics.lastUpdatedAt,
          },
        },
        project: {
          activeProjectId: projectContext?.activeProjectId ?? null,
          projectName: currentProject?.name ?? null,
          activeFileId: currentProject?.activeFileId ?? null,
          activeFileName: activeFile?.name ?? null,
          dialect: currentProject?.dialect ?? null,
          runMode: currentProject?.runMode ?? null,
          totalFiles: currentProject?.files.length ?? 0,
          selectedFileIds: currentProject?.selectedFileIds ?? [],
        },
      },
      timestamp: new Date().toISOString(),
    };
  }, [lineageState, projectContext, getMetrics]);

  return debugData;
}
