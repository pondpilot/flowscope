# Schema Metadata Format

FlowScope accepts optional schema metadata to improve lineage accuracy and enable table/column validation.

## Basic Structure

```typescript
interface SchemaMetadata {
  // Default catalog for unqualified names (optional)
  defaultCatalog?: string;

  // Default schema for unqualified names (optional)
  defaultSchema?: string;

  // Case sensitivity setting (optional)
  caseSensitivity?: 'preserve' | 'lower' | 'upper';

  // Table definitions
  tables: SchemaTable[];
}

interface SchemaTable {
  // Fully qualified table name (e.g., "public.users" or "catalog.schema.table")
  name: string;

  // Column definitions
  columns: ColumnSchema[];
}

interface ColumnSchema {
  // Column name
  name: string;

  // Data type (optional, for future use)
  dataType?: string;
}
```

## Example: PostgreSQL

```typescript
const schema: SchemaMetadata = {
  defaultSchema: 'public',
  tables: [
    {
      name: 'public.users',
      columns: [
        { name: 'id' },
        { name: 'name' },
        { name: 'email' },
        { name: 'created_at' },
      ],
    },
    {
      name: 'public.orders',
      columns: [
        { name: 'order_id' },
        { name: 'user_id' },
        { name: 'total' },
        { name: 'order_date' },
      ],
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
      name: 'MY_DATABASE.ANALYTICS.USERS',
      columns: [
        { name: 'ID' },
        { name: 'NAME' },
        { name: 'EMAIL' },
      ],
    },
  ],
};
```

## Example: BigQuery

```typescript
const schema: SchemaMetadata = {
  defaultCatalog: 'my-project',
  defaultSchema: 'my_dataset',
  tables: [
    {
      name: 'my-project.my_dataset.users',
      columns: [
        { name: 'id' },
        { name: 'name' },
        { name: 'email' },
      ],
    },
  ],
};
```

## Validation Behavior

When schema is provided:
- Tables referenced in SQL are validated against the schema
- Unknown tables generate `UNKNOWN_TABLE` warnings
- Column validation is available for column-level lineage

When schema is not provided:
- FlowScope extracts table names from SQL as-is
- No validation is performed
- `SELECT *` cannot be expanded
