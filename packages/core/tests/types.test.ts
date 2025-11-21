import { describe, it, expect } from 'vitest';
import type {
  AnalyzeRequest,
  AnalyzeResult,
  Dialect,
  Node,
  Edge,
  Issue,
} from '../src/types';
import { IssueCodes } from '../src/types';

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
        globalLineage: { nodes: [], edges: [] },
        issues: [],
        summary: {
          statementCount: 0,
          tableCount: 0,
          columnCount: 0,
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
      };

      expect(node.type).toBe('table');
      expect(node.label).toBe('users');
    });

    it('should represent CTE nodes', () => {
      const node: Node = {
        id: 'cte_12345',
        type: 'cte',
        label: 'active_users',
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
});
