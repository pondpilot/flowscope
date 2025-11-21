import {
  createContext,
  useContext,
  useState,
  useCallback,
  useMemo,
  type ReactNode,
} from 'react';
import type { AnalyzeResult, Span } from '@pondpilot/flowscope-core';
import type { LineageContextValue, LineageState, LineageActions } from './types';

const LineageContext = createContext<LineageContextValue | null>(null);

export interface LineageProviderProps {
  children: ReactNode;
  initialResult?: AnalyzeResult | null;
  initialSql?: string;
}

export function LineageProvider({
  children,
  initialResult = null,
  initialSql = '',
}: LineageProviderProps): JSX.Element {
  const [result, setResult] = useState<AnalyzeResult | null>(initialResult);
  const [sql, setSql] = useState(initialSql);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedStatementIndex, setSelectedStatementIndex] = useState(0);
  const [highlightedSpan, setHighlightedSpan] = useState<Span | null>(null);

  const updateResult = useCallback(
    (nextResult: AnalyzeResult | null) => {
      setResult(nextResult);
      setSelectedNodeId(null);
      setHighlightedSpan(null);
      setSelectedStatementIndex((previousIndex) => {
        const statementCount = nextResult?.statements.length ?? 0;
        if (statementCount === 0) {
          return 0;
        }
        const maxIndex = statementCount - 1;
        return Math.max(0, Math.min(previousIndex, maxIndex));
      });
    },
    []
  );

  const selectNode = useCallback((nodeId: string | null) => {
    setSelectedNodeId(nodeId);
    if (nodeId === null) {
      setHighlightedSpan(null);
    }
  }, []);

  const selectStatement = useCallback((index: number) => {
    setSelectedStatementIndex(index);
    setSelectedNodeId(null);
    setHighlightedSpan(null);
  }, []);

  const highlightSpan = useCallback((span: Span | null) => {
    setHighlightedSpan(span);
  }, []);

  const state: LineageState = useMemo(
    () => ({
      result,
      sql,
      selectedNodeId,
      selectedStatementIndex,
      highlightedSpan,
    }),
    [result, sql, selectedNodeId, selectedStatementIndex, highlightedSpan]
  );

  const actions: LineageActions = useMemo(
    () => ({
      setResult: updateResult,
      setSql,
      selectNode,
      selectStatement,
      highlightSpan,
    }),
    [updateResult, setSql, selectNode, selectStatement, highlightSpan]
  );

  const value = useMemo(() => ({ state, actions }), [state, actions]);

  return <LineageContext.Provider value={value}>{children}</LineageContext.Provider>;
}

export function useLineage(): LineageContextValue {
  const context = useContext(LineageContext);
  if (!context) {
    throw new Error('useLineage must be used within a LineageProvider');
  }
  return context;
}

export function useLineageState(): LineageState {
  return useLineage().state;
}

export function useLineageActions(): LineageActions {
  return useLineage().actions;
}
