import { useLineage } from '../context';

interface SummaryBarProps {
  className?: string;
}

export function SummaryBar({ className }: SummaryBarProps): JSX.Element {
  const { state } = useLineage();
  const { result } = state;

  if (!result) {
    return <></>;
  }

  const { summary } = result;

  return (
    <div className={`flowscope-summary-bar ${className || ''}`}>
      <span className="flowscope-stat">
        <strong>{summary.statementCount}</strong> statements
      </span>
      <span className="flowscope-stat">
        <strong>{summary.tableCount}</strong> tables
      </span>
      <span className="flowscope-stat">
        <strong>{summary.columnCount}</strong> columns
      </span>
      {summary.hasErrors && (
        <span className="flowscope-stat flowscope-stat-error">
          <strong>{summary.issueCount.errors}</strong> errors
        </span>
      )}
    </div>
  );
}
