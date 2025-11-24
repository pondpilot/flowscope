# Error Handling Guide

FlowScope uses a structured issue system for reporting errors, warnings, and informational messages.

## Issue Structure

```typescript
interface Issue {
  // Severity level
  severity: 'error' | 'warning' | 'info';

  // Machine-readable code
  code: string;

  // Human-readable message
  message: string;

  // Source location (optional)
  span?: {
    start: number;  // Byte offset
    end: number;    // Byte offset
  };

  // Associated statement index (optional)
  statementIndex?: number;
}
```

## Issue Codes

### Errors

| Code | Description |
|------|-------------|
| `PARSE_ERROR` | SQL syntax error - the parser could not understand the input |
| `REQUEST_PARSE_ERROR` | Invalid JSON request format |
| `SERIALIZATION_ERROR` | Failed to serialize result (internal error) |

### Warnings

| Code | Description |
|------|-------------|
| `UNKNOWN_TABLE` | Table referenced in SQL not found in schema |
| `UNSUPPORTED_SYNTAX` | SQL construct recognized but not fully supported |
| `APPROXIMATE_LINEAGE` | Lineage is approximate due to `SELECT *` without schema |

### Info

| Code | Description |
|------|-------------|
| `DIALECT_FALLBACK` | Requested dialect unavailable, using fallback |

## Handling Errors

```typescript
const result = await analyzeSql({ sql, dialect: 'postgres' });

// Check summary for quick error detection
if (result.summary.hasErrors) {
  console.error('Analysis has errors');
}

// Group issues by severity
const errors = result.issues.filter(i => i.severity === 'error');
const warnings = result.issues.filter(i => i.severity === 'warning');
const infos = result.issues.filter(i => i.severity === 'info');

// Handle specific error codes
for (const issue of result.issues) {
  switch (issue.code) {
    case 'PARSE_ERROR':
      console.error(`Syntax error: ${issue.message}`);
      break;
    case 'UNKNOWN_TABLE':
      console.warn(`Unknown table: ${issue.message}`);
      break;
    default:
      console.log(`[${issue.severity}] ${issue.code}: ${issue.message}`);
  }
}
```

## Partial Results

FlowScope continues analysis even when errors occur:

- If one statement fails to parse, other statements are still analyzed
- Unsupported syntax generates warnings but doesn't stop analysis
- The result always contains whatever lineage could be extracted

```typescript
const result = await analyzeSql({
  sql: `
    SELECT * FROM users;
    INVALID SYNTAX HERE;
    SELECT * FROM orders;
  `,
  dialect: 'postgres',
});

// statements[0] and statements[2] will have lineage
// statements[1] will be empty with PARSE_ERROR issue
console.log('Analyzed statements:', result.statements.length);
console.log('Errors:', result.issues.filter(i => i.severity === 'error').length);
```

## Using Spans for Highlighting

When `span` is available, use it to highlight the problematic code:

```typescript
for (const issue of result.issues) {
  if (issue.span) {
    const problemCode = sql.substring(issue.span.start, issue.span.end);
    console.log(`Problem at: "${problemCode}"`);
  }
}
```
