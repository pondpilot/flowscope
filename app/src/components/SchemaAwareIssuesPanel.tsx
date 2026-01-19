import { useMemo } from 'react';
import { useLineage } from '@pondpilot/flowscope-react';
import { AlertCircle, Database, ExternalLink, Eye } from 'lucide-react';
import { useNavigation } from '@/lib/navigation-context';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { IssuesFilterBar } from './IssuesFilterBar';
import {
  useViewStateStore,
  getIssuesStateWithDefaults,
} from '@/lib/view-state-store';
import type { Issue } from '@pondpilot/flowscope-core';

interface SchemaAwareIssuesPanelProps {
  projectId: string;
  onOpenSchemaEditor: () => void;
}

const SCHEMA_ISSUE_CODES = ['UNKNOWN_COLUMN', 'UNKNOWN_TABLE', 'SCHEMA_CONFLICT'];

const SEVERITY_ORDER = { error: 0, warning: 1, info: 2 };

function isSchemaIssue(issue: Issue): boolean {
  return SCHEMA_ISSUE_CODES.includes(issue.code);
}

export function SchemaAwareIssuesPanel({ projectId, onOpenSchemaEditor }: SchemaAwareIssuesPanelProps) {
  const { state, actions } = useLineage();
  const { result } = state;
  const { navigateTo, navigateToEditor } = useNavigation();
  const statements = result?.statements || [];

  // Get filter state from store
  const storedFilterState = useViewStateStore(
    (state) => state.viewStates[projectId]?.issues
  );
  const filterState = useMemo(
    () => getIssuesStateWithDefaults(storedFilterState),
    [storedFilterState]
  );

  // Get the source name for an issue based on statement index
  const getIssueSourceName = (issue: Issue): string | undefined => {
    if (issue.statementIndex !== undefined && statements[issue.statementIndex]) {
      return statements[issue.statementIndex].sourceName;
    }
    return undefined;
  };

  // Sort issues by severity
  const sortedIssues = useMemo(() => {
    return (
      result?.issues
        .slice()
        .sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]) || []
    );
  }, [result?.issues]);

  // Extract available codes and source files for filter dropdowns
  const { availableCodes, availableSourceFiles } = useMemo(() => {
    const codes = new Set<string>();
    const files = new Set<string>();

    for (const issue of sortedIssues) {
      codes.add(issue.code);
      const sourceName = getIssueSourceName(issue);
      if (sourceName) {
        files.add(sourceName);
      }
    }

    return {
      availableCodes: Array.from(codes).sort(),
      availableSourceFiles: Array.from(files).sort(),
    };
  }, [sortedIssues, statements]);

  // Apply filters to issues
  const filteredIssues = useMemo(() => {
    return sortedIssues.filter((issue) => {
      // Severity filter
      if (filterState.severity !== 'all' && issue.severity !== filterState.severity) {
        return false;
      }

      // Code filter
      if (filterState.codes.length > 0 && !filterState.codes.includes(issue.code)) {
        return false;
      }

      // Source file filter
      if (filterState.sourceFiles.length > 0) {
        const sourceName = getIssueSourceName(issue);
        if (!sourceName || !filterState.sourceFiles.includes(sourceName)) {
          return false;
        }
      }

      return true;
    });
  }, [sortedIssues, filterState, statements]);

  // Calculate counts for the filter bar (total, not filtered)
  const counts = useMemo(() => {
    const errors = sortedIssues.filter((i) => i.severity === 'error').length;
    const warnings = sortedIssues.filter((i) => i.severity === 'warning').length;
    const infos = sortedIssues.filter((i) => i.severity === 'info').length;
    return {
      all: sortedIssues.length,
      errors,
      warnings,
      infos,
    };
  }, [sortedIssues]);

  // Count schema issues from total (not filtered) to keep banner stable
  const schemaIssueCount = sortedIssues.filter(isSchemaIssue).length;

  const handleIssueClick = (issue: Issue) => {
    if (issue.span) {
      actions.highlightSpan(issue.span);
    }
    if (issue.statementIndex !== undefined) {
      actions.selectStatement(issue.statementIndex);
    }
  };

  const handleOpenInEditor = (issue: Issue) => {
    const sourceName = getIssueSourceName(issue);
    if (sourceName) {
      navigateToEditor(sourceName, issue.span);
    }
  };

  const handleShowInLineage = (issue: Issue) => {
    if (issue.statementIndex !== undefined) {
      actions.selectStatement(issue.statementIndex);
      navigateTo('lineage', { statementIndex: issue.statementIndex, fitView: true });
    }
  };

  if (!result) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
        <p>No data available</p>
      </div>
    );
  }

  const hasActiveFilters =
    filterState.severity !== 'all' ||
    filterState.codes.length > 0 ||
    filterState.sourceFiles.length > 0;

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Filter bar - only show if there are issues */}
      {sortedIssues.length > 0 && (
        <IssuesFilterBar
          projectId={projectId}
          availableCodes={availableCodes}
          availableSourceFiles={availableSourceFiles}
          counts={counts}
          schemaIssueCount={schemaIssueCount}
          onOpenSchemaEditor={onOpenSchemaEditor}
        />
      )}

      {/* Issues list */}
      <div className="flex-1 overflow-auto p-4">
        {sortedIssues.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
            <p className="text-sm">Analysis completed without issues</p>
          </div>
        ) : filteredIssues.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
            <p className="text-sm">No issues match the current filters</p>
          </div>
        ) : (
          <TooltipProvider delayDuration={300}>
            <div className="space-y-2">
              {/* Filtered count indicator */}
              {hasActiveFilters && (
                <p className="text-xs text-muted-foreground mb-3">
                  Showing {filteredIssues.length} of {sortedIssues.length} issues
                </p>
              )}
              {filteredIssues.map((issue, idx) => {
                const isSchema = isSchemaIssue(issue);
                const sourceName = getIssueSourceName(issue);
                return (
                  <div
                    key={`${issue.code}-${idx}`}
                    onClick={() => handleIssueClick(issue)}
                    className={`p-3 border rounded-md cursor-pointer transition-colors ${
                      issue.severity === 'error'
                        ? 'border-error-light/30 dark:border-error-dark/30 bg-error-light/10 dark:bg-error-dark/10 hover:bg-error-light/20 dark:hover:bg-error-dark/20'
                        : issue.severity === 'warning'
                          ? 'border-warning-light/30 dark:border-warning-dark/30 bg-warning-light/10 dark:bg-warning-dark/10 hover:bg-warning-light/20 dark:hover:bg-warning-dark/20'
                          : 'border-primary/20 bg-highlight hover:bg-highlight/80'
                    } ${isSchema ? 'ring-2 ring-primary ring-offset-1 dark:ring-offset-background' : ''}`}
                  >
                    <div className="flex items-start justify-between gap-2 mb-1">
                      <div className="flex items-center gap-2">
                        <AlertCircle
                          className={`h-4 w-4 shrink-0 ${
                            issue.severity === 'error'
                              ? 'text-error-light dark:text-error-dark'
                              : issue.severity === 'warning'
                                ? 'text-warning-light dark:text-warning-dark'
                                : 'text-primary'
                          }`}
                        />
                        <span
                          className={`text-xs font-medium uppercase ${
                            issue.severity === 'error'
                              ? 'text-error-light dark:text-error-dark'
                              : issue.severity === 'warning'
                                ? 'text-warning-light dark:text-warning-dark'
                                : 'text-primary'
                          }`}
                        >
                          {issue.severity}
                        </span>
                        <code className="text-xs bg-black/5 dark:bg-white/10 px-1.5 py-0.5 rounded font-mono">
                          {issue.code}
                        </code>
                      </div>
                      <div className="flex items-center gap-1">
                        {isSchema && (
                          <span className="text-xs bg-primary text-primary-foreground px-2 py-0.5 rounded-full font-medium flex items-center gap-1">
                            <Database className="h-3 w-3" />
                            Schema
                          </span>
                        )}
                        {issue.statementIndex !== undefined && (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <button
                                className="w-6 h-6 flex items-center justify-center rounded hover:bg-black/10 dark:hover:bg-white/10 text-muted-foreground hover:text-foreground"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleShowInLineage(issue);
                                }}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter' || e.key === ' ') {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    handleShowInLineage(issue);
                                  }
                                }}
                                aria-label="Show in Lineage"
                              >
                                <Eye className="w-3.5 h-3.5" />
                              </button>
                            </TooltipTrigger>
                            <TooltipContent side="top" className="text-xs">
                              Show in Lineage
                            </TooltipContent>
                          </Tooltip>
                        )}
                        {sourceName && (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <button
                                className="w-6 h-6 flex items-center justify-center rounded hover:bg-black/10 dark:hover:bg-white/10 text-muted-foreground hover:text-foreground"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleOpenInEditor(issue);
                                }}
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter' || e.key === ' ') {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    handleOpenInEditor(issue);
                                  }
                                }}
                                aria-label="Open in Editor"
                              >
                                <ExternalLink className="w-3.5 h-3.5" />
                              </button>
                            </TooltipTrigger>
                            <TooltipContent side="top" className="text-xs">
                              Open in Editor
                            </TooltipContent>
                          </Tooltip>
                        )}
                      </div>
                    </div>
                    <p className="text-sm text-foreground mb-1">{issue.message}</p>
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                      {issue.statementIndex !== undefined && (
                        <span>Statement {issue.statementIndex + 1}</span>
                      )}
                      {sourceName && (
                        <>
                          {issue.statementIndex !== undefined && <span>•</span>}
                          <span className="font-mono truncate max-w-[200px]">{sourceName}</span>
                        </>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </TooltipProvider>
        )}
      </div>
    </div>
  );
}
