import type { ReactNode } from 'react';
import { act, render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { LineageExplorerInner } from '../src/components/LineageExplorer';
import {
  createLineageStore,
  LineageStoreProvider,
  useLineage,
  useLineageActions,
  useLineageState,
} from '../src/store';
import type { LineageActions, LineageContextValue } from '../src/types';

vi.mock('../src/components/GraphView', () => ({ GraphView: () => null }));
vi.mock('../src/components/SqlView', () => ({ SqlView: () => null }));
vi.mock('../src/components/IssuesPanel', () => ({ IssuesPanel: () => null }));

describe('lineage store hooks', () => {
  it('rerenders a state selector only when its selected value changes', () => {
    const store = createLineageStore();
    let renderCount = 0;

    function SelectedSql(): ReactNode {
      renderCount += 1;
      const { sql } = useLineageState((state) => ({
        sql: state.sql,
        selectedNodeId: state.selectedNodeId,
      }));
      return <span>{sql}</span>;
    }

    const view = render(
      <LineageStoreProvider store={store}>
        <SelectedSql />
      </LineageStoreProvider>
    );

    expect(renderCount).toBe(1);

    act(() => store.getState().setSearchTerm('unrelated'));
    expect(renderCount).toBe(1);

    act(() => store.getState().setSql('SELECT 1'));
    expect(renderCount).toBe(2);
    expect(view.getByText('SELECT 1')).toBeTruthy();
  });

  it('keeps aggregate action and legacy context references stable across parent rerenders', () => {
    const store = createLineageStore();
    const actionSnapshots: LineageActions[] = [];
    const selectedActionSnapshots: Array<Pick<LineageActions, 'setResult' | 'setSql'>> = [];
    const contextSnapshots: LineageContextValue[] = [];

    function Consumer(): null {
      actionSnapshots.push(useLineageActions());
      selectedActionSnapshots.push(
        useLineageActions((actions) => ({
          setResult: actions.setResult,
          setSql: actions.setSql,
        }))
      );
      contextSnapshots.push(useLineage());
      return null;
    }

    const view = render(
      <LineageStoreProvider store={store}>
        <Consumer />
      </LineageStoreProvider>
    );
    view.rerender(
      <LineageStoreProvider store={store}>
        <Consumer />
      </LineageStoreProvider>
    );

    expect(actionSnapshots).toHaveLength(2);
    expect(actionSnapshots[1]).toBe(actionSnapshots[0]);
    expect(selectedActionSnapshots).toHaveLength(2);
    expect(selectedActionSnapshots[1]).toBe(selectedActionSnapshots[0]);
    expect(contextSnapshots).toHaveLength(2);
    expect(contextSnapshots[1]).toBe(contextSnapshots[0]);
    expect(contextSnapshots[1].actions).toBe(contextSnapshots[0].actions);
    expect(contextSnapshots[1].state).toBe(contextSnapshots[0].state);
  });
});

describe('LineageExplorerInner store synchronization', () => {
  it('runs synchronization effects only when their corresponding props change', () => {
    const store = createLineageStore();
    const originalSetResult = store.getState().setResult;
    const originalSetSql = store.getState().setSql;
    const setResult = vi.fn(originalSetResult);
    const setSql = vi.fn(originalSetSql);
    store.setState({ setResult, setSql });

    const view = render(
      <LineageStoreProvider store={store}>
        <LineageExplorerInner result={null} sql="SELECT 1" />
      </LineageStoreProvider>
    );

    expect(setResult).toHaveBeenCalledTimes(1);
    expect(setSql).toHaveBeenCalledTimes(1);

    act(() => store.getState().setSearchTerm('unrelated'));
    view.rerender(
      <LineageStoreProvider store={store}>
        <LineageExplorerInner result={null} sql="SELECT 1" />
      </LineageStoreProvider>
    );

    expect(setResult).toHaveBeenCalledTimes(1);
    expect(setSql).toHaveBeenCalledTimes(1);

    view.rerender(
      <LineageStoreProvider store={store}>
        <LineageExplorerInner result={null} sql="SELECT 2" />
      </LineageStoreProvider>
    );

    expect(setResult).toHaveBeenCalledTimes(1);
    expect(setSql).toHaveBeenCalledTimes(2);
    expect(setSql).toHaveBeenLastCalledWith('SELECT 2');
  });
});
