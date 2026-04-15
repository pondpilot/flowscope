import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { scopeNodeToStatement, scopeNodesToStatement } from '../src/statementScopedLineage';
import type { AnalyzeResult, Node } from '../src/types';

describe('statementScopedLineage', () => {
  it('rewrites occurrence and body metadata to the selected statement only', () => {
    const node: Node = {
      id: 'table:users',
      type: 'table',
      label: 'users',
      qualifiedName: 'public.users',
      statementIds: [0, 1],
      span: { start: 5, end: 10 },
      nameSpans: [
        { start: 5, end: 10 },
        { start: 40, end: 45 },
      ],
      bodySpan: { start: 12, end: 20 },
      metadata: {
        occurrenceSpans: [
          { start: 5, end: 10 },
          { start: 40, end: 45 },
        ],
        occurrenceStatementIds: [0, 1],
        occurrenceSourceNames: ['models/a.sql', 'models/b.sql'],
        bodySpans: [
          { start: 12, end: 20 },
          { start: 46, end: 70 },
        ],
        bodyStatementIds: [0, 1],
        bodySourceNames: ['models/a.sql', 'models/b.sql'],
      },
      filters: [],
    };

    const scoped = scopeNodeToStatement(node, 1, 'models/b.sql');

    assert.deepEqual(scoped.statementIds, [1]);
    assert.deepEqual(scoped.nameSpans, [{ start: 40, end: 45 }]);
    assert.deepEqual(scoped.metadata?.occurrenceSpans, [{ start: 40, end: 45 }]);
    assert.deepEqual(scoped.metadata?.occurrenceStatementIds, [1]);
    assert.deepEqual(scoped.metadata?.occurrenceSourceNames, ['models/b.sql']);
    assert.deepEqual(scoped.bodySpan, { start: 46, end: 70 });
    assert.deepEqual(scoped.metadata?.bodySpans, [{ start: 46, end: 70 }]);
    assert.deepEqual(scoped.metadata?.bodyStatementIds, [1]);
    assert.deepEqual(scoped.metadata?.bodySourceNames, ['models/b.sql']);
  });

  it('scopes all nodes in a result using the statement source name', () => {
    const result: AnalyzeResult = {
      statements: [
        {
          statementIndex: 0,
          statementType: 'SELECT',
          sourceName: 'models/a.sql',
          joinCount: 0,
          complexityScore: 1,
        },
        {
          statementIndex: 1,
          statementType: 'SELECT',
          sourceName: 'models/b.sql',
          joinCount: 0,
          complexityScore: 1,
        },
      ],
      nodes: [
        {
          id: 'table:users',
          type: 'table',
          label: 'users',
          qualifiedName: 'public.users',
          statementIds: [0, 1],
          nameSpans: [
            { start: 5, end: 10 },
            { start: 40, end: 45 },
          ],
          metadata: {
            occurrenceSpans: [
              { start: 5, end: 10 },
              { start: 40, end: 45 },
            ],
            occurrenceStatementIds: [0, 1],
            occurrenceSourceNames: ['models/a.sql', 'models/b.sql'],
          },
          filters: [],
        },
      ],
      edges: [],
      issues: [],
      summary: {
        statementCount: 2,
        tableCount: 1,
        columnCount: 0,
        joinCount: 0,
        complexityScore: 1,
        issueCount: { errors: 0, warnings: 0, infos: 0 },
        hasErrors: false,
      },
    };

    const scoped = scopeNodesToStatement(result, 1, result.statements[1].sourceName);

    assert.equal(scoped.length, 1);
    assert.deepEqual(scoped[0].nameSpans, [{ start: 40, end: 45 }]);
    assert.deepEqual(scoped[0].metadata?.occurrenceSourceNames, ['models/b.sql']);
  });
});
