import { useEffect } from 'react';
import { LineageProvider, useLineage } from '../context';
import { GraphView } from './GraphView';
import { SqlView } from './SqlView';
import { ColumnPanel } from './ColumnPanel';
import { IssuesPanel } from './IssuesPanel';
import type { LineageExplorerProps } from '../types';

interface StatementTabsProps {
  className?: string;
}

function StatementTabs({ className }: StatementTabsProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedStatementIndex } = state;

  if (!result || result.statements.length <= 1) {
    return <></>;
  }

  return (
    <div className={`flowscope-statement-tabs ${className || ''}`}>
      {result.statements.map((stmt, idx) => (
        <button
          key={idx}
          className={`flowscope-tab ${idx === selectedStatementIndex ? 'flowscope-tab-active' : ''}`}
          onClick={() => actions.selectStatement(idx)}
        >
          {idx + 1}: {stmt.statementType}
        </button>
      ))}
    </div>
  );
}

interface SummaryBarProps {
  className?: string;
}

function SummaryBar({ className }: SummaryBarProps): JSX.Element {
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

interface LineageExplorerInnerProps {
  result: LineageExplorerProps['result'];
  sql: LineageExplorerProps['sql'];
  onSqlChange?: (sql: string) => void;
}

function LineageExplorerInner({
  result,
  sql,
  onSqlChange,
}: LineageExplorerInnerProps): JSX.Element {
  const { actions } = useLineage();

  useEffect(() => {
    actions.setResult(result);
  }, [result, actions]);

  useEffect(() => {
    actions.setSql(sql);
  }, [sql, actions]);

  return (
    <div className="flowscope-explorer-inner">
      <SummaryBar />
      <StatementTabs />
      <div className="flowscope-main-layout">
        <div className="flowscope-left-panel">
          <SqlView editable={!!onSqlChange} onChange={onSqlChange} />
          <IssuesPanel />
        </div>
        <div className="flowscope-center-panel">
          <GraphView />
        </div>
        <div className="flowscope-right-panel">
          <ColumnPanel />
        </div>
      </div>
    </div>
  );
}

export function LineageExplorer({
  result,
  sql,
  className,
  onSqlChange,
}: LineageExplorerProps): JSX.Element {
  return (
    <LineageProvider initialResult={result} initialSql={sql}>
      <div className={`flowscope-explorer ${className || ''}`}>
        <LineageExplorerInner result={result} sql={sql} onSqlChange={onSqlChange} />
      </div>
    </LineageProvider>
  );
}
