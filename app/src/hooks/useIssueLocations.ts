import { useMemo, useCallback } from 'react';
import type { Issue, StatementLineage } from '@pondpilot/flowscope-core';
import { byteOffsetToLineColumn } from '@/lib/utils';

export interface FileInfo {
  name: string;
  path: string;
  content: string;
}

export interface IssueLocation {
  sourceName: string | undefined;
  location: string | null;
  span: { start: number; end: number } | undefined;
}

/**
 * Hook for resolving issue locations from file contents.
 *
 * Handles the mapping between issues, statements, and source files to provide
 * line:column locations for display in the UI.
 */
export function useIssueLocations(files: FileInfo[], statements: StatementLineage[]) {
  // Memoized file lookup map for O(1) access by name or path
  const fileMap = useMemo(() => {
    const map = new Map<string, FileInfo>();
    for (const file of files) {
      map.set(file.name, file);
      if (file.path && file.path !== file.name) {
        map.set(file.path, file);
      }
    }
    return map;
  }, [files]);

  // Get the source name for an issue - prefer issue.sourceName, fall back to statement
  const getIssueSourceName = useCallback(
    (issue: Issue): string | undefined => {
      if (issue.sourceName) {
        return issue.sourceName;
      }
      if (issue.statementIndex !== undefined && statements[issue.statementIndex]) {
        return statements[issue.statementIndex].sourceName;
      }
      return undefined;
    },
    [statements]
  );

  // Get the span for an issue - prefer issue.span, fall back to statement span
  const getIssueSpan = useCallback(
    (issue: Issue): { start: number; end: number } | undefined => {
      if (issue.span) {
        return issue.span;
      }
      if (issue.statementIndex !== undefined && statements[issue.statementIndex]) {
        return statements[issue.statementIndex].span;
      }
      return undefined;
    },
    [statements]
  );

  // Get line:column location for an issue
  const getIssueLocation = useCallback(
    (issue: Issue, sourceName: string | undefined): string | null => {
      const span = getIssueSpan(issue);
      if (!sourceName || !span) return null;

      const file = fileMap.get(sourceName);
      if (!file || file.content === undefined) return null;

      const { line, column } = byteOffsetToLineColumn(file.content, span.start);
      return `${line}:${column}`;
    },
    [fileMap, getIssueSpan]
  );

  // Pre-compute locations for a list of issues (memoized per-issue computation)
  const getIssueLocationsMap = useCallback(
    (issues: Issue[]): Map<Issue, IssueLocation> => {
      const locationMap = new Map<Issue, IssueLocation>();
      for (const issue of issues) {
        const sourceName = getIssueSourceName(issue);
        const location = getIssueLocation(issue, sourceName);
        const span = getIssueSpan(issue);
        locationMap.set(issue, { sourceName, location, span });
      }
      return locationMap;
    },
    [getIssueSourceName, getIssueLocation, getIssueSpan]
  );

  return {
    fileMap,
    getIssueSourceName,
    getIssueSpan,
    getIssueLocation,
    getIssueLocationsMap,
  };
}
