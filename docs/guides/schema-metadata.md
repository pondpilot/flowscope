# Schema Metadata Format

FlowScope accepts optional schema metadata to improve lineage accuracy and enable validation.

## Basic Structure

```typescript
interface SchemaMetadata {
  defaultCatalog?: string;
  defaultSchema?: string;
  searchPath?: SchemaNamespaceHint[];
  caseSensitivity?: 'dialect' | 'lower' | 'upper' | 'exact';
  tables?: SchemaTable[];
  allowImplied?: boolean;
}
```

## Table Definitions

```typescript
interface SchemaTable {
  catalog?: string;
  schema?: string;
  name: string;
  columns?: ColumnSchema[];
}

interface ColumnSchema {
  name: string;
  dataType?: string;
  isPrimaryKey?: boolean;
  foreignKey?: { table: string; column: string };
}
```

## Example: PostgreSQL

```typescript
const schema: SchemaMetadata = {
  defaultSchema: 'public',
  tables: [
    {
      schema: 'public',
      name: 'users',
      columns: [{ name: 'id' }, { name: 'name' }, { name: 'email' }],
    },
  ],
};
```

## Example: Snowflake

```typescript
const schema: SchemaMetadata = {
  defaultCatalog: 'MY_DATABASE',
  defaultSchema: 'ANALYTICS',
  caseSensitivity: 'upper',
  tables: [
    {
      catalog: 'MY_DATABASE',
      schema: 'ANALYTICS',
      name: 'USERS',
      columns: [{ name: 'ID' }, { name: 'NAME' }, { name: 'EMAIL' }],
    },
  ],
};
```

## Validation Behavior

- Known tables/columns are validated against the schema.
- Unknown tables/columns emit `UNKNOWN_TABLE` / `UNKNOWN_COLUMN` warnings.
- Missing schema results in best-effort lineage and `APPROXIMATE_LINEAGE` when needed.
