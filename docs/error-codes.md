# Error Code Catalog

This document lists the issue codes emitted by FlowScope. Codes are defined in:
- `crates/flowscope-core/src/types/common.rs`
- `packages/core/src/types.ts`

## Error Codes

| Code | Severity | Notes |
| --- | --- | --- |
| `PARSE_ERROR` | Error | SQL syntax error; statement lineage skipped. |
| `INVALID_REQUEST` | Error | Request payload invalid or missing required fields. |
| `MEMORY_LIMIT_EXCEEDED` | Error | Reserved for host/runtime memory limits. |

## Warning Codes

| Code | Severity | Notes |
| --- | --- | --- |
| `UNSUPPORTED_SYNTAX` | Warning | Statement parsed but not fully supported. |
| `UNKNOWN_TABLE` | Warning | Table reference not resolved from schema. |
| `UNKNOWN_COLUMN` | Warning | Column reference not resolved from schema. |
| `UNRESOLVED_REFERENCE` | Warning | Cross-statement reference not produced earlier. |
| `SCHEMA_CONFLICT` | Warning | Imported schema conflicts with implied schema. |
| `TYPE_MISMATCH` | Warning | Type incompatibility detected in expression (e.g., comparing INTEGER to TEXT). |
| `PAYLOAD_SIZE_WARNING` | Warning | Reserved for large payload warnings. |

## Info Codes

| Code | Severity | Notes |
| --- | --- | --- |
| `APPROXIMATE_LINEAGE` | Info | Lineage is approximate due to missing schema. |
| `DIALECT_FALLBACK` | Info | Reserved for dialect fallback behavior. |
| `CANCELLED` | Info | Reserved for host-initiated cancellation. |

## Deprecated Codes

| Code | Status |
| --- | --- |
| `UNSUPPORTED_RECURSIVE_CTE` | Deprecated (recursive CTEs are supported). |

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
