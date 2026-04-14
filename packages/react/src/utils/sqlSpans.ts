import { byteOffsetToCharOffset, type Span } from '@pondpilot/flowscope-core';

export interface CharRange {
  from: number;
  to: number;
}

/**
 * Convert a span's UTF-8 byte offsets (the analyzer's coordinate system) to
 * character offsets (CodeMirror's coordinate system).
 */
export function spanToCharRange(sql: string, span: Span): CharRange {
  return {
    from: byteOffsetToCharOffset(sql, span.start),
    to: byteOffsetToCharOffset(sql, span.end),
  };
}

/**
 * Best-effort wrapper for UI rendering paths that may briefly hold stale spans
 * while the surrounding SQL text is switching files or waiting for re-analysis.
 */
export function trySpanToCharRange(sql: string, span: Span, label = 'span'): CharRange | null {
  try {
    return spanToCharRange(sql, span);
  } catch (error) {
    if (process.env.NODE_ENV !== 'production') {
      console.warn(`[SqlView] Ignoring stale ${label} for current SQL text.`, error);
    }
    return null;
  }
}
