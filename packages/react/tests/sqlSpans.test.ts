import { describe, expect, it, vi, afterEach } from 'vitest';

import { spanToCharRange, trySpanToCharRange } from '../src/utils/sqlSpans';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('sqlSpans', () => {
  it('converts UTF-8 byte offsets to CodeMirror character offsets', () => {
    const sql = "SELECT '日本語'";

    expect(spanToCharRange(sql, { start: 8, end: 17 })).toEqual({
      from: 8,
      to: 11,
    });
  });

  it('returns null instead of throwing for stale spans', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);

    expect(trySpanToCharRange('SELECT 1', { start: 0, end: 999 }, 'active highlight')).toBeNull();
    expect(warn).toHaveBeenCalledTimes(1);
  });
});
