import { useMemo } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import { useProject } from '../lib/project-store';

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
