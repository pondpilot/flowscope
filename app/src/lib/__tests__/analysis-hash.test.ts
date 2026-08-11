import { describe, expect, it } from 'vitest';

import { buildFileSyncKey } from '../analysis-hash';

describe('buildFileSyncKey', () => {
  it('changes when same-length file content changes', () => {
    const before = buildFileSyncKey({
      files: [{ name: 'query.sql', content: 'SELECT 1' }],
    });
    const after = buildFileSyncKey({
      files: [{ name: 'query.sql', content: 'SELECT 2' }],
    });
    const restored = buildFileSyncKey({
      files: [{ name: 'query.sql', content: 'SELECT 1' }],
    });

    expect(after).not.toBe(before);
    expect(restored).toBe(before);
  });

  it('is stable for unchanged files', () => {
    const files = [
      { name: 'models/orders.sql', content: 'SELECT * FROM raw_orders' },
      { name: 'models/customers.sql', content: 'SELECT * FROM raw_customers' },
    ];

    const first = buildFileSyncKey({ files });
    const second = buildFileSyncKey({
      files: files.map((file) => ({ ...file })),
    });

    expect(second).toBe(first);
  });
});
