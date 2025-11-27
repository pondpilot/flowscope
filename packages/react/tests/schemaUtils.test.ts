import { describe, it, expect } from 'vitest';
import {
  stripIdentifierQuotes,
  splitQualifiedIdentifier,
  normalizeQualifiedName,
  collectTableLookupKeys,
  resolveForeignKeyTarget,
} from '../src/utils/schemaUtils';
import type { SchemaTable, ResolvedSchemaTable } from '@pondpilot/flowscope-core';

describe('stripIdentifierQuotes', () => {
  it('strips double quotes', () => {
    expect(stripIdentifierQuotes('"users"')).toBe('users');
  });

  it('strips backticks', () => {
    expect(stripIdentifierQuotes('`users`')).toBe('users');
  });

  it('strips square brackets', () => {
    expect(stripIdentifierQuotes('[users]')).toBe('users');
  });

  it('leaves unquoted identifiers unchanged', () => {
    expect(stripIdentifierQuotes('users')).toBe('users');
  });

  it('handles empty string', () => {
    expect(stripIdentifierQuotes('')).toBe('');
  });

  it('handles single character', () => {
    expect(stripIdentifierQuotes('a')).toBe('a');
  });

  it('trims whitespace', () => {
    expect(stripIdentifierQuotes('  "users"  ')).toBe('users');
  });

  it('does not strip mismatched quotes', () => {
    expect(stripIdentifierQuotes('"users`')).toBe('"users`');
  });
});

describe('splitQualifiedIdentifier', () => {
  it('splits unquoted identifier', () => {
    expect(splitQualifiedIdentifier('catalog.schema.table')).toEqual([
      'catalog',
      'schema',
      'table',
    ]);
  });

  it('splits double-quoted identifier', () => {
    expect(splitQualifiedIdentifier('"public"."users"')).toEqual(['"public"', '"users"']);
  });

  it('splits backtick-quoted identifier', () => {
    expect(splitQualifiedIdentifier('`public`.`users`')).toEqual(['`public`', '`users`']);
  });

  it('splits square-bracket-quoted identifier', () => {
    expect(splitQualifiedIdentifier('[dbo].[users]')).toEqual(['[dbo]', '[users]']);
  });

  it('handles dots inside quoted identifiers', () => {
    expect(splitQualifiedIdentifier('"my.schema"."my.table"')).toEqual([
      '"my.schema"',
      '"my.table"',
    ]);
  });

  it('handles escaped double quotes', () => {
    expect(splitQualifiedIdentifier('"col""name"')).toEqual(['"col""name"']);
  });

  it('handles escaped square brackets', () => {
    expect(splitQualifiedIdentifier('[col]]name]')).toEqual(['[col]]name]']);
  });

  it('handles mixed quoting styles', () => {
    expect(splitQualifiedIdentifier('`catalog`."schema".[table]')).toEqual([
      '`catalog`',
      '"schema"',
      '[table]',
    ]);
  });

  it('handles simple table name', () => {
    expect(splitQualifiedIdentifier('users')).toEqual(['users']);
  });

  it('handles empty string', () => {
    expect(splitQualifiedIdentifier('')).toEqual([]);
  });

  it('handles whitespace-only string', () => {
    expect(splitQualifiedIdentifier('   ')).toEqual([]);
  });
});

describe('normalizeQualifiedName', () => {
  it('normalizes unquoted name', () => {
    expect(normalizeQualifiedName('public.users')).toBe('public.users');
  });

  it('strips quotes from all parts', () => {
    expect(normalizeQualifiedName('"public"."users"')).toBe('public.users');
  });

  it('handles mixed quoting', () => {
    expect(normalizeQualifiedName('`catalog`."schema".[table]')).toBe('catalog.schema.table');
  });

  it('returns null for empty string', () => {
    expect(normalizeQualifiedName('')).toBeNull();
  });

  it('returns null for null', () => {
    expect(normalizeQualifiedName(null)).toBeNull();
  });

  it('returns null for undefined', () => {
    expect(normalizeQualifiedName(undefined)).toBeNull();
  });

  it('returns null for whitespace-only', () => {
    expect(normalizeQualifiedName('   ')).toBeNull();
  });

  it('handles quoted dots in identifiers', () => {
    expect(normalizeQualifiedName('"my.schema"."my.table"')).toBe('my.schema.my.table');
  });
});

describe('collectTableLookupKeys', () => {
  it('collects simple table name', () => {
    const table: SchemaTable = { name: 'users' };
    const keys = collectTableLookupKeys(table);
    expect(keys).toContain('users');
  });

  it('includes schema-qualified name', () => {
    const table: SchemaTable = { name: 'users', schema: 'public' };
    const keys = collectTableLookupKeys(table);
    expect(keys).toContain('users');
    expect(keys).toContain('public.users');
  });

  it('includes catalog-qualified name', () => {
    const table: SchemaTable = { name: 'users', catalog: 'mydb' };
    const keys = collectTableLookupKeys(table);
    expect(keys).toContain('users');
    expect(keys).toContain('mydb.users');
  });

  it('includes fully qualified name', () => {
    const table: SchemaTable = { name: 'users', schema: 'public', catalog: 'mydb' };
    const keys = collectTableLookupKeys(table);
    expect(keys).toContain('users');
    expect(keys).toContain('public.users');
    expect(keys).toContain('mydb.users');
    expect(keys).toContain('mydb.public.users');
  });

  it('does not duplicate when name already contains qualifier', () => {
    const table: SchemaTable = { name: 'public.users', schema: 'public' };
    const keys = collectTableLookupKeys(table);
    expect(keys).toContain('public.users');
    expect(keys).not.toContain('public.public.users');
  });

  it('handles ResolvedSchemaTable', () => {
    const table: ResolvedSchemaTable = {
      name: 'users',
      schema: 'public',
      columns: [],
      origin: 'imported',
      updatedAt: '2025-01-01T00:00:00Z',
    };
    const keys = collectTableLookupKeys(table);
    expect(keys).toContain('users');
    expect(keys).toContain('public.users');
  });
});

describe('resolveForeignKeyTarget', () => {
  it('resolves exact match', () => {
    const lookup = new Map([['users', 'users']]);
    expect(resolveForeignKeyTarget('users', lookup)).toBe('users');
  });

  it('resolves with suffix matching', () => {
    const lookup = new Map([['users', 'users']]);
    expect(resolveForeignKeyTarget('public.users', lookup)).toBe('users');
  });

  it('resolves fully qualified to simple', () => {
    const lookup = new Map([['users', 'users']]);
    expect(resolveForeignKeyTarget('catalog.schema.users', lookup)).toBe('users');
  });

  it('prefers longer match when available', () => {
    const lookup = new Map([
      ['users', 'users'],
      ['public.users', 'users'],
    ]);
    expect(resolveForeignKeyTarget('public.users', lookup)).toBe('users');
  });

  it('strips quotes from foreign key reference', () => {
    const lookup = new Map([['users', 'users']]);
    expect(resolveForeignKeyTarget('"public"."users"', lookup)).toBe('users');
  });

  it('returns null for non-existent table', () => {
    const lookup = new Map([['orders', 'orders']]);
    expect(resolveForeignKeyTarget('users', lookup)).toBeNull();
  });

  it('returns null for empty string', () => {
    const lookup = new Map([['users', 'users']]);
    expect(resolveForeignKeyTarget('', lookup)).toBeNull();
  });

  it('handles mixed case table names', () => {
    const lookup = new Map([['Users', 'Users']]);
    expect(resolveForeignKeyTarget('Users', lookup)).toBe('Users');
  });
});
