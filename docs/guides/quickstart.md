# Quickstart: TypeScript Usage

This guide shows how to use FlowScope in a TypeScript project.

## Installation

```bash
npm install @pondpilot/flowscope-core
# or
yarn add @pondpilot/flowscope-core
```

## Basic Usage

```typescript
import { initWasm, analyzeSql } from '@pondpilot/flowscope-core';

// Initialize WASM (call once at app startup)
await initWasm();

// Analyze SQL
const result = await analyzeSql({
  sql: 'SELECT * FROM users JOIN orders ON users.id = orders.user_id',
  dialect: 'postgres',
});

// Access lineage data
console.log('Statements:', result.statements.length);
console.log('Tables:', result.statements[0].nodes.filter(n => n.type === 'table'));
console.log('Issues:', result.issues);
```

## With Schema Metadata

Providing schema metadata enables table validation:

```typescript
const result = await analyzeSql({
  sql: 'SELECT id, name FROM users',
  dialect: 'postgres',
  schema: {
    defaultSchema: 'public',
    tables: [
      {
        name: 'public.users',
        columns: [{ name: 'id' }, { name: 'name' }, { name: 'email' }],
      },
    ],
  },
});
```

## Handling Results

```typescript
// Check for errors
if (result.summary.hasErrors) {
  console.error('Analysis failed:', result.issues.filter(i => i.severity === 'error'));
}

// Iterate over statements
for (const stmt of result.statements) {
  console.log(`Statement ${stmt.statementIndex}: ${stmt.statementType}`);

  // Get tables
  const tables = stmt.nodes.filter(n => n.type === 'table');
  console.log('Tables:', tables.map(t => t.label));

  // Get edges (data flow)
  console.log('Edges:', stmt.edges.length);
}

// Access global lineage (cross-statement)
console.log('Global nodes:', result.globalLineage.nodes.length);
console.log('Global edges:', result.globalLineage.edges.length);
```

## Error Handling

```typescript
try {
  const result = await analyzeSql({ sql, dialect: 'postgres' });

  // Analysis completed - check for issues
  if (result.summary.hasErrors) {
    // Parse errors or unsupported syntax
    for (const issue of result.issues) {
      console.log(`[${issue.severity}] ${issue.code}: ${issue.message}`);
    }
  }
} catch (error) {
  // Technical error (WASM failed to load, etc.)
  console.error('Failed to analyze:', error);
}
```

## Column-Level Lineage

FlowScope tracks column-level data flow. Columns are created as nodes with `type: 'column'`:

```typescript
const result = await analyzeSql({
  sql: 'SELECT u.id, u.name AS user_name FROM users u',
  dialect: 'postgres',
});

// Get column nodes
const columns = result.statements[0].nodes.filter(n => n.type === 'column');
console.log('Columns:', columns.map(c => c.label));
// Output: ['id', 'user_name']

// Get data flow edges (column to column)
const dataFlowEdges = result.statements[0].edges.filter(e => e.type === 'data_flow');
console.log('Data flow edges:', dataFlowEdges.length);

// Get derivation edges (for computed columns)
const derivationEdges = result.statements[0].edges.filter(e => e.type === 'derivation');
```

### Disabling Column Lineage

For performance, you can disable column lineage:

```typescript
const result = await analyzeSql({
  sql: 'SELECT * FROM users',
  dialect: 'postgres',
  options: {
    enableColumnLineage: false,
  },
});
```

### SELECT * Expansion

When schema metadata is provided, `SELECT *` is expanded to individual columns:

```typescript
const result = await analyzeSql({
  sql: 'SELECT * FROM users',
  dialect: 'postgres',
  schema: {
    tables: [
      {
        name: 'users',
        columns: [
          { name: 'id', dataType: 'integer' },
          { name: 'name', dataType: 'varchar' },
          { name: 'email', dataType: 'varchar' },
        ],
      },
    ],
  },
});

// Columns will include id, name, email
const columns = result.statements[0].nodes.filter(n => n.type === 'column');
```

## Supported Dialects

- `generic` - Standard SQL
- `postgres` - PostgreSQL
- `snowflake` - Snowflake
- `bigquery` - Google BigQuery
