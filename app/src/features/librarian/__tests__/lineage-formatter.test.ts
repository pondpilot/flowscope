import { describe, expect, it } from 'vitest';
import type { AnalyzeResult } from '@pondpilot/flowscope-core';

import { formatLineage } from '../services/lineage-formatter';

function makeResult(overrides: Partial<AnalyzeResult> = {}): AnalyzeResult {
  return {
    statements: [],
    nodes: [],
    edges: [],
    issues: [],
    summary: {
      tableCount: 0,
      columnCount: 0,
      statementCount: 0,
      joinCount: 0,
      complexityScore: 0,
      issueCount: { errors: 0, warnings: 0, infos: 0 },
      hasErrors: false,
    },
    ...overrides,
  };
}

describe('formatLineage', () => {
  it('returns empty string for null input', () => {
    expect(formatLineage(null)).toBe('');
  });

  it('returns empty string for empty lineage and no schema', () => {
    expect(formatLineage(makeResult())).toBe('');
  });

  it('formats resolved schema tables with columns', () => {
    const result = makeResult({
      resolvedSchema: {
        tables: [
          {
            name: 'users',
            schema: 'public',
            columns: [
              { name: 'id', dataType: 'integer', isPrimaryKey: true },
              { name: 'email', dataType: 'varchar' },
              {
                name: 'org_id',
                dataType: 'integer',
                foreignKey: { table: 'orgs', column: 'id' },
              },
            ],
            origin: 'imported',
            updatedAt: '2026-01-01T00:00:00Z',
          },
        ],
      },
    });

    const output = formatLineage(result);
    expect(output).toContain('public.users');
    expect(output).toContain('id | integer | PK');
    expect(output).toContain('email | varchar');
    expect(output).toContain('org_id | integer | FK -> orgs.id');
  });

  it('falls back to global lineage nodes when no resolved schema', () => {
    const result = makeResult({
      nodes: [
        {
          id: 'n1',
          type: 'table',
          label: 'orders',
          canonicalName: { name: 'orders' },
          statementIds: [0],
        },
        {
          id: 'n2',
          type: 'view',
          label: 'order_summary',
          canonicalName: { name: 'order_summary' },
          statementIds: [0],
        },
      ],
      edges: [],
    });

    const output = formatLineage(result);
    expect(output).toContain('- orders');
    expect(output).toContain('- order_summary');
  });

  it('formats relationships from edges', () => {
    const result = makeResult({
      nodes: [
        {
          id: 'n1',
          type: 'table',
          label: 'users',
          canonicalName: { name: 'users' },
          statementIds: [0],
        },
        {
          id: 'n2',
          type: 'column',
          label: 'users.id',
          canonicalName: { name: 'id', column: 'id' },
          statementIds: [0],
        },
      ],
      edges: [
        {
          id: 'e1',
          from: 'n1',
          to: 'n2',
          type: 'ownership',
          statementIds: [0],
        },
      ],
    });

    const output = formatLineage(result);
    expect(output).toContain('users --[ownership]--> users.id');
  });

  it('uses edge IDs as fallback when node labels not found', () => {
    const result = makeResult({
      nodes: [],
      edges: [
        {
          id: 'e1',
          from: 'unknown1',
          to: 'unknown2',
          type: 'data_flow',
          statementIds: [0],
        },
      ],
    });

    const output = formatLineage(result);
    expect(output).toContain('unknown1 --[data_flow]--> unknown2');
  });

  it('includes both schema and relationships when both present', () => {
    const result = makeResult({
      resolvedSchema: {
        tables: [
          {
            name: 'a',
            columns: [{ name: 'col1' }],
            origin: 'imported',
            updatedAt: '2026-01-01T00:00:00Z',
          },
        ],
      },
      nodes: [
        {
          id: 'n1',
          type: 'table',
          label: 'a',
          canonicalName: { name: 'a' },
          statementIds: [0],
        },
        {
          id: 'n2',
          type: 'table',
          label: 'b',
          canonicalName: { name: 'b' },
          statementIds: [1],
        },
      ],
      edges: [{ id: 'e1', from: 'n1', to: 'n2', type: 'cross_statement', statementIds: [0, 1] }],
    });

    const output = formatLineage(result);
    expect(output).toContain('Tables:');
    expect(output).toContain('Relationships:');
    // Should use resolved schema for tables, not fall back to nodes
    expect(output).toContain('  - col1');
  });
});
