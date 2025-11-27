/**
 * Utilities for parsing and normalizing SQL identifier names.
 *
 * These functions handle the complexity of qualified SQL identifiers which may include:
 * - Multiple parts (catalog.schema.table)
 * - Various quoting styles (double quotes, backticks, square brackets)
 * - Escaped quotes within identifiers
 *
 * Limitations:
 * - Some dialect-specific escaping rules may not be fully supported
 *   (e.g., doubled backticks in MySQL for escaped backticks)
 * - Unicode identifiers are supported but not validated
 */

import type { SchemaTable, ResolvedSchemaTable } from '@pondpilot/flowscope-core';

/**
 * Strip quote characters from a single identifier segment.
 *
 * Handles double quotes ("identifier"), backticks (`identifier`),
 * and square brackets ([identifier]).
 */
export function stripIdentifierQuotes(segment: string): string {
  const trimmed = segment.trim();
  if (trimmed.length < 2) return trimmed;
  const first = trimmed[0];
  const last = trimmed[trimmed.length - 1];
  if (
    (first === '"' && last === '"') ||
    (first === '`' && last === '`') ||
    (first === '[' && last === ']')
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

/**
 * Split a qualified identifier (e.g., "catalog.schema.table") into its parts.
 *
 * Correctly handles quoted identifiers that may contain dots.
 * Supports double quotes, backticks, and square brackets.
 * Handles escaped quotes (doubled quotes) within identifiers.
 *
 * @example
 * splitQualifiedIdentifier('public.users') // ['public', 'users']
 * splitQualifiedIdentifier('"my.schema"."my.table"') // ['"my.schema"', '"my.table"']
 * splitQualifiedIdentifier('[catalog].[schema].[table]') // ['[catalog]', '[schema]', '[table]']
 */
export function splitQualifiedIdentifier(identifier: string): string[] {
  const parts: string[] = [];
  let current = '';
  let quoteChar: '"' | '`' | ']' | null = null;

  for (let i = 0; i < identifier.length; i += 1) {
    const char = identifier[i];

    // Enter quote mode
    if (!quoteChar && (char === '"' || char === '`')) {
      quoteChar = char;
      current += char;
      continue;
    }

    if (!quoteChar && char === '[') {
      quoteChar = ']';
      current += char;
      continue;
    }

    // Inside quoted identifier
    if (quoteChar) {
      current += char;

      // Handle escaped double quotes (doubled)
      if (quoteChar === '"' && char === '"' && identifier[i + 1] === '"') {
        current += identifier[i + 1];
        i += 1;
        continue;
      }

      // Handle escaped square brackets (doubled)
      if (quoteChar === ']' && char === ']' && identifier[i + 1] === ']') {
        current += identifier[i + 1];
        i += 1;
        continue;
      }

      // Exit quote mode
      if (char === quoteChar) {
        quoteChar = null;
      }

      continue;
    }

    // Unquoted dot is a separator
    if (char === '.') {
      if (current.trim()) {
        parts.push(current.trim());
      }
      current = '';
      continue;
    }

    current += char;
  }

  if (current.trim()) {
    parts.push(current.trim());
  }

  return parts;
}

/**
 * Normalize a qualified name by splitting and stripping quotes from each part.
 *
 * Returns null for empty/invalid input.
 * The result is a dot-joined string of unquoted parts.
 *
 * @example
 * normalizeQualifiedName('public.users') // 'public.users'
 * normalizeQualifiedName('"public"."users"') // 'public.users'
 * normalizeQualifiedName('  ') // null
 */
export function normalizeQualifiedName(value?: string | null): string | null {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const segments = splitQualifiedIdentifier(trimmed)
    .map((segment) => stripIdentifierQuotes(segment))
    .filter((segment) => segment.length > 0);
  if (segments.length === 0) return null;
  return segments.join('.');
}

/**
 * Collect all possible lookup keys for a table.
 *
 * Generates variations of the table name that might be used to reference it:
 * - Just the table name
 * - schema.table (if schema is provided)
 * - catalog.table (if catalog is provided)
 * - catalog.schema.table (if both are provided)
 *
 * All keys are normalized to strip quotes.
 */
export function collectTableLookupKeys(table: SchemaTable | ResolvedSchemaTable): string[] {
  const keys = new Set<string>();

  const register = (candidate?: string | null) => {
    const normalized = normalizeQualifiedName(candidate);
    if (normalized) {
      keys.add(normalized);
    }
  };

  register(table.name);

  const hasQualifier = table.name.includes('.');
  if (!hasQualifier && table.schema) {
    register(`${table.schema}.${table.name}`);
  }

  if (!hasQualifier && table.catalog) {
    register(`${table.catalog}.${table.name}`);
  }

  if (!hasQualifier && table.catalog && table.schema) {
    register(`${table.catalog}.${table.schema}.${table.name}`);
  }

  return Array.from(keys);
}

/**
 * Resolve a foreign key target table name to an actual table in the lookup.
 *
 * Tries progressively shorter suffixes to find a match. For example, if the FK
 * references "catalog.schema.users", it will try:
 * 1. catalog.schema.users
 * 2. schema.users
 * 3. users
 *
 * Note: This may match incorrectly if multiple tables have the same short name
 * (e.g., both sales.users and hr.users exist). In such cases, the first match
 * in the lookup map is returned, which is non-deterministic.
 *
 * @returns The table name from the lookup map, or null if not found.
 */
export function resolveForeignKeyTarget(
  foreignTable: string,
  tableLookup: Map<string, string>
): string | null {
  const normalized = normalizeQualifiedName(foreignTable);
  if (!normalized) return null;

  const parts = normalized.split('.');
  for (let i = 0; i < parts.length; i += 1) {
    const candidate = parts.slice(i).join('.');
    const match = tableLookup.get(candidate);
    if (match) {
      return match;
    }
  }

  return null;
}
