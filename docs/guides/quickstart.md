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

for (const stmt of result.statements) {
  console.log(`Statement ${stmt.statementIndex}: ${stmt.statementType}`);
  console.log('Edges:', stmt.edges.length);
}

console.log('Global nodes:', result.globalLineage.nodes.length);
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
- `mssql`, `mysql`, `postgres`, `redshift`, `snowflake`, `sqlite`
