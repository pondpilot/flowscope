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
import {
  analyzeSql,
  edgesInStatement,
  initWasm,
  nodesInStatement,
} from '@pondpilot/flowscope-core';

await initWasm();

const result = await analyzeSql({
  sql: 'SELECT * FROM users JOIN orders ON users.id = orders.user_id',
  dialect: 'postgres',
});

console.log('Statements:', result.statements.length);
console.log('Issues:', result.issues);
```

## With Schema Metadata

```typescript
const result = await analyzeSql({
  sql: 'SELECT id, name FROM users',
  dialect: 'postgres',
  schema: {
    defaultSchema: 'public',
    tables: [
      {
        schema: 'public',
        name: 'users',
        columns: [{ name: 'id' }, { name: 'name' }, { name: 'email' }],
      },
    ],
  },
});
```

## Handling Results

```typescript
if (result.summary.hasErrors) {
  console.error('Analysis failed:', result.issues);
}

for (const statement of result.statements) {
  const nodes = nodesInStatement(result, statement.statementIndex);
  const edges = edgesInStatement(result, statement.statementIndex);

  console.log(`Statement ${statement.statementIndex}: ${statement.statementType}`);
  console.log('Nodes:', nodes.length);
  console.log('Edges:', edges.length);
}

console.log('All graph nodes:', result.nodes.length);
console.log('All graph edges:', result.edges.length);
```

## Disabling Column Lineage

```typescript
const result = await analyzeSql({
  sql: 'SELECT * FROM users',
  dialect: 'postgres',
  options: { enableColumnLineage: false },
});
```

## Supported Dialects

- `generic`, `ansi`, `bigquery`, `clickhouse`, `databricks`, `duckdb`, `hive`
- `mssql`, `mysql`, `oracle`, `postgres`, `redshift`, `snowflake`, `sqlite`
