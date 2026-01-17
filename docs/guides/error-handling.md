# Error Handling Guide

FlowScope returns structured issues for errors, warnings, and informational messages.

## Issue Structure

```typescript
interface Issue {
  severity: 'error' | 'warning' | 'info';
  code: string;
  message: string;
  span?: { start: number; end: number };
  statementIndex?: number;
}
```

## Common Codes

- `PARSE_ERROR`, `INVALID_REQUEST`
- `UNSUPPORTED_SYNTAX`, `UNKNOWN_TABLE`, `UNKNOWN_COLUMN`
- `APPROXIMATE_LINEAGE`, `UNRESOLVED_REFERENCE`, `SCHEMA_CONFLICT`

See `docs/error-codes.md` for the full list.

## Handling Errors

```typescript
const result = await analyzeSql({ sql, dialect: 'postgres' });

if (result.summary.hasErrors) {
  const errors = result.issues.filter((i) => i.severity === 'error');
  console.error('Analysis errors:', errors);
}
```

## Partial Results

FlowScope returns partial results whenever possible:

- A statement that fails to parse contributes a `PARSE_ERROR` issue.
- Other statements continue to produce lineage.
- `APPROXIMATE_LINEAGE` indicates reduced accuracy (usually missing schema).

## Span Highlighting

```typescript
for (const issue of result.issues) {
  if (issue.span) {
    const snippet = sql.slice(issue.span.start, issue.span.end);
    console.log('Problem code:', snippet);
  }
}
```
