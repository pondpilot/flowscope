import { useEffect } from 'react';
import { LineageProvider, useLineage } from '../context';
import { GraphView } from './GraphView';
import { SqlView } from './SqlView';
import { ColumnPanel } from './ColumnPanel';
import { IssuesPanel } from './IssuesPanel';
import { StatementSelector } from './StatementSelector';
import { SummaryBar } from './SummaryBar';
import type { LineageExplorerProps } from '../types';

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
      <StatementSelector />
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
