import { useMemo } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import { useProject } from '../lib/project-store';
import { getLastParseResult } from '../lib/schema-parser';
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

  const debugData = useMemo<DebugData>(() => {
    const currentProject = projectContext?.currentProject;
    const activeFile = currentProject?.files.find(
      (f) => f.id === currentProject.activeFileId
    );

    const resolvedSchema = lineageState.result?.resolvedSchema;
    const importedCount = resolvedSchema?.tables?.filter((t: ResolvedSchemaTable) => t.origin === 'imported').length ?? 0;
    const impliedCount = resolvedSchema?.tables?.filter((t: ResolvedSchemaTable) => t.origin === 'implied').length ?? 0;

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
          parseErrors: getLastParseResult()?.issues?.filter((i: Issue) => i.severity === 'error').map((i: Issue) => i.message) ?? [],
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
  }, [lineageState, projectContext]);

  return debugData;
}
