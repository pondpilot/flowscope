/**
 * Schema SQL Parser - Extracts schema from CREATE TABLE statements
 * Uses the WASM core to parse SQL and extract table/column information
 */

import type { Dialect } from './project-store';
import type {
  SchemaTable,
  AnalyzeRequest,
  AnalyzeResult,
  ColumnSchema,
  StatementLineage,
  Issue,
  StatementRef,
} from '@pondpilot/flowscope-core';
import { SCHEMA_LIMITS } from './constants';

export interface ParsedSchema {
  tables: SchemaTable[];
  errors: string[];
}

// Keep the last parse result for debugging
let lastParseResult: AnalyzeResult | null = null;

/**
 * Get the last parse result for debugging purposes.
 * Returns null if the result has been garbage collected.
 */
export function getLastParseResult(): AnalyzeResult | null {
  return lastParseResult;
}

/**
 * Parse CREATE TABLE statements using the SQL parser
 * With the updated WASM, resolvedSchema should now be returned for DDL statements
 */
export async function parseSchemaSQL(
  schemaSQL: string,
  dialect: Dialect,
  analyzeFunction: (params: AnalyzeRequest) => Promise<AnalyzeResult>
): Promise<ParsedSchema> {
  if (!schemaSQL.trim()) {
    return { tables: [], errors: [] };
  }

  const errors: string[] = [];
  const tables: SchemaTable[] = [];

  // Validate schema size
  if (schemaSQL.length > SCHEMA_LIMITS.MAX_SIZE) {
    const sizeMB = (schemaSQL.length / 1024 / 1024).toFixed(2);
    const maxSizeMB = (SCHEMA_LIMITS.MAX_SIZE / 1024 / 1024).toFixed(0);
    errors.push(`Schema SQL is too large (${sizeMB}MB). Maximum size is ${maxSizeMB}MB.`);
    return { tables, errors };
  }

  try {
    // Analyze the schema SQL to parse CREATE TABLE statements
    const result = await analyzeFunction({
      sql: '',
      files: [{ name: 'schema.sql', content: schemaSQL }],
      dialect,
      schema: {
        allowImplied: true,
      },
    });

    // Capture result for debug view
    lastParseResult = result;

    // Extract tables from resolvedSchema (now available after WASM rebuild)
    if (result.resolvedSchema?.tables) {
      for (const table of result.resolvedSchema.tables) {
        tables.push({
          catalog: table.catalog,
          schema: table.schema,
          name: table.name,
          columns: table.columns.map((col: ColumnSchema) => ({
            name: col.name,
            dataType: col.dataType,
          })),
        });
      }
    } else {
      // Fallback: Extract from global lineage nodes (includes canonical names) when resolvedSchema is absent
      const createStatementIndexes = new Set(
        (result.statements || [])
          .filter((stmt: StatementLineage) => stmt.statementType === 'CREATE_TABLE')
          .map((stmt: StatementLineage) => stmt.statementIndex)
      );

      const tableMap = new Map<string, SchemaTable>();

      for (const node of result.globalLineage?.nodes || []) {
        // Only consider nodes that belong to CREATE TABLE statements from the schema SQL
        const isFromSchemaDDL = node.statementRefs?.some((ref: StatementRef) =>
          createStatementIndexes.has(ref.statementIndex)
        );
        if (!isFromSchemaDDL) continue;

        const canonical = node.canonicalName;
        const key = [canonical.catalog, canonical.schema, canonical.name].filter(Boolean).join('.');

        if (node.type === 'table') {
          if (!tableMap.has(key)) {
            tableMap.set(key, {
              catalog: canonical.catalog,
              schema: canonical.schema,
              name: canonical.name,
              columns: [],
            });
          }
        } else if (node.type === 'column') {
          const columnName = canonical.column || node.label;
          if (!columnName) continue;

          const table =
            tableMap.get(key) ||
            (() => {
              const newTable: SchemaTable = {
                catalog: canonical.catalog,
                schema: canonical.schema,
                name: canonical.name,
                columns: [],
              };
              tableMap.set(key, newTable);
              return newTable;
            })();

          table.columns = table.columns || [];

          // Avoid duplicate columns when the same column appears multiple times
          if (!table.columns.some((col) => col.name === columnName)) {
            table.columns.push({
              name: columnName,
              dataType: undefined, // Type info not available in lineage nodes
            });
          }
        }
      }

      tables.push(...Array.from(tableMap.values()));
    }

    // Collect parsing errors
    if (result.issues?.length > 0) {
      const errorIssues = result.issues.filter((i: Issue) => i.severity === 'error');
      errors.push(...errorIssues.map((i: Issue) => i.message));
    }

    if (tables.length === 0 && errors.length === 0) {
      errors.push('No CREATE TABLE statements found in schema SQL');
    }
  } catch (error) {
    errors.push(
      `Failed to parse schema SQL: ${error instanceof Error ? error.message : String(error)}`
    );
  }

  return { tables, errors };
}
