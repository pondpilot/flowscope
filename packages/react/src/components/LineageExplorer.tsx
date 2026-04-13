import { useEffect, type JSX } from 'react';
import { useLineage } from '../store';
import { LineageProvider } from '../context';
import { GraphView } from './GraphView';
import { SqlView } from './SqlView';
import { IssuesPanel } from './IssuesPanel';
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
      <div className="flowscope-main-layout">
        <div className="flowscope-left-panel">
          <SqlView editable={!!onSqlChange} onChange={onSqlChange} />
          <IssuesPanel />
        </div>
        <div className="flowscope-center-panel">
          <GraphView />
        </div>
        {/* ColumnPanel is removed as 'Details' tab is deprecated */}
        {/* <div className="flowscope-right-panel">
          <ColumnPanel />
        </div> */}
      </div>
    </div>
  );
}

export function LineageExplorer({
  result,
  sql,
  className,
  onSqlChange,
  theme = 'light',
  defaultLayoutAlgorithm,
}: LineageExplorerProps): JSX.Element {
  const themeClass = theme === 'dark' ? 'dark' : '';

  return (
    <LineageProvider
      initialResult={result}
      initialSql={sql}
      defaultLayoutAlgorithm={defaultLayoutAlgorithm}
    >
      <div className={`flowscope-explorer ${themeClass} ${className || ''}`.trim()}>
        <LineageExplorerInner result={result} sql={sql} onSqlChange={onSqlChange} />
      </div>
    </LineageProvider>
  );
}
