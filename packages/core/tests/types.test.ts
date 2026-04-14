import { describe, it, expect } from 'vitest';
import type { AnalyzeRequest, AnalyzeResult, Dialect, Node, Edge, Issue } from '../src/types';
import { IssueCodes, applyEdits } from '../src/types';

describe('Types', () => {
  describe('AnalyzeRequest', () => {
    it('should accept valid request structure', () => {
      const request: AnalyzeRequest = {
        sql: 'SELECT * FROM users',
        dialect: 'postgres',
      };

      expect(request.sql).toBe('SELECT * FROM users');
      expect(request.dialect).toBe('postgres');
    });

    it('should accept optional fields', () => {
      const request: AnalyzeRequest = {
        sql: 'SELECT * FROM users',
        dialect: 'postgres',
        options: {
          enableColumnLineage: true,
        },
        schema: {
          defaultSchema: 'public',
          tables: [
            {
              name: 'users',
              columns: [{ name: 'id' }, { name: 'email', dataType: 'varchar' }],
            },
          ],
        },
      };

      expect(request.options?.enableColumnLineage).toBe(true);
      expect(request.schema?.defaultSchema).toBe('public');
      expect(request.schema?.tables?.[0].name).toBe('users');
    });
  });

  describe('AnalyzeResult', () => {
    it('should have correct structure', () => {
      const result: AnalyzeResult = {
        statements: [],
        nodes: [],
        edges: [],
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
      };

      expect(result.statements).toHaveLength(0);
      expect(result.summary.hasErrors).toBe(false);
    });
  });

  describe('Dialect', () => {
    it('should accept valid dialect values', () => {
      const dialects: Dialect[] = ['generic', 'postgres', 'snowflake', 'bigquery'];

      dialects.forEach((dialect) => {
        const request: AnalyzeRequest = {
          sql: 'SELECT 1',
          dialect,
        };
        expect(request.dialect).toBe(dialect);
      });
    });
  });

  describe('IssueCodes', () => {
    it('should have all expected issue codes', () => {
      expect(IssueCodes.PARSE_ERROR).toBe('PARSE_ERROR');
      expect(IssueCodes.UNKNOWN_TABLE).toBe('UNKNOWN_TABLE');
      expect(IssueCodes.UNKNOWN_COLUMN).toBe('UNKNOWN_COLUMN');
      expect(IssueCodes.UNSUPPORTED_SYNTAX).toBe('UNSUPPORTED_SYNTAX');
      expect(IssueCodes.UNSUPPORTED_RECURSIVE_CTE).toBe('UNSUPPORTED_RECURSIVE_CTE');
    });
  });

  describe('Node', () => {
    it('should represent table nodes', () => {
      const node: Node = {
        id: 'table_12345',
        type: 'table',
        label: 'users',
        qualifiedName: 'public.users',
        statementIds: [0],
      };

      expect(node.type).toBe('table');
      expect(node.label).toBe('users');
    });

    it('should represent CTE nodes', () => {
      const node: Node = {
        id: 'cte_12345',
        type: 'cte',
        label: 'active_users',
        statementIds: [0],
      };

      expect(node.type).toBe('cte');
    });
  });

  describe('Edge', () => {
    it('should represent data flow edges', () => {
      const edge: Edge = {
        id: 'edge_12345',
        from: 'table_a',
        to: 'table_b',
        type: 'data_flow',
        statementIds: [0],
      };

      expect(edge.type).toBe('data_flow');
      expect(edge.from).toBe('table_a');
      expect(edge.to).toBe('table_b');
    });

    it('should support operation labels', () => {
      const edge: Edge = {
        id: 'edge_12345',
        from: 'table_a',
        to: 'table_b',
        type: 'data_flow',
        operation: 'INNER_JOIN',
        statementIds: [0],
      };

      expect(edge.operation).toBe('INNER_JOIN');
    });
  });

  describe('Issue', () => {
    it('should represent errors', () => {
      const issue: Issue = {
        severity: 'error',
        code: 'PARSE_ERROR',
        message: 'Unexpected token',
        span: { start: 10, end: 20 },
        statementIndex: 0,
      };

      expect(issue.severity).toBe('error');
      expect(issue.code).toBe('PARSE_ERROR');
    });

    it('should represent warnings', () => {
      const issue: Issue = {
        severity: 'warning',
        code: 'UNKNOWN_TABLE',
        message: 'Table not found in schema',
      };

      expect(issue.severity).toBe('warning');
    });
  });

  describe('applyEdits', () => {
    it('returns original string for empty edits', () => {
      expect(applyEdits('select 1', [])).toBe('select 1');
    });

    it('applies a single edit', () => {
      const result = applyEdits('select id from t', [
        { span: { start: 0, end: 6 }, replacement: 'SELECT' },
      ]);
      expect(result).toBe('SELECT id from t');
    });

    it('applies multiple non-overlapping edits', () => {
      const result = applyEdits('select id from t', [
        { span: { start: 0, end: 6 }, replacement: 'SELECT' },
        { span: { start: 10, end: 14 }, replacement: 'FROM' },
      ]);
      expect(result).toBe('SELECT id FROM t');
    });

    it('applies edits regardless of input order', () => {
      // Provide edits in reverse order; applyEdits should sort internally
      const result = applyEdits('select id from t', [
        { span: { start: 10, end: 14 }, replacement: 'FROM' },
        { span: { start: 0, end: 6 }, replacement: 'SELECT' },
      ]);
      expect(result).toBe('SELECT id FROM t');
    });

    it('handles multi-byte UTF-8 characters', () => {
      // '你好' is 6 bytes in UTF-8 (3 bytes per character), starting at byte 7
      const result = applyEdits('select 你好', [
        { span: { start: 7, end: 13 }, replacement: 'hello' },
      ]);
      expect(result).toBe('select hello');
    });

    it('handles edit at start of string', () => {
      const result = applyEdits('abc', [{ span: { start: 0, end: 1 }, replacement: 'X' }]);
      expect(result).toBe('Xbc');
    });

    it('handles edit at end of string', () => {
      const result = applyEdits('abc', [{ span: { start: 2, end: 3 }, replacement: 'Z' }]);
      expect(result).toBe('abZ');
    });

    it('handles insertion (zero-width span)', () => {
      const result = applyEdits('ab', [{ span: { start: 1, end: 1 }, replacement: 'X' }]);
      expect(result).toBe('aXb');
    });

    it('handles deletion (empty replacement)', () => {
      const result = applyEdits('abcdef', [{ span: { start: 2, end: 4 }, replacement: '' }]);
      expect(result).toBe('abef');
    });

    it('throws RangeError for out-of-bounds span', () => {
      expect(() => applyEdits('abc', [{ span: { start: 0, end: 100 }, replacement: 'x' }])).toThrow(
        RangeError
      );
    });

    it('throws RangeError when start > end', () => {
      expect(() => applyEdits('abc', [{ span: { start: 2, end: 1 }, replacement: 'x' }])).toThrow(
        RangeError
      );
    });

    it('throws for overlapping edits', () => {
      expect(() =>
        applyEdits('select id', [
          { span: { start: 0, end: 6 }, replacement: 'SELECT' },
          { span: { start: 3, end: 9 }, replacement: 'other' },
        ])
      ).toThrow('Overlapping edits');
    });
  });
});
