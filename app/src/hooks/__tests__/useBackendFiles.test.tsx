import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('@pondpilot/flowscope-core', () => ({
  VALID_DIALECTS: ['generic', 'ansi'],
}));

import { useBackendFiles } from '../useBackendFiles';

describe('useBackendFiles', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('preserves the file snapshot across unchanged polling responses', async () => {
    let sql = 'SELECT 1';
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const data = url.endsWith('/api/files')
          ? [{ name: 'query.sql', content: sql }]
          : url.endsWith('/api/config')
            ? { dialect: 'generic', watch_dirs: [], has_schema: false }
            : null;
        return {
          ok: true,
          json: async () => data,
        } as Response;
      })
    );

    const { result, unmount } = renderHook(() => useBackendFiles(true));
    await waitFor(() =>
      expect(result.current.files).toEqual([{ name: 'query.sql', content: sql }])
    );
    const firstSnapshot = result.current.files;

    await act(async () => result.current.refresh());
    expect(result.current.files).toBe(firstSnapshot);

    sql = 'SELECT 2';
    await act(async () => result.current.refresh());
    expect(result.current.files).not.toBe(firstSnapshot);
    expect(result.current.files?.[0].content).toBe('SELECT 2');
    unmount();
  });
});
