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
  // resolver actually reads (`nodes`); the rest of `AnalyzeResult` is
  // irrelevant here.
  return {
    nodes,
    edges: [],
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
    expect(result).toEqual({ nodeIds: [], tablesToExpand: [], primaryFocusId: null });
  });

  it('returns empty result for an empty refs array', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), []);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [], primaryFocusId: null });
  });

  it('matches a table reference by label (case-insensitive)', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [{ tableName: 'bkpf' }]);
    expect(out.nodeIds).toEqual(['t:bkpf']);
    expect(out.tablesToExpand).toEqual([]);
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('matches a qualified column to exactly one node and expands its parent', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
    expect(out.nodeIds).toEqual(['c:bkpf.mandt']);
    expect(out.tablesToExpand).toEqual(['t:bkpf']);
    // Column nodes are not top-level ReactFlow nodes, so primaryFocusId points
    // at the parent table for viewport recentering.
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('matches qualified column case-insensitively', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'bkpf', columnName: 'mandt' },
    ]);
    expect(out.nodeIds).toEqual(['c:bkpf.mandt']);
    expect(out.tablesToExpand).toEqual(['t:bkpf']);
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('expands every parent table for a bare column with multiple matches', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { columnName: 'MANDT', bareColumn: true },
    ]);
    expect(new Set(out.nodeIds)).toEqual(new Set(['c:bkpf.mandt', 'c:bseg.mandt', 'c:t001.mandt']));
    expect(new Set(out.tablesToExpand)).toEqual(new Set(['t:bkpf', 't:bseg', 't:t001']));
    // Focus on the parent of the first matched column.
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('returns empty arrays when the reference has no match', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [{ tableName: 'NOPE' }]);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [], primaryFocusId: null });
  });

  it('skips refs with zero matches but returns matches for others', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'NOPE' },
      { tableName: 'BKPF' },
      { columnName: 'GHOST', bareColumn: true },
    ]);
    expect(out.nodeIds).toEqual(['t:bkpf']);
    expect(out.tablesToExpand).toEqual([]);
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('deduplicates node IDs and tablesToExpand across overlapping refs', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { tableName: 'BKPF', columnName: 'MANDT' },
      { columnName: 'MANDT', bareColumn: true },
      { tableName: 'BKPF', columnName: 'MANDT' },
    ]);
    expect(out.nodeIds.filter((id) => id === 'c:bkpf.mandt')).toHaveLength(1);
    expect(out.tablesToExpand.filter((id) => id === 't:bkpf')).toHaveLength(1);
    expect(new Set(out.nodeIds)).toEqual(new Set(['c:bkpf.mandt', 'c:bseg.mandt', 'c:t001.mandt']));
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
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [], primaryFocusId: null });
  });

  it('does not match bare-column refs against table nodes', () => {
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { columnName: 'BKPF', bareColumn: true },
    ]);
    expect(out).toEqual({ nodeIds: [], tablesToExpand: [], primaryFocusId: null });
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

  it('expands the parent table for column canonical names without a column field', () => {
    const nodes: TestNode[] = [
      {
        id: 't:users',
        type: 'table',
        label: 'users',
        canonicalName: { name: 'users' },
      },
      {
        id: 'c:users.id',
        type: 'column',
        label: 'id',
        canonicalName: { schema: 'users', name: 'id' },
      },
    ];

    const bareOut = resolveLineageNodeIds(makeResult(nodes), [
      { columnName: 'id', bareColumn: true },
    ]);
    expect(bareOut.nodeIds).toEqual(['c:users.id']);
    expect(bareOut.tablesToExpand).toEqual(['t:users']);
    expect(bareOut.primaryFocusId).toBe('t:users');

    const qualifiedOut = resolveLineageNodeIds(makeResult(nodes), [
      { tableName: 'users', columnName: 'id' },
    ]);
    expect(qualifiedOut.nodeIds).toEqual(['c:users.id']);
    expect(qualifiedOut.tablesToExpand).toEqual(['t:users']);
    expect(qualifiedOut.primaryFocusId).toBe('t:users');
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
    // First ref matches a table directly, so primaryFocusId is that table.
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('falls back to the column id for primaryFocusId when no parent table can be resolved', () => {
    const nodes: TestNode[] = [{ id: 'c:mandt', type: 'column', label: 'MANDT' }];
    const out = resolveLineageNodeIds(makeResult(nodes), [
      { columnName: 'MANDT', bareColumn: true },
    ]);
    expect(out.nodeIds).toEqual(['c:mandt']);
    expect(out.tablesToExpand).toEqual([]);
    // No parent table available — caller will see the column id and accept the
    // useNodeFocus no-op rather than focusing on something incorrect.
    expect(out.primaryFocusId).toBe('c:mandt');
  });

  it('exposes the parent table as primaryFocusId even when a column ref appears first', () => {
    // Regression: previously the navigation layer focused on highlightNodeIds[0]
    // directly, which silently no-ops when that id is a column. The resolver
    // now hands callers the parent table id for viewport focus.
    const out = resolveLineageNodeIds(makeResult(FULL_GRAPH), [
      { columnName: 'MANDT', bareColumn: true },
      { tableName: 'BKPF' },
    ]);
    expect(out.nodeIds[0]).toMatch(/^c:/);
    expect(out.primaryFocusId).toBe('t:bkpf');
  });

  it('routes columns to their actual parent table when multiple schemas share a table name', () => {
    // Regression: previously the parent-table index was keyed only on the
    // bare lowercased name, so a column from `staging.BKPF.MANDT` would have
    // its parent looked up as 'bkpf' and resolved to whichever BKPF was
    // registered first (here, the sap version) — leaving the staging.BKPF
    // table collapsed and its column highlight invisible.
    const nodes: TestNode[] = [
      {
        id: 't:sap-bkpf',
        type: 'table',
        label: 'BKPF',
        canonicalName: { schema: 'sap', name: 'BKPF' },
      },
      {
        id: 't:staging-bkpf',
        type: 'table',
        label: 'BKPF',
        canonicalName: { schema: 'staging', name: 'BKPF' },
      },
      {
        id: 'c:sap-bkpf-mandt',
        type: 'column',
        label: 'MANDT',
        canonicalName: { schema: 'sap', name: 'BKPF', column: 'MANDT' },
      },
      {
        id: 'c:staging-bkpf-mandt',
        type: 'column',
        label: 'MANDT',
        canonicalName: { schema: 'staging', name: 'BKPF', column: 'MANDT' },
      },
    ];
    const out = resolveLineageNodeIds(makeResult(nodes), [
      { columnName: 'MANDT', bareColumn: true },
    ]);
    expect(new Set(out.nodeIds)).toEqual(new Set(['c:sap-bkpf-mandt', 'c:staging-bkpf-mandt']));
    // Both parent tables must be expanded so each column highlight is visible.
    expect(new Set(out.tablesToExpand)).toEqual(new Set(['t:sap-bkpf', 't:staging-bkpf']));
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
