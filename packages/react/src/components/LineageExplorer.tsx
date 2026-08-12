import { useEffect, type JSX } from 'react';
import { useLineageActions } from '../store';
import { LineageProvider } from '../context';
import { GraphView } from './GraphView';
import { SqlView } from './SqlView';
import { IssuesPanel } from './IssuesPanel';
import type { LineageExplorerProps } from '../types';

interface LineageExplorerInnerProps {
  result: LineageExplorerProps['result'];
  sql: LineageExplorerProps['sql'];
  onSqlChange?: (sql: string) => void;
  dialect?: LineageExplorerProps['dialect'];
  completionSchema?: LineageExplorerProps['completionSchema'];
  disableCompletion?: LineageExplorerProps['disableCompletion'];
  onCompletionError?: LineageExplorerProps['onCompletionError'];
}

export function LineageExplorerInner({
  result,
  sql,
  onSqlChange,
  dialect,
  completionSchema,
  disableCompletion,
  onCompletionError,
}: LineageExplorerInnerProps): JSX.Element {
  const setResult = useLineageActions((actions) => actions.setResult);
  const setSql = useLineageActions((actions) => actions.setSql);

  useEffect(() => {
    setResult(result);
  }, [result, setResult]);

  useEffect(() => {
    setSql(sql);
  }, [sql, setSql]);

  return (
    <div className="flowscope-explorer-inner">
      <div className="flowscope-main-layout">
        <div className="flowscope-left-panel">
          <SqlView
            editable={!!onSqlChange}
            onChange={onSqlChange}
            dialect={dialect}
            completionSchema={completionSchema}
            disableCompletion={disableCompletion}
            onCompletionError={onCompletionError}
          />
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
  dialect,
  completionSchema,
  disableCompletion,
  onCompletionError,
}: LineageExplorerProps): JSX.Element {
  const themeClass = theme === 'dark' ? 'dark' : '';

  return (
    <LineageProvider
      initialResult={result}
      initialSql={sql}
      defaultLayoutAlgorithm={defaultLayoutAlgorithm}
    >
      <div className={`flowscope-explorer ${themeClass} ${className || ''}`.trim()}>
        <LineageExplorerInner
          result={result}
          sql={sql}
          onSqlChange={onSqlChange}
          dialect={dialect}
          completionSchema={completionSchema}
          disableCompletion={disableCompletion}
          onCompletionError={onCompletionError}
        />
      </div>
    </LineageProvider>
  );
}
