import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { describe, expect, it } from 'vitest';

import { resolveLineageNodeIds } from '../lineage-node-resolver';

interface TestNode {
  id: string;
  type: string;
  label: string;
  canonicalName?: {
    catalog?: string;
    schema?: string;
    name?: string;
    column?: string;
  };
}

function makeResult(nodes: TestNode[]): AnalyzeResult {
  // Cast through `unknown` because the test only populates the fields the
  // resolver actually reads (`globalLineage.nodes`); the rest of `AnalyzeResult`
  // is irrelevant here.
  return {
    globalLineage: { nodes, edges: [] },
    statements: [],
    issues: [],
    summary: {
      statementCount: 0,
      tableCount: 0,
      columnCount: 0,
      joinCount: 0,
      complexityScore: 0,
      issueCount: { errors: 0, warnings: 0, infos: 0 },
      hasErrors: false,
    },
  } as unknown as AnalyzeResult;
}

const TABLE_BKPF: TestNode = {
  id: 't:bkpf',
  type: 'table',
  label: 'BKPF',
  canonicalName: { schema: 'sap', name: 'BKPF' },
};
const TABLE_BSEG: TestNode = {
  id: 't:bseg',
  type: 'table',
  label: 'BSEG',
  canonicalName: { schema: 'sap', name: 'BSEG' },
};
const TABLE_T001: TestNode = {
  id: 't:t001',
  type: 'table',
  label: 'T001',
  canonicalName: { schema: 'sap', name: 'T001' },
};
const COL_BKPF_MANDT: TestNode = {
  id: 'c:bkpf.mandt',
  type: 'column',
  label: 'MANDT',
  canonicalName: { schema: 'sap', name: 'BKPF', column: 'MANDT' },
};
const COL_BKPF_BUKRS: TestNode = {
  id: 'c:bkpf.bukrs',
  type: 'column',
  label: 'BUKRS',
  canonicalName: { schema: 'sap', name: 'BKPF', column: 'BUKRS' },
};
const COL_BSEG_MANDT: TestNode = {
  id: 'c:bseg.mandt',
  type: 'column',
  label: 'MANDT',
  canonicalName: { schema: 'sap', name: 'BSEG', column: 'MANDT' },
};
const COL_T001_MANDT: TestNode = {
  id: 'c:t001.mandt',
  type: 'column',
  label: 'MANDT',
  canonicalName: { schema: 'sap', name: 'T001', column: 'MANDT' },
};

const FULL_GRAPH: TestNode[] = [
  TABLE_BKPF,
  TABLE_BSEG,
  TABLE_T001,
  COL_BKPF_MANDT,
  COL_BKPF_BUKRS,
  COL_BSEG_MANDT,
  COL_T001_MANDT,
];

describe('resolveLineageNodeIds', () => {
  it('returns empty result for null AnalyzeResult', () => {
    const result = resolveLineageNodeIds(null, [{ tableName: 'BKPF' }]);
    expect(result).toEqual({ nodeIds: [], tablesToExpand: [] });
  });

  it('returns empty result for an empty refs array', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), []);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [] });
  });

  it('matches a table reference by label (case-insensitive)', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [{ tableName: 'bkpf' }]);
    expect(out.nodeIds).toEqual(['t:bkpf']);
    expect(out.tablesToExpand).toEqual([]);
  });

  it('matches a qualified column to exactly one node and expands its parent', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
    expect(out.nodeIds).toEqual(['c:bkpf.mandt']);
    expect(out.tablesToExpand).toEqual(['t:bkpf']);
  });

  it('matches qualified column case-insensitively', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'bkpf', columnName: 'mandt' },
    ]);
    expect(out.nodeIds).toEqual(['c:bkpf.mandt']);
    expect(out.tablesToExpand).toEqual(['t:bkpf']);
  });

  it('expands every parent table for a bare column with multiple matches', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { columnName: 'MANDT', bareColumn: true },
    ]);
    expect(new Set(out.nodeIds)).toEqual(
      new Set(['c:bkpf.mandt', 'c:bseg.mandt', 'c:t001.mandt'])
    );
    expect(new Set(out.tablesToExpand)).toEqual(new Set(['t:bkpf', 't:bseg', 't:t001']));
  });

  it('returns empty arrays when the reference has no match', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [{ tableName: 'NOPE' }]);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [] });
  });

  it('skips refs with zero matches but returns matches for others', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'NOPE' },
      { tableName: 'BKPF' },
      { columnName: 'GHOST', bareColumn: true },
    ]);
    expect(out.nodeIds).toEqual(['t:bkpf']);
    expect(out.tablesToExpand).toEqual([]);
  });

  it('deduplicates node IDs and tablesToExpand across overlapping refs', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'BKPF', columnName: 'MANDT' },
      { columnName: 'MANDT', bareColumn: true },
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
    expect(out.nodeIds.filter((id) => id === 'c:bkpf.mandt')).toHaveLength(1);
    expect(out.tablesToExpand.filter((id) => id === 't:bkpf')).toHaveLength(1);
    expect(new Set(out.nodeIds)).toEqual(
      new Set(['c:bkpf.mandt', 'c:bseg.mandt', 'c:t001.mandt'])
    );
    expect(new Set(out.tablesToExpand)).toEqual(new Set(['t:bkpf', 't:bseg', 't:t001']));
  });

  it('preserves first-occurrence order across refs', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'BSEG' },
      { tableName: 'BKPF' },
      { tableName: 'BSEG' },
    ]);
    expect(out.nodeIds).toEqual(['t:bseg', 't:bkpf']);
  });

  it('does not match table-like refs against column nodes', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [{ tableName: 'MANDT' }]);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [] });
  });

  it('does not match bare-column refs against table nodes', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { columnName: 'BKPF', bareColumn: true },
    ]);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [] });
  });

  it('matches a qualified column where the canonical name contains a schema prefix', () => {
    const nodes: TestNode[] = [
      {
        id: 't:bkpf',
        type: 'table',
        label: 'BKPF',
        canonicalName: { catalog: 'erp', schema: 'sap', name: 'BKPF' },
      },
      {
        id: 'c:bkpf.mandt',
        type: 'column',
        label: 'MANDT',
        canonicalName: { catalog: 'erp', schema: 'sap', name: 'BKPF', column: 'MANDT' },
      },
    ];
    const out = resolveLineageNodeIds(makeResult(nodes), [
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
    expect(out.nodeIds).toEqual(['c:bkpf.mandt']);
    expect(out.tablesToExpand).toEqual(['t:bkpf']);
  });

  it('handles nodes with no canonicalName by falling back to label', () => {
    const nodes: TestNode[] = [
      { id: 't:bkpf', type: 'table', label: 'BKPF' },
      { id: 'c:mandt', type: 'column', label: 'MANDT' },
    ];
    const out = resolveLineageNodeIds(makeResult(nodes), [
      { tableName: 'BKPF' },
      { columnName: 'MANDT', bareColumn: true },
    ]);
    expect(out.nodeIds).toEqual(['t:bkpf', 'c:mandt']);
    // Without canonicalName.name we cannot map the column to a parent.
    expect(out.tablesToExpand).toEqual([]);
  });

  it('treats views and CTEs as table-like for table refs', () => {
    const nodes: TestNode[] = [
      { id: 'v:report', type: 'view', label: 'REPORT' },
      { id: 'cte:tmp', type: 'cte', label: 'TMP' },
    ];
    const out = resolveLineageNodeIds(makeResult(nodes), [
      { tableName: 'REPORT' },
      { tableName: 'TMP' },
    ]);
    expect(out.nodeIds).toEqual(['v:report', 'cte:tmp']);
  });
});
