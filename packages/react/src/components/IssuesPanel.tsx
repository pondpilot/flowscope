import type { JSX } from 'react';
import { useLineage } from '../store';
import type { IssuesPanelProps, Issue } from '../types';

const SEVERITY_ORDER = { error: 0, warning: 1, info: 2 };

export function IssuesPanel({ className, onIssueClick }: IssuesPanelProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result } = state;

  const sortedIssues =
    result?.issues
      .slice()
      .sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]) || [];

  const handleIssueClick = (issue: Issue) => {
    if (issue.span) {
      actions.highlightSpan(issue.span);
    }
    if (issue.statementIndex !== undefined) {
      actions.selectStatement(issue.statementIndex);
    }
    onIssueClick?.(issue);
  };

  if (!result) {
    return (
      <div className={`flowscope-issues-panel flowscope-panel-empty ${className || ''}`}>
        <p>No data available</p>
      </div>
    );
  }

  const { errors, warnings, infos } = result.summary.issueCount;

  return (
    <div className={`flowscope-issues-panel ${className || ''}`}>
      <div className="flowscope-panel-header">
        <h3>Issues</h3>
        <div className="flowscope-issue-counts">
          {errors > 0 && <span className="flowscope-count-error">{errors} errors</span>}
          {warnings > 0 && <span className="flowscope-count-warning">{warnings} warnings</span>}
          {infos > 0 && <span className="flowscope-count-info">{infos} info</span>}
          {sortedIssues.length === 0 && <span className="flowscope-count-success">No issues</span>}
        </div>
      </div>
      <div className="flowscope-panel-content">
        {sortedIssues.length === 0 ? (
          <p className="flowscope-hint">Analysis completed without issues</p>
        ) : (
          <ul className="flowscope-issue-list">
            {sortedIssues.map((issue, idx) => (
              <li
                key={`${issue.code}-${idx}`}
                className={`flowscope-issue flowscope-issue-${issue.severity}`}
                onClick={() => handleIssueClick(issue)}
              >
                <div className="flowscope-issue-header">
                  <span className={`flowscope-severity flowscope-severity-${issue.severity}`}>
                    {issue.severity}
                  </span>
                  <code className="flowscope-issue-code">{issue.code}</code>
                </div>
                <p className="flowscope-issue-message">{issue.message}</p>
                {issue.statementIndex !== undefined && (
                  <span className="flowscope-issue-location">
                    Statement {issue.statementIndex + 1}
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
